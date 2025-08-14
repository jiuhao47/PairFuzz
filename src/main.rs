use clap::Parser;
use libafl::monitors::OnDiskJsonMonitor;
use libafl::{
    corpus::{Corpus, InMemoryCorpus, InMemoryOnDiskCorpus},
    events::SimpleEventManager,
    executors::{inprocess::InProcessExecutor, DiffExecutor, ExitKind},
    feedback_and_fast,
    feedbacks::{DiffExitKindFeedback, MaxMapFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    generators::RandPrintablesGenerator,
    inputs::{BytesInput, HasTargetBytes},
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    observers::StdMapObserver,
    schedulers::QueueScheduler,
    stages::mutational::StdMutationalStage,
    state::{HasCorpus, HasSolutions, StdState},
};
#[cfg(windows)]
use libafl::{
    corpus::{Corpus, InMemoryCorpus, InMemoryOnDiskCorpus},
    events::SimpleEventManager,
    executors::{inprocess::InProcessExecutor, DiffExecutor, ExitKind},
    feedbacks::{CrashFeedback, MaxMapFeedback},
    fuzzer::{Fuzzer, StdFuzzer},
    generators::RandPrintablesGenerator,
    inputs::{BytesInput, HasTargetBytes},
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    observers::StdMapObserver,
    schedulers::QueueScheduler,
    stages::mutational::StdMutationalStage,
    state::{HasCorpus, HasSolutions, StdState},
};
use libafl_bolts::{nonzero, rands::StdRand, tuples::tuple_list, AsSlice};
use libafl_targets::{edges_max_num, DifferentialAFLMapSwapObserver};
#[cfg(not(miri))]
use mimalloc::MiMalloc;
use std::{
    alloc::{alloc_zeroed, Layout},
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

#[global_allocator]
#[cfg(not(miri))]
static GLOBAL: MiMalloc = MiMalloc;

// bindings to the functions defined in the target
mod bindings {
    #![allow(non_snake_case)]
    #![allow(non_camel_case_types)]
    #![allow(non_upper_case_globals)]
    #![allow(unused)]
    #![allow(clippy::unreadable_literal)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

use bindings::{diffRayTestOneInput_1, diffRayTestOneInput_2};

/// 差分模糊测试器命令行参数
#[derive(Parser, Debug)]
#[command(name = "diffray")]
#[command(about = "一个差分模糊测试器，用于发现两个程序实现之间的差异")]
struct Args {
    /// 初始种子输入文件夹
    #[arg(short = 'i', long = "input", help = "初始种子输入文件夹路径")]
    input_dir: Option<PathBuf>,

    /// 最长模糊测试时间（秒）
    #[arg(
        short = 't',
        long = "max_time",
        help = "最长模糊测试时间（秒）",
        conflicts_with = "max_iterations"
    )]
    max_time: Option<u64>,

    /// 最大执行次数
    #[arg(
        short = 'n',
        long = "max-iterations",
        help = "最大执行次数",
        conflicts_with = "max_time"
    )]
    max_iterations: Option<u64>,

    /// 输出差分输入的文件夹
    #[arg(
        short = 'o',
        long = "output",
        help = "输出差分输入的文件夹路径",
        default_value = "./diffs"
    )]
    output_dir: PathBuf,

    /// 发现差分输入时不停止模糊测试器
    #[arg(
        short = 'c',
        long = "continue",
        help = "发现差分输入时不停止模糊测试器"
    )]
    continue_on_crash: bool,

    /// 日志文件存储位置
    #[arg(short = 'l', long = "log", help = "日志文件存储位置")]
    log_file: Option<PathBuf>,

    /// 详细输出
    #[arg(short = 'v', long = "verbose", help = "启用详细输出")]
    verbose: bool,

    /// 单次执行超时时间（毫秒）
    #[arg(
        long = "timeout",
        help = "单次执行超时时间（毫秒）",
        default_value = "5000"
    )]
    timeout_ms: u64,
}

#[cfg(feature = "multimap")]
mod multimap {
    pub use libafl::observers::{HitcountsIterableMapObserver, MultiMapObserver};
    pub use libafl_bolts::ownedref::OwnedMutSlice;
}
#[cfg(feature = "multimap")]
use multimap::{HitcountsIterableMapObserver, MultiMapObserver, OwnedMutSlice};

#[cfg(not(feature = "multimap"))]
mod slicemap {
    pub use libafl::observers::HitcountsMapObserver;

