use clap::Parser;
use libafl::monitors::OnDiskJsonMonitor;
use libafl::{
    corpus::{Corpus, InMemoryOnDiskCorpus},
    events::SimpleEventManager,
    executors::{inprocess::InProcessExecutor, DiffExecutor},
    feedback_and_fast, feedback_or_fast,
    feedbacks::{DiffExitKindFeedback, MaxMapFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    generators::RandPrintablesGenerator,
    inputs::BytesInput,
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    schedulers::QueueScheduler,
    stages::mutational::StdMutationalStage,
    state::{HasCorpus, HasSolutions, StdState},
};
// Note: we intentionally avoid MinMapFeedback here to keep concrete feedback types
// identical across branches; we emulate Min via inverted owned map.
use libafl_bolts::{nonzero, rands::StdRand, tuples::tuple_list};
use libafl_targets::edges_max_num;
use mimalloc::MiMalloc;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

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
    // 解析命令行参数
    let args = Args::parse();

    // 设置日志
    if let Some(log_file) = &args.log_file {
        // 如果指定了日志文件，配置文件日志记录
        let _log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)
            .expect("无法创建或打开日志文件");

        // 简单的日志设置
        if args.verbose {
            println!("日志将写入到: {:?}", args.log_file.as_ref().unwrap());
        }
    } else if args.verbose {
        println!("启用详细输出模式");
    }

    // 创建输出目录
    if let Err(e) = fs::create_dir_all(&args.output_dir) {
        eprintln!("无法创建输出目录 {:?}: {}", args.output_dir, e);
        std::process::exit(1);
    }

    if args.verbose {
        println!("配置参数:");
        println!("  输入目录: {:?}", args.input_dir);
        println!("  输出目录: {:?}", args.output_dir);
        println!("  最大执行时间: {:?}秒", args.max_time);
        println!("  最大迭代次数: {:?}", args.max_iterations);
        println!("  发现差分时继续: {}", args.continue_on_crash);
        println!("  日志文件: {:?}", args.log_file);
        println!("  执行超时时间: {}毫秒", args.timeout_ms);
    }

    let start_time = Instant::now();
    let mut iteration_count = 0u64;
    let mut harness_impl_a = |input: &BytesInput| crate::harness::first_harness(input);
    let mut harness_impl_b = |input: &BytesInput| crate::harness::second_harness(input);
    let num_edges: usize = edges_max_num(); // upper bound

    // Build observers using helper which handles EDGES allocation
    let (map_observer_a, map_observer_b, objective_observer) =
        observers::coverage_helpers::make_map_observers(num_edges);

    // create a map swapper from the two map observers (requires the swapper to borrow them)
    let mut map_observer_a = map_observer_a;
    let mut map_observer_b = map_observer_b;
    let map_swapper =
        crate::feedbacks::coverage::make_map_swapper(&mut map_observer_a, &mut map_observer_b);

    // Feedback to rate the interestingness of an input
    // create return-value observers for the return-value feedback (per-implementation)
    let rv_feedback_observer_a = ReturnValueObserver::new("first_return");
    let rv_feedback_observer_b = ReturnValueObserver::new("second_return");

    // Compose map feedback with return-value feedback according to selected modes
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

    // create return-value observers that are attached to executors (per-implementation)
    let rv_exec_observer_a = ReturnValueObserver::new("first_return");
    let rv_exec_observer_b = ReturnValueObserver::new("second_return");

    // Combine DiffExitKindFeedback with return-value feedback and map objective
    let mut objective = feedback_and_fast!(
        DiffExitKindFeedback::new(),
        MaxMapFeedback::with_name("diff-cov", &objective_observer)
    );

    // Compose coverage feedback (must exist before creating State so the State
    // can capture feedbacks' persistent metadata during construction).
    let coverage_fb = feedback_and_fast!(coverage_fb_a, coverage_fb_b);
    let mut feedback = feedback_or_fast!(coverage_fb, rv_wrapper);

    let rand = StdRand::new();
    let corpus = InMemoryOnDiskCorpus::new(args.corpus_dir.clone()).unwrap();
    let solution = InMemoryOnDiskCorpus::new(args.output_dir.clone()).unwrap();
    // create a State from scratch
    let mut state = StdState::new(
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
    .unwrap();

    // Pre-allocate metadata via the feedback helper (avoids doing manual
    // named-metadata manipulation here). Caller now invokes the helper
    // with access to `state` and the observers.
    // Use temporary TwoStateMapFeedback instances to initialize metadata
    // now that `state` is available. This avoids needing the original
    // `coverage_fb_a`/`coverage_fb_b` values which were moved into the
    // composed feedback earlier.
    {
        use crate::feedbacks::two_state_map::TwoStateMapFeedback;

        let mut tmp_a = TwoStateMapFeedback::new_with_params(&map_observer_a, 1, 1, true, true);
        let _ = tmp_a.ensure_initialized_with_observer(&mut state, &map_observer_a);
        let mut tmp_b = TwoStateMapFeedback::new_with_params(&map_observer_b, 1, 1, true, true);
        let _ = tmp_b.ensure_initialized_with_observer(&mut state, &map_observer_b);
    }

    // (coverage_fb and feedback already composed above)

    // The Monitor trait define how the fuzzer stats are displayed to the user
    let monitors = {
        let json_log_path = args
            .log_file
            .as_ref()
            .map(|p| p.with_extension("json"))
            .unwrap_or_else(|| PathBuf::from("fuzzer_stats.json"));
        let json_mon = OnDiskJsonMonitor::new(json_log_path, |_| true);
        tuple_list!(json_mon)
    };

    // The event manager handle the various events generated during the fuzzing loop
    // such as the notification of the addition of a new item to the corpus
    let mut mgr = SimpleEventManager::new(monitors);

    // A queue policy to get testcases from the corpus
    let scheduler = QueueScheduler::new();

    // A fuzzer with feedbacks and a corpus scheduler
    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

    // Create the executor for an in-process function with just one observer
    let executor_a = InProcessExecutor::with_timeout(
        &mut harness_impl_a,
        // include objective_observer so the objective feedback can read it
        tuple_list!(map_observer_a, rv_exec_observer_a),
        &mut fuzzer,
        &mut state,
        &mut mgr,
        Duration::from_millis(args.timeout_ms),
    )
    .expect("Failed to create executor A");

    let executor_b = InProcessExecutor::with_timeout(
        &mut harness_impl_b,
        tuple_list!(map_observer_b, rv_exec_observer_b),
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
            if args.verbose {
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

                    if args.verbose {
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

            if args.verbose {
                println!("成功导入 {} 个输入文件", state.corpus().count());
            }
        } else {
            eprintln!("错误: 指定的种子目录不存在或不是目录: {:?}", input_dir);
            std::process::exit(1);
        }
    } else {
        if args.verbose {
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

    if args.verbose {
        println!("开始模糊测试...");
    }

    // 根据配置决定使用哪种模糊测试循环
    if let Some(max_iter) = args.max_iterations {
        // 使用 fuzz_loop_for 进行固定次数的模糊测试
        if args.verbose {
            println!("使用最大执行次数限制: {} 次", max_iter);
        }

        // 将大的迭代次数分成较小的块，以便检查是否发现了差分输入
        let chunk_size = 1000u64;
        let mut remaining_iters = max_iter;

        while remaining_iters > 0 {
            // 检查是否找到差分输入
            let has_solutions = !state.solutions().is_empty();
            if has_solutions && !args.continue_on_crash {
                if args.verbose {
                    println!("发现差分输入，停止模糊测试");
                }
                break;
            }

            // 计算本次迭代的数量
            let current_chunk = std::cmp::min(chunk_size, remaining_iters);

            // 执行模糊测试
            match fuzzer.fuzz_loop_for(
                &mut stages,
                &mut differential_executor,
                &mut state,
                &mut mgr,
                current_chunk,
            ) {
                Ok(_) => {
                    iteration_count += current_chunk;
                    remaining_iters -= current_chunk;

                    if args.verbose {
                        let elapsed = start_time.elapsed();
                        println!(
                            "已完成迭代: {}, 剩余: {}, 用时: {:.2}s, 语料库大小: {}, 解决方案: {}",
                            iteration_count,
                            remaining_iters,
                            elapsed.as_secs_f64(),
                            state.corpus().count(),
                            state.solutions().count()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("模糊测试循环中出现错误: {}", e);
                    break;
                }
            }
        }
    } else if let Some(max_time) = args.max_time {
        // 使用时间限制的模糊测试
        if args.verbose {
            println!("使用时间限制: {} 秒", max_time);
        }

        // 使用较小的迭代块来定期检查时间和差分输入
        let chunk_size = 1000u64;

        loop {
            // 检查时间限制
            let elapsed = start_time.elapsed();
            if elapsed.as_secs() >= max_time {
                if args.verbose {
                    println!("达到时间限制 {} 秒，停止模糊测试", max_time);
                }
                break;
            }

            // 检查是否找到差分输入
            let has_solutions = !state.solutions().is_empty();
            if has_solutions && !args.continue_on_crash {
                if args.verbose {
                    println!("发现差分输入，停止模糊测试");
                }
                break;
            }

            // 执行一个较小的迭代块
            match fuzzer.fuzz_loop_for(
                &mut stages,
                &mut differential_executor,
                &mut state,
                &mut mgr,
                chunk_size,
            ) {
                Ok(_) => {
                    iteration_count += chunk_size;

                    if args.verbose && iteration_count % 10000 == 0 {
                        let elapsed = start_time.elapsed();
                        println!(
                            "迭代: {}, 用时: {:.2}s, 语料库大小: {}, 解决方案: {}",
                            iteration_count,
                            elapsed.as_secs_f64(),
                            state.corpus().count(),
                            state.solutions().count()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("模糊测试循环中出现错误: {}", e);
                    break;
                }
            }
        }
    } else {
        // 无限制的模糊测试循环（使用传统的 fuzz_one 方法）
        if args.verbose {
            println!("运行无限制的模糊测试循环");
        }

        loop {
            // 检查是否找到差分输入
            let has_solutions = !state.solutions().is_empty();
            if has_solutions && !args.continue_on_crash {
                if args.verbose {
                    println!("发现差分输入，停止模糊测试");
                }
                break;
            }

            // 执行一次模糊测试迭代
            match fuzzer.fuzz_one(
                &mut stages,
                &mut differential_executor,
                &mut state,
                &mut mgr,
            ) {
                Ok(_) => {
                    iteration_count += 1;

                    // 定期输出进度信息
                    if args.verbose && iteration_count % 1000 == 0 {
                        let elapsed = start_time.elapsed();
                        println!(
                            "迭代: {}, 用时: {:.2}s, 语料库大小: {}, 解决方案: {}",
                            iteration_count,
                            elapsed.as_secs_f64(),
                            state.corpus().count(),
                            state.solutions().count()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("模糊测试循环中出现错误: {}", e);
                    break;
                }
            }
        }
    }

    // 输出最终统计信息
    let final_elapsed = start_time.elapsed();
    let solutions_count = state.solutions().count();
    let corpus_count = state.corpus().count();

    println!("模糊测试完成:");
    println!("  总执行时间: {:.2} 秒", final_elapsed.as_secs_f64());
    println!("  总迭代次数: {}", iteration_count);
    println!("  语料库大小: {}", corpus_count);
    println!("  发现的差分输入数量: {}", solutions_count);

    if solutions_count > 0 {
        println!("  差分输入保存在: {:?}", args.output_dir);
    }

    // 如果指定了日志文件，写入最终统计信息
    if let Some(log_file) = &args.log_file {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)
        {
            let _ = writeln!(
                file,
                "[{}] 模糊测试完成 - 时间: {:.2}s, 迭代: {}, 语料库: {}, 差分输入: {}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                final_elapsed.as_secs_f64(),
                iteration_count,
                corpus_count,
                solutions_count
            );
        }
    }

    // No multimap-specific allocations to free in this build.
}
