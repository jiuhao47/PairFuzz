use clap::Parser;
use libafl::monitors::OnDiskJsonMonitor;
use libafl::{
    corpus::{Corpus, InMemoryOnDiskCorpus},
    events::SimpleRestartingEventManager,
    executors::{inprocess::InProcessExecutor, DiffExecutor},
    feedback_and_fast,
    feedback_or_fast,
    // feedbacks::MaxMapFeedback,
    fuzzer::{Fuzzer, StdFuzzer},
    generators::RandPrintablesGenerator,
    inputs::BytesInput,
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    schedulers::QueueScheduler,
    stages::mutational::StdMutationalStage,
    state::{HasCorpus, StdState},
};
// Note: we intentionally avoid MinMapFeedback here to keep concrete feedback types
// identical across branches; we emulate Min via inverted owned map.
use libafl_bolts::shmem::{ShMemProvider, StdShMemProvider};
use libafl_bolts::{nonzero, rands::StdRand, tuples::tuple_list};
use libafl_targets::edges_max_num;
use mimalloc::MiMalloc;
use std::{fs, path::PathBuf, time::Duration};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

// bindings to the functions defined in the target
mod bindings {
    #![allow(non_snake_case)]
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(unused)]
    #![allow(dead_code)]
    #![allow(clippy::unreadable_literal)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

// Custom observers
mod driver;
mod feedbacks;
mod observers;
mod utils;

use observers::ReturnValueObserver;
// debug_log is provided by utils if needed
use crate::driver::args::Args;
// ...existing imports...

mod slicemap {
    // EDGES storage moved into observers helper; no static here
}
// observers and map observer types are constructed via observers::make_map_observers

mod harness;