    pub static mut EDGES: &mut [u8] = &mut [];
}
#[cfg(not(feature = "multimap"))]
use slicemap::{HitcountsMapObserver, EDGES};

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

    // 记录开始时间
    let start_time = Instant::now();
    let mut iteration_count = 0u64;
    // The closure that we want to fuzz
    let mut first_harness = |input: &BytesInput| {
        let target = input.target_bytes();
        let buf = target.as_slice();
        let result = unsafe { diffRayTestOneInput_1(buf.as_ptr(), buf.len()) };
        // 将返回码转换为 ExitKind，0 表示成功，非 0 表示错误
        if result == 0 {
            // println!("libpng_parse return {}", result);
            ExitKind::Ok
        } else {
            // println!("libpng_parse return {}", result);
            ExitKind::Crash
        }
    };
    let mut second_harness = |input: &BytesInput| {
        let target = input.target_bytes();
        let buf = target.as_slice();
        let result = unsafe { diffRayTestOneInput_2(buf.as_ptr(), buf.len()) };
        // 将返回码转换为 ExitKind，0 表示成功，非 0 表示错误
        if result == 0 {
            // println!("libspng_parse return {}", result);
            ExitKind::Ok
        } else {
            // println!("libspng_parse return {}", result);
            ExitKind::Crash
        }
    };

    let num_edges: usize = edges_max_num(); // upper bound

    #[cfg(feature = "multimap")]
    let (
        first_map_observer,
        second_map_observer,
        map_swapper,
        map_observer,
        layout,
        first_edges,
        second_edges,
    ) = {
        // initialize the maps
        let layout = Layout::from_size_align(num_edges, 64).unwrap();
        let first_edges = unsafe { (alloc_zeroed(layout), num_edges) };
        let second_edges = unsafe { (alloc_zeroed(layout), num_edges) };

        let combined_edges = unsafe {
            vec![
                OwnedMutSlice::from_raw_parts_mut(first_edges.0, first_edges.1),
                OwnedMutSlice::from_raw_parts_mut(second_edges.0, second_edges.1),
            ]
        };

        // create the base maps used to observe the different executors from two independent maps
        let mut first_map_observer =
            unsafe { StdMapObserver::from_mut_ptr("first-edges", first_edges.0, first_edges.1) };
        let mut second_map_observer =
            unsafe { StdMapObserver::from_mut_ptr("second-edges", second_edges.0, second_edges.1) };

        // create a map swapper so that we can replace the coverage map pointer (requires feature pointer_maps!)
        let map_swapper =
            DifferentialAFLMapSwapObserver::new(&mut first_map_observer, &mut second_map_observer);

        // create a combined map observer, e.g. for calibration
        // we use MultiMapObserver::differential to indicate that we want to use the observer in
        // differential mode
        let map_observer = HitcountsIterableMapObserver::new(MultiMapObserver::differential(
            "combined-edges",
            combined_edges,
        ));

        (
            first_map_observer,
            second_map_observer,
            map_swapper,
            map_observer,
            layout,
            first_edges,
            second_edges,
        )
    };
    #[cfg(not(feature = "multimap"))]
    let (first_map_observer, second_map_observer, map_swapper, map_observer) = {
        // initialize the map
        unsafe {
            let layout = Layout::from_size_align(num_edges * 2, 64).unwrap();
            EDGES = core::slice::from_raw_parts_mut(alloc_zeroed(layout), num_edges * 2);
        }

        // TODO: This will break soon, fix me! See https://github.com/AFLplusplus/LibAFL/issues/2786
        #[allow(static_mut_refs)] // only a problem on nightly
        let edges_ptr = unsafe { EDGES.as_mut_ptr() };

        // create the base maps used to observe the different executors by splitting a slice
        let mut first_map_observer =
            unsafe { StdMapObserver::from_mut_ptr("first-edges", edges_ptr, num_edges) };
        let mut second_map_observer = unsafe {
            StdMapObserver::from_mut_ptr("second-edges", edges_ptr.add(num_edges), num_edges)
        };

        // create a map swapper so that we can replace the coverage map pointer (requires feature pointer_maps!)
        let map_swapper =
            DifferentialAFLMapSwapObserver::new(&mut first_map_observer, &mut second_map_observer);

        // create a combined map observer, e.g. for calibration
        // we use StdMapObserver::differential to indicate that we want to use the observer in
        // differential mode
        let map_observer = unsafe {
            HitcountsMapObserver::new(StdMapObserver::differential_from_mut_ptr(
                "combined-edges",
                edges_ptr,
                num_edges * 2,
            ))
        };

        (
            first_map_observer,
            second_map_observer,
            map_swapper,
            map_observer,
        )
    };

    // Feedback to rate the interestingness of an input
    let mut feedback = MaxMapFeedback::new(&map_observer);

    // A feedback to choose if an input is a solution or not
    // Crash here means "both crashed", which is our objective
    // let mut objective = CrashFeedback::new();
    let mut objective = feedback_and_fast!(
        DiffExitKindFeedback::new(),
        MaxMapFeedback::with_name("mapfeedback_metadata_objective", &map_observer)
    );

    // create a State from scratch
    let mut state = StdState::new(
        // RNG
        StdRand::new(),
        // Corpus that will be evolved, we keep it in memory for performance
        InMemoryCorpus::new(),
        // Corpus in which we store solutions (diffs in this example),
        // on disk so the user can get them after stopping the fuzzer
        InMemoryOnDiskCorpus::new(args.output_dir.clone()).unwrap(),
        // States of the feedbacks.
        // The feedbacks can report the data that should persist in the State.
        &mut feedback,
        // Same for objective feedbacks
        &mut objective,
    )
    .unwrap();

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
    let first_executor = InProcessExecutor::with_timeout(
        &mut first_harness,
        tuple_list!(first_map_observer),
        &mut fuzzer,
        &mut state,
        &mut mgr,
        Duration::from_millis(args.timeout_ms),
    )
    .expect("Failed to create the first executor");

    let second_executor = InProcessExecutor::with_timeout(
        &mut second_harness,
        tuple_list!(second_map_observer),
        &mut fuzzer,
        &mut state,
        &mut mgr,
        Duration::from_millis(args.timeout_ms),
    )
    .expect("Failed to create the second executor");

    // create the differential executor, providing both the map swapper (which will ensure the
    // instrumentation picks the correct map to write to) and the map observer (which provides the
    // combined feedback)
    let mut differential_executor = DiffExecutor::new(
        first_executor,
        second_executor,
        tuple_list!(map_swapper, map_observer),
    );

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

    #[cfg(feature = "multimap")]
    unsafe {
        std::alloc::dealloc(first_edges.0, layout);
        std::alloc::dealloc(second_edges.0, layout);
    }
}