#[expect(clippy::too_many_lines)]
pub fn main() {
    // 解析命令行参数（为了获取 -v 计数以配置日志级别）
    let args = Args::parse();

    // 初始化日志（集中到 utils::logging）
    crate::utils::logging::init_from_verbose(args.verbose, None);
    unsafe { std::env::set_var("ASAN_OPTIONS", "abort_on_error=1:detect_leaks=false") }

    // 设置日志目录
    if let Some(log_dir) = &args.log_dir {
        // 创建日志目录（如果不存在）
        if let Err(e) = fs::create_dir_all(log_dir) {
            eprintln!("无法创建日志目录 {:?}: {}", log_dir, e);
            std::process::exit(1);
        }

        // 打开或创建主日志文件 diffray.log
        let log_path = log_dir.join("diffray.log");
        let _log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .expect("无法创建或打开日志文件");

        if args.verbose > 0 {
            println!("日志将写入到目录: {:?} (主日志: {:?})", log_dir, log_path);
        }
    } else if args.verbose > 0 {
        println!("启用详细输出模式");
    }

    // 创建输出目录
    if let Err(e) = fs::create_dir_all(&args.output_dir) {
        eprintln!("无法创建输出目录 {:?}: {}", args.output_dir, e);
        std::process::exit(1);
    }

    if args.verbose > 0 {
        println!("配置参数:");
        println!("  输入目录: {:?}", args.input_dir);
        println!("  输出目录: {:?}", args.output_dir);
        println!("  日志目录: {:?}", args.log_dir);
        println!("  执行超时时间: {}毫秒", args.timeout_ms);
    }

    // unlimited fuzzing, no time or iteration limits
    let mut harness_impl_a = |input: &BytesInput| crate::harness::first_harness(input);
    let mut harness_impl_b = |input: &BytesInput| crate::harness::second_harness(input);
    let num_edges: usize = edges_max_num(); // upper bound

    // Build observers using helper which handles EDGES allocation
    let (map_observer_a, map_observer_b, objective_observer) =
        observers::coverage_helpers::make_map_observers(num_edges);

    // startup observer initial-value logging removed (not useful)

    // create a map swapper from the two map observers (requires the swapper to borrow them)
    let mut map_observer_a = map_observer_a;
    let mut map_observer_b = map_observer_b;
    let map_swapper =
        crate::feedbacks::coverage::make_map_swapper(&mut map_observer_a, &mut map_observer_b);

    // Feedback to rate the interestingness of an input
    // create return-value observers for the return-value feedback (per-implementation)
    // Note: these are *feedback-side* placeholders and should not be used to build
    // the objective that will be evaluated with executor observers. We instead
    // create executor-attached observers below and use their handles for the
    // ReturnValueObjective so the handles match at runtime.
    let rv_feedback_observer_a = ReturnValueObserver::new("first_return");
    let rv_feedback_observer_b = ReturnValueObserver::new("second_return");

    // Compose map feedback with return-value feedback according to selected modes
    // Note: the wrapper here can still reference the feedback-side observers
    // (used for State initialization), but the objective must be built using
    // the executor-attached observers handles so they resolve at runtime.
    let rv_wrapper = crate::driver::feedbacks::make_rv_wrapper(
        args.rv_mode,
        &rv_feedback_observer_a,
        &rv_feedback_observer_b,
    );

    // build coverage feedback pair according to selected mode
    let (coverage_fb_a, coverage_fb_b) =
        crate::driver::feedbacks::make_cov_wrapper(args.cov_mode, &map_observer_a, &map_observer_b);

    // Defer composing the combined feedback until after we've created
    // `state` and had a chance to initialize coverage feedback metadata.

    // Compose coverage feedback (must exist before creating State so the State
    // can capture feedbacks' persistent metadata during construction).
    let coverage_fb = feedback_and_fast!(coverage_fb_a, coverage_fb_b);
    let mut feedback = feedback_or_fast!(coverage_fb, rv_wrapper);

    // create return-value observers that are attached to executors (per-implementation)
    let rv_exec_observer_a = ReturnValueObserver::new("first_return");
    let rv_exec_observer_b = ReturnValueObserver::new("second_return");

    // Build objectives: crash-any, return-value (using executor-attached observers),
    // and the existing diff+map objective; consider an input interesting if any of
    // these objectives are satisfied.
    // Treat any crash as a solution: use CrashAnyObjective directly so append_metadata
    // will be invoked when a crash is detected. Previously we combined the crash
    // objective with a MaxMapFeedback which could make the overall objective false
    // (if no new coverage was detected) and thus skip append_metadata.
    // Single combined objective: prefer crash detection (short-circuit), otherwise
    // fall back to return-value diffs. Use executor-attached observers for rv checks.
    let diffray_obj = crate::feedbacks::DiffRayObjective::new(
        "zero_nonzero",
        &[&rv_exec_observer_a, &rv_exec_observer_b],
    );
    // include a small coverage feedback for rv objective as befores
    let mut objective = diffray_obj; // use the combined single objective

    // NOTE: State creation is deferred until after we attempt to launch
    // a restarting event manager. The restarter may provide an existing
    // State via shared memory if we are resuming after a crash.

    // The Monitor trait define how the fuzzer stats are displayed to the user
    // Use a function pointer for the log predicate so the monitor implements `Debug`.
    fn always_log(_: &mut libafl::monitors::stats::ClientStatsManager) -> bool {
        true
    }

    let monitors = {
        // place the fuzzer JSON stats inside the log directory if provided
        let json_log_path = args
            .log_dir
            .as_ref()
            .map(|d| d.join("fuzzer_stats.json"))
            .unwrap_or_else(|| PathBuf::from("fuzzer_stats.json"));
        let json_mon = OnDiskJsonMonitor::new(
            json_log_path,
            always_log as fn(&mut libafl::monitors::stats::ClientStatsManager) -> bool,
        );
        tuple_list!(json_mon)
    };

    // The event manager handle the various events generated during the fuzzing loop
    // such as the notification of the addition of a new item to the corpus
    // In this binary we support restarting: spawn a SimpleRestartingEventManager
    // which returns an optional State (when restarting) and an EventManager.
    let mut shmem_provider = StdShMemProvider::new().expect("failed to create shmem provider");

    let (maybe_state, mut mgr) =
        match SimpleRestartingEventManager::launch(monitors, &mut shmem_provider) {
            Ok(res) => res,
            Err(e) => match e {
                libafl::Error::ShuttingDown => {
                    // restarter requested shutdown
                    return;
                }
                _ => panic!("Failed to setup the restarter: {}", e),
            },
        };

    // If the restarter didn't provide an existing state, create one from scratch
    let rand = StdRand::new();
    let corpus = InMemoryOnDiskCorpus::new(args.corpus_dir.clone()).unwrap();
    let solution = InMemoryOnDiskCorpus::new(args.output_dir.clone()).unwrap();
    let _was_restored = maybe_state.is_some();
    let mut state = maybe_state.unwrap_or_else(|| {
        StdState::new(
            // RNG
            rand,
            // Corpus that will be evolved, use an on-disk-backed in-memory corpus so items are persisted
            corpus,
            // Corpus in which we store solutions (diffs in this example),
            // on disk so the user can get them after stopping the fuzzer
            solution,
            // States of the feedbacks.
            // The feedbacks can report the data that should persist in the State.
            &mut feedback,
            // Same for objective feedbacks
            &mut objective,
        )
        .unwrap()
    });

    // restored-state logging removed (hashes and initial-value logs not useful)

    // Pre-allocate metadata via the feedback helper (avoids doing manual
    // named-metadata manipulation here). Caller now invokes the helper
    // with access to `state` and the observers.
    // Use temporary TwoStateMapFeedback instances to initialize metadata
    // now that `state` is available. This avoids needing the original
    // `coverage_fb_a`/`coverage_fb_b` values which were moved into the
    // composed feedback earlier.
    // {
    //     use crate::feedbacks::two_state_map::TwoStateMapFeedback;

    //     let mut tmp_a = TwoStateMapFeedback::new_with_params(&map_observer_a, 1, 1, true, true);
    //     // Use the StateInitializer impl to initialize named metadata for the map observer
    //     let _ = <crate::feedbacks::two_state_map::TwoStateMapFeedback<_> as libafl::feedbacks::StateInitializer<_>>::
    //         init_state(&mut tmp_a, &mut state);
    //     let mut tmp_b = TwoStateMapFeedback::new_with_params(&map_observer_b, 1, 1, true, true);
    //     let _ = <crate::feedbacks::two_state_map::TwoStateMapFeedback<_> as libafl::feedbacks::StateInitializer<_>>::
    //         init_state(&mut tmp_b, &mut state);
    // }

    // A queue policy to get testcases from the corpus
    let scheduler = QueueScheduler::new();

    // A fuzzer with feedbacks and a corpus scheduler
    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

    // Create the executor for an in-process function with just one observer
    let executor_a = InProcessExecutor::with_timeout(
        &mut harness_impl_a,
        // include objective_observer so the objective feedback can read it
        tuple_list!(
            map_observer_a,
            rv_exec_observer_a,
            objective_observer.clone()
        ),
        &mut fuzzer,
        &mut state,
        &mut mgr,
        Duration::from_millis(args.timeout_ms),
    )
    .expect("Failed to create executor A");

    let executor_b = InProcessExecutor::with_timeout(
        &mut harness_impl_b,
        tuple_list!(
            map_observer_b,
            rv_exec_observer_b,
            objective_observer.clone()
        ),
        &mut fuzzer,
        &mut state,
        &mut mgr,
        Duration::from_millis(args.timeout_ms),
    )
    .expect("Failed to create executor B");

    let diff_observers_list = tuple_list!(map_swapper, objective_observer,);
    let mut differential_executor = DiffExecutor::new(executor_a, executor_b, diff_observers_list);

    // Generator of printable bytearrays of max size 32
    let mut generator = RandPrintablesGenerator::new(nonzero!(32));

    // 处理初始种子输入
    if let Some(input_dir) = &args.input_dir {
        if input_dir.exists() && input_dir.is_dir() {
            if args.verbose > 0 {
                println!("从目录加载初始种子: {:?}", input_dir);
            }

            // 使用 load_initial_inputs 方法加载种子
            state
                .load_initial_inputs(
                    &mut fuzzer,
                    &mut differential_executor,
                    &mut mgr,
                    &[input_dir.clone()],
                )
                .unwrap_or_else(|err| {
                    eprintln!("无法加载初始种子从 {:?}: {}", input_dir, err);

                    if args.verbose > 0 {
                        println!("种子加载失败，生成随机初始输入");
                    }

                    // 如果加载失败，生成随机初始输入
                    state
                        .generate_initial_inputs(
                            &mut fuzzer,
                            &mut differential_executor,
                            &mut generator,
                            &mut mgr,
                            8,
                        )
                        .expect("Failed to generate the initial corpus");
                });

            if args.verbose > 0 {
                println!("成功导入 {} 个输入文件", state.corpus().count());
            }
        } else {
            eprintln!("错误: 指定的种子目录不存在或不是目录: {:?}", input_dir);
            std::process::exit(1);
        }
    } else {
        if args.verbose > 0 {
            println!("未指定种子目录，生成随机初始输入");
        }
        // Generate 8 initial inputs
        state
            .generate_initial_inputs(
                &mut fuzzer,
                &mut differential_executor,
                &mut generator,
                &mut mgr,
                8,
            )
            .expect("Failed to generate the initial corpus");
    }

    // Setup a mutational stage with a basic bytes mutator
    let mutator = HavocScheduledMutator::new(havoc_mutations());
    let mut stages = tuple_list!(StdMutationalStage::new(mutator));

    if args.verbose > 0 {
        println!("开始模糊测试...");
    }

    // 根据配置决定使用哪种模糊测试循环
    // Unlimited fuzz loop: use the standard fuzz_loop (no time/iteration limits)
    if args.verbose > 0 {
        println!("运行无限制的模糊测试循环");
    }

    // This call will run indefinitely (or until the restarter/manager stops it)
    match fuzzer.fuzz_loop(
        &mut stages,
        &mut differential_executor,
        &mut state,
        &mut mgr,
    ) {
        Ok(_) => {}
        Err(e) => eprintln!("模糊测试循环中出现错误: {}", e),
    }
}
