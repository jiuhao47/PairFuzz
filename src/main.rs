use clap::Parser;
use libafl::monitors::OnDiskJsonMonitor;
use libafl::{
    corpus::{Corpus, InMemoryOnDiskCorpus},
    events::SimpleRestartingEventManager,
    executors::{forkserver::ForkserverExecutor, DiffExecutor},
    feedback_and_fast,
    feedback_or_fast,
    // feedbacks::MaxMapFeedback,
    fuzzer::{Fuzzer, StdFuzzer},
    generators::RandPrintablesGenerator,
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    schedulers::QueueScheduler,
    stages::mutational::StdMutationalStage,
    state::{HasCorpus, StdState},
};
// Note: we intentionally avoid MinMapFeedback here to keep concrete feedback types
// identical across branches; we emulate Min via inverted owned map.
use libafl::common::HasMetadata;
use libafl::executors::StdChildArgs;
use libafl::state::HasStartTime;
use libafl_bolts::shmem::{ShMemProvider, StdShMemProvider};
use libafl_bolts::StdTargetArgs;
use libafl_bolts::{current_time, nonzero, rands::StdRand, tuples::tuple_list};
use mimalloc::MiMalloc;
use std::{fs, path::PathBuf, time::Duration};

// driver::constants now holds shared constants (see src/driver/constants.rs)

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

// Custom modules and utilities
mod driver;
mod feedbacks;
mod observers;
mod utils;

use observers::ReturnValueObserver;
// CLI args and logging helpers
use crate::driver::args::Args;
use crate::utils::logging::{error, info, init_from_verbose, set_log_dir, warn};

#[expect(clippy::too_many_lines)]
pub fn main() {
    // Parse command-line arguments and initialize logging
    let args = Args::parse();
    init_from_verbose(args.verbose, None);
    // Disable ASAN leak detection and make ASAN abort on errors when present
    unsafe { std::env::set_var("ASAN_OPTIONS", "abort_on_error=1:detect_leaks=false") }

    // Ensure log directory exists and configure logger to write there
    if let Some(log_dir) = &args.log_dir {
        if let Err(e) = fs::create_dir_all(log_dir) {
            error(
                "main",
                &format!("Failed to create log directory {:?}: {}", log_dir, e),
            );
            std::process::exit(1);
        };
        set_log_dir(Some(log_dir.clone()));
        if args.verbose > 0 {
            info(
                "main",
                &format!(
                    "Logs will be written to directory: {:?} (main log: diffray.log)",
                    log_dir
                ),
            );
        }
    } else if args.verbose > 0 {
        info("main", "Detailed output enabled");
    }

    // Run the main logic in a fallible function to allow unified error handling
    if let Err(e) = run(args) {
        error("main", &format!("Fatal error: {}", e));
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    // Ensure output directory exists
    if let Err(e) = fs::create_dir_all(&args.output_dir) {
        error(
            "main",
            &format!(
                "Failed to create output directory {:?}: {}",
                args.output_dir, e
            ),
        );
        return Err(Box::new(e));
    }

    if args.verbose > 0 {
        info("main", "Configuration:");
        info("main", &format!("  Input directory: {:?}", args.input_dir));
        info(
            "main",
            &format!("  Output directory: {:?}", args.output_dir),
        );
        info("main", &format!("  Log directory: {:?}", args.log_dir));
        info(
            "main",
            &format!("  Single-execution timeout: {} ms", args.timeout_ms),
        );
    }

    // The remainder of the original main function continues below...

    // Fuzzing configuration
    // num_edges: maximum size of an individual coverage map
    let num_edges: usize = crate::driver::constants::MAP_SIZE;

    // We'll allocate coverage observers (A, B) and a combined objective buffer.
    use libafl::observers::HitcountsMapObserver;
    use libafl::observers::StdMapObserver;

    let (map_observer_a, map_observer_b, objective_observer);

    // Return-value feedback observers (used by rv-mode Single/Combo)
    let rv_feedback_observer_a = ReturnValueObserver::new("first_return");
    let rv_feedback_observer_b = ReturnValueObserver::new("second_return");

    // Executor-attached return-value observers (may be backed by small shmem regions)
    use libafl_bolts::shmem::StdShMem;
    let mut rv_exec_observer_a = ReturnValueObserver::new("first_return");
    let mut rv_exec_observer_b = ReturnValueObserver::new("second_return");

    // Monitoring setup: how stats are displayed/recorded
    // Provide a predicate that always logs so JSON stats are written frequently.
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
        // Also include a SimpleMonitor that prints to stdout. When the `introspection`
        // feature is enabled in libafl this monitor will include per-client perf stats.
        // The printing behavior is controlled by the CLI flag `--no-simple-monitor`.
        let enable_simple = args.simple_monitor_enabled();
        let simple_mon = libafl::monitors::SimpleMonitor::new(move |s| {
            if enable_simple {
                println!("{s}")
            }
        });
        tuple_list!(json_mon, simple_mon)
    };

    // Event manager: supports restarting and may provide a restored State
    let mut shmem_provider = StdShMemProvider::new().expect("failed to create shmem provider");

    // Try to allocate per-target coverage shared memory. Fall back to heap-backed maps on error.
    let mut _cov_shmem_a: Option<libafl_bolts::shmem::StdShMem>;
    let mut _cov_shmem_b: Option<libafl_bolts::shmem::StdShMem>;
    let cov_shm_id_a: Option<String>;
    let cov_shm_id_b: Option<String>;

    // Allocate coverage shmem for A (or fallback)
    match crate::observers::shmem::rv_shmem::try_alloc_cov_shmem_with_ptr(
        &mut shmem_provider,
        num_edges,
        "__DIFFRAY_TMP_COV_A",
    ) {
        Ok((s_a, ptr_a, id)) => {
            cov_shm_id_a = Some(id);
            // create observer pointing at this shmem
            unsafe {
                let obs_a =
                    StdMapObserver::differential_from_mut_ptr("first-edges", ptr_a, num_edges);
                map_observer_a = HitcountsMapObserver::new(obs_a);
            }
            _cov_shmem_a = Some(s_a);
        }
        Err(e) => {
            error(
                "main",
                &format!(
                    "Failed to allocate coverage shmem for A ({}). Aborting to avoid inconsistent observer state.",
                    e
                ),
            );
            std::process::exit(1);
        }
    }

    // Allocate coverage shmem for B (or fallback)
    match crate::observers::shmem::rv_shmem::try_alloc_cov_shmem_with_ptr(
        &mut shmem_provider,
        num_edges,
        "__DIFFRAY_TMP_COV_B",
    ) {
        Ok((s_b, ptr_b, id)) => {
            cov_shm_id_b = Some(id);
            unsafe {
                let obs_b =
                    StdMapObserver::differential_from_mut_ptr("second-edges", ptr_b, num_edges);
                map_observer_b = HitcountsMapObserver::new(obs_b);
            }
            _cov_shmem_b = Some(s_b);
        }
        Err(e) => {
            error(
                "main",
                &format!(
                    "Failed to allocate coverage shmem for B ({}). Aborting to avoid inconsistent observer state.",
                    e
                ),
            );
            std::process::exit(1);
        }
    }

    // Objective buffer: combine A and B maps into a heap-backed buffer of size 2 * num_edges
    let objective_len = num_edges.checked_mul(2).unwrap_or(num_edges);
    // Allocate the objective buffer and KEEP ownership in a Box so it will be
    // automatically freed when dropped. We pass a raw pointer to the observer
    // but retain the Box (moved into the updater later) to avoid leaks.
    let mut objective_box = vec![0u8; objective_len].into_boxed_slice();
    let objective_ptr: *mut u8 = objective_box.as_mut_ptr();
    unsafe {
        let obs_obj = StdMapObserver::differential_from_mut_ptr(
            "objective-edges",
            objective_ptr,
            objective_len,
        );
        objective_observer = HitcountsMapObserver::new(obs_obj);
    }

    // Build coverage/return-value feedbacks. Support rv-mode == Single by constructing
    // a DiffrayFeedback that uses return-value observers only. Otherwise fall
    // back to the SingleCov two-map feedback used previously.
    let mut feedback = match args.mode {
        crate::driver::args::RunMode::SinglePl => {
            crate::feedbacks::diffray_feedback::DiffrayFeedback::new_single_pl(
                &map_observer_a,
                &map_observer_b,
            )
        }
        crate::driver::args::RunMode::ComboPl => {
            crate::feedbacks::diffray_feedback::DiffrayFeedback::new_combo_pl(
                &map_observer_a,
                &map_observer_b,
            )
        }
        crate::driver::args::RunMode::SingleRv => {
            crate::feedbacks::diffray_feedback::DiffrayFeedback::new_single_rv(
                &rv_feedback_observer_a,
                &rv_feedback_observer_b,
                &map_observer_a,
                &map_observer_b,
            )
        }
        crate::driver::args::RunMode::ComboRv => {
            crate::feedbacks::diffray_feedback::DiffrayFeedback::new_combo_rv(
                &rv_feedback_observer_a,
                &rv_feedback_observer_b,
                &map_observer_a,
                &map_observer_b,
            )
        }
        crate::driver::args::RunMode::ComboCov => {
            crate::feedbacks::diffray_feedback::DiffrayFeedback::new_combo_cov(
                &map_observer_a,
                &map_observer_b,
            )
        }
        crate::driver::args::RunMode::SingleCov => {
            crate::feedbacks::diffray_feedback::DiffrayFeedback::new_single_cov(
                &map_observer_a,
                &map_observer_b,
            )
        }
    };

    // Build objectives: diff objective (based on return values) and map objective
    let diff_obj = crate::feedbacks::DiffObjective::new(
        "zero_nonzero",
        &[&rv_exec_observer_a, &rv_exec_observer_b],
    );
    let map_diff_obj_fb = libafl::feedbacks::MaxMapFeedback::new(&objective_observer);
    let mut objective = feedback_and_fast!(
        feedback_or_fast!(crate::feedbacks::CrashObjective::new(), diff_obj),
        map_diff_obj_fb
    );

    let (maybe_state, mut mgr) =
        match SimpleRestartingEventManager::launch(monitors, &mut shmem_provider) {
            Ok(res) => res,
            Err(e) => match e {
                libafl::Error::ShuttingDown => {
                    // restarter requested shutdown
                    return Ok(());
                }
                _ => panic!("Failed to setup the restarter: {}", e),
            },
        };

    // Launch the restarting event manager. It may return a restored State.
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

    // Initialize named metadata for map observers so corpus/state resume and
    // scheduler heuristics that rely on observer metadata work correctly.
    // We create a temporary DiffrayFeedback instance and use its
    // `StateInitializer` impl to register any named metadata in the `State`.
    {
        let mut tmp_maps_fb = crate::feedbacks::diffray_feedback::DiffrayFeedback::new_single_cov(
            &map_observer_a,
            &map_observer_b,
        );
        let _ = <crate::feedbacks::diffray_feedback::DiffrayFeedback<_> as
            libafl::feedbacks::StateInitializer<_>>::init_state(&mut tmp_maps_fb, &mut state);
    }

    // Scheduler and fuzzer construction
    let scheduler = QueueScheduler::new();

    // A fuzzer with feedbacks and a corpus scheduler
    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

    // Build two Forkserver executors (A and B) and wrap them in a DiffExecutor
    // Reuse the existing shmem provider to reduce duplicate allocations/syscalls
    // instead of creating a new `fs_shmem_provider`.

    // Some targets expect __AFL_SHM_ID in the environment; we write any allocated
    // coverage shmem ids into the parent's env so forkserver builders can pick them up.
    // Allocate small shmem regions for return-value transport (4 bytes each).
    let mut rv_shmem_a: Option<StdShMem> = None;
    let mut rv_shmem_b: Option<StdShMem> = None;
    if args.prog_a.is_some() || args.prog_b.is_some() {
        // use the same provider to allocate small shmems; warn on errors (will fallback to TLS)
        match crate::observers::shmem::rv_shmem::try_new_shmem_and_write_env(
            &mut shmem_provider,
            4,
            "__DIFFRAY_RV_SHM_ID_A",
        ) {
            Ok(s) => rv_shmem_a = Some(s),
            Err(e) => warn("main", &format!("failed to alloc rv shmem A: {}", e)),
        }
        match crate::observers::shmem::rv_shmem::try_new_shmem_and_write_env(
            &mut shmem_provider,
            4,
            "__DIFFRAY_RV_SHM_ID_B",
        ) {
            Ok(s) => rv_shmem_b = Some(s),
            Err(e) => warn("main", &format!("failed to alloc rv shmem B: {}", e)),
        }
    }

    // Tokens used for auto-dictionary
    let mut tokens = libafl::mutators::Tokens::new();

    // Build executor A (forkserver) and attach coverage/rv shmems if available
    let executor_a = if let Some(prog_a) = &args.prog_a {
        // Validate target binary for executor A and detect capabilities
        let caps_a = match crate::driver::bin_check::check_binary(
            &mut prog_a.clone(),
            false,
            "__AFL_SHM_ID",
        ) {
            Ok(c) => c,
            Err(e) => {
                error("main", &format!("Target A binary validation failed: {}", e));
                std::process::exit(1);
            }
        };
        info(
            "main",
            &format!(
                "Detected capabilities for A: persistent={} defer={} asan={} lsan={} instrumented={}",
                caps_a.persistent, caps_a.defer, caps_a.has_asan, caps_a.has_lsan, caps_a.instrumented
            ),
        );
        // If we allocated a rv shmem earlier, extract id/size and inject into child's env
        // Determine persistent setting: CLI flags take precedence over detected capability.
        // Use `Args::persistent_enabled()` to encapsulate precedence between
        // `--persistent` and `--no-persistent`.
        let is_persistent_a = if args.persistent_enabled() {
            caps_a.persistent
        } else {
            false
        };

        let mut builder_a = ForkserverExecutor::builder()
            .program(prog_a.as_os_str().to_os_string())
            .shmem_provider(&mut shmem_provider)
            .autotokens(&mut tokens)
            .is_persistent(is_persistent_a)
            .coverage_map_size(num_edges)
            .timeout(Duration::from_millis(args.timeout_ms));

        // Collect child envs to inject into the builder in a single pass.
        let mut child_envs_a: Vec<(&str, String)> = Vec::new();

        // If a per-target coverage shmem was allocated, add its id to the child env list
        if let Some(id) = cov_shm_id_a.as_ref() {
            child_envs_a.push(("__AFL_SHM_ID", id.clone()));
            // Also log for diagnostics
            info("main", &format!("Injected __AFL_SHM_ID for A = {}", id));
        }

        if let Some(s) = rv_shmem_a.take() {
            match crate::observers::shmem::rv_shmem::extract_id_size_and_clear(
                &s,
                "__DIFFRAY_TMP_RV_A",
            ) {
                Ok((id, size)) => {
                    // Queue envs for child builder injection using the extracted id/size
                    child_envs_a.push(("__DIFFRAY_RV_SHM_ID_A", id.clone()));
                    child_envs_a.push(("__DIFFRAY_RV_SHM_ID_A_SIZE", size.clone()));
                    info(
                        "main",
                        &format!(
                            "Prepared __DIFFRAY_RV_SHM_ID_A for builder (id={} size={})",
                            id, size
                        ),
                    );
                }
                Err(e) => warn("main", &format!("failed to extract rv shmem id A: {}", e)),
            }
            // Attach shmem to executor-side observer
            rv_exec_observer_a.set_shmem(s);
            info(
                "main",
                "Attached rv shmem to executor observer A (first_return)",
            );
            // rv_shmem_snapshot.log removed per user request
        }

        // If per-target cov shmem was allocated, write __AFL_SHM_ID into parent env
        if let Some(s) = _cov_shmem_a.as_ref() {
            if let Err(e) = crate::observers::shmem::rv_shmem::write_shmem_to_env(s, "__AFL_SHM_ID")
            {
                warn(
                    "main",
                    &format!("failed to write __AFL_SHM_ID for A: {}", e),
                );
            }
            info("main", "Wrote __AFL_SHM_ID into parent env for executor A");
        }

        // Apply collected child envs to the builder in one pass to reduce repetitive
        // builder.env calls scattered across the code.
        for (k, v) in child_envs_a.drain(..) {
            builder_a = builder_a.env(k, v);
        }

        // Clear any parent env pollution that may remain (single loop instead of scattered calls)
        for key in &["__DIFFRAY_RV_SHM_ID_A", "__DIFFRAY_RV_SHM_ID_A_SIZE"] {
            let _ = std::env::remove_var(key);
        }

        // Build executor A using dynamic map sizing; exit on failure.
        match builder_a.build_dynamic_map(map_observer_a, tuple_list!(rv_exec_observer_a)) {
            Ok(exec) => exec,
            Err(e) => {
                error(
                    "main",
                    &format!("Failed to create forkserver executor A: {}", e),
                );
                std::process::exit(1);
            }
        }
    } else {
        error(
            "main",
            "prog-a not provided; forkserver executor A cannot be created",
        );
        std::process::exit(1);
    };

    // Build executor B (forkserver) and attach coverage/rv shmems if available
    let executor_b = if let Some(prog_b) = &args.prog_b {
        // Validate target binary for executor B and detect capabilities
        let caps_b = match crate::driver::bin_check::check_binary(
            &mut prog_b.clone(),
            false,
            "__AFL_SHM_ID",
        ) {
            Ok(c) => c,
            Err(e) => {
                error("main", &format!("Target B binary validation failed: {}", e));
                std::process::exit(1);
            }
        };
        info(
            "main",
            &format!(
                "Detected capabilities for B: persistent={} defer={} asan={} lsan={} instrumented={}",
                caps_b.persistent, caps_b.defer, caps_b.has_asan, caps_b.has_lsan, caps_b.instrumented
            ),
        );

        // Determine persistent setting for B: CLI flags take precedence over detected capability.
        let is_persistent_b = if args.persistent_enabled() {
            caps_b.persistent
        } else {
            false
        };

        let mut builder_b = ForkserverExecutor::builder()
            .program(prog_b.as_os_str().to_os_string())
            .shmem_provider(&mut shmem_provider)
            .autotokens(&mut tokens)
            .is_persistent(is_persistent_b)
            .coverage_map_size(num_edges)
            .timeout(Duration::from_millis(args.timeout_ms));

        // Collect child envs for B and inject in a single pass
        let mut child_envs_b: Vec<(&str, String)> = Vec::new();

        // If a per-target coverage shmem was allocated, add its id to the child env list
        if let Some(id) = cov_shm_id_b.as_ref() {
            child_envs_b.push(("__AFL_SHM_ID", id.clone()));
            info("main", &format!("Injected __AFL_SHM_ID for B = {}", id));
        }

        if let Some(s) = rv_shmem_b.take() {
            match crate::observers::shmem::rv_shmem::extract_id_size_and_clear(
                &s,
                "__DIFFRAY_TMP_RV_B",
            ) {
                Ok((id, size)) => {
                    child_envs_b.push(("__DIFFRAY_RV_SHM_ID_B", id.clone()));
                    child_envs_b.push(("__DIFFRAY_RV_SHM_ID_B_SIZE", size.clone()));
                    info(
                        "main",
                        &format!(
                            "Prepared __DIFFRAY_RV_SHM_ID_B for builder (id={} size={})",
                            id, size
                        ),
                    );
                }
                Err(e) => warn("main", &format!("failed to extract rv shmem id B: {}", e)),
            }
            rv_exec_observer_b.set_shmem(s);
            info(
                "main",
                "Attached rv shmem to executor observer B (second_return)",
            );
            // rv_shmem_snapshot.log removed per user request
        }

        // If per-target cov shmem was allocated, write __AFL_SHM_ID into parent env
        if let Some(s) = _cov_shmem_b.as_ref() {
            if let Err(e) = crate::observers::shmem::rv_shmem::write_shmem_to_env(s, "__AFL_SHM_ID")
            {
                warn(
                    "main",
                    &format!("failed to write __AFL_SHM_ID for B: {}", e),
                );
            }
            info("main", "Wrote __AFL_SHM_ID into parent env for executor B");
        }

        // Apply collected child envs to the builder in one pass
        for (k, v) in child_envs_b.drain(..) {
            builder_b = builder_b.env(k, v);
        }

        // Clear any parent env pollution that may remain
        for key in &["__DIFFRAY_RV_SHM_ID_B", "__DIFFRAY_RV_SHM_ID_B_SIZE"] {
            let _ = std::env::remove_var(key);
        }

        match builder_b.build_dynamic_map(map_observer_b, tuple_list!(rv_exec_observer_b)) {
            Ok(exec) => exec,
            Err(e) => {
                error(
                    "main",
                    &format!("Failed to create forkserver executor B: {}", e),
                );
                std::process::exit(1);
            }
        }
    } else {
        error(
            "main",
            "prog-b not provided; forkserver executor B cannot be created",
        );
        std::process::exit(1);
    };

    // Create a ShmemComboUpdater that will copy (A,B) coverage into the objective
    // buffer after the second observation. Moving shmem objects into the updater
    // ensures they remain alive for the duration of the run.
    let shmem_a = _cov_shmem_a.take();
    let shmem_b = _cov_shmem_b.take();
    // Move the objective backing into the updater so it owns and will free it.
    let updater = observers::shmem::ShmemComboUpdater::new(
        "shmem_combo_updater",
        num_edges,
        // combined_ptr is ignored when combined_backing is Some
        objective_ptr,
        Some(objective_box),
        shmem_a,
        shmem_b,
    );
    let diff_observers_list = tuple_list!(objective_observer, updater);

    let mut differential_executor = DiffExecutor::new(executor_a, executor_b, diff_observers_list);

    // Generator of printable bytearrays of max size 32
    let mut generator = RandPrintablesGenerator::new(nonzero!(32));

    // Handle initial seeds similar to libafl-fuzz:
    // If the State indicates we must load initial inputs (e.g. first run), try
    // to load them from the provided `-i` directory. If loading fails, fall
    // back to generating a small initial corpus. If the State does not require
    // initial seeds, generate random initial inputs.
    if state.must_load_initial_inputs() {
        if let Some(input_dir) = &args.input_dir {
            if input_dir.exists() && input_dir.is_dir() {
                if args.verbose > 0 {
                    info(
                        "main",
                        &format!("Loading initial seeds from directory: {:?}", input_dir),
                    );
                }
                state
                    .load_initial_inputs(
                        &mut fuzzer,
                        &mut differential_executor,
                        &mut mgr,
                        &[input_dir.clone()],
                    )
                    .unwrap_or_else(|err| panic!("Failed to load initial corpus: {:?}", err));

                if args.verbose > 0 {
                    info(
                        "main",
                        &format!(
                            "Successfully imported {} input files",
                            state.corpus().count()
                        ),
                    );
                }
            } else {
                error(
                    "main",
                    &format!("Error: specified seed directory does not exist or is not a directory: {:?}", input_dir),
                );
                std::process::exit(1);
            }
        } else {
            if args.verbose > 0 {
                warn(
                    "main",
                    "State requires initial seeds but -i was not provided; generating random initial inputs instead",
                );
            }
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
    } else {
        if args.verbose > 0 {
            info(
                "main",
                "No initial seeds required; generating random inputs",
            );
        }
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

    // Add tokens metadata, record start time and notify fuzzer that seed loading is done
    state.add_metadata(tokens);
    *state.start_time_mut() = current_time();

    // Setup a mutational stage with a basic bytes mutator
    let mutator = HavocScheduledMutator::new(havoc_mutations());
    // Stages: only the mutational stage
    let mut stages = tuple_list!(StdMutationalStage::new(mutator));

    if args.verbose > 0 {
        info("main", "Starting fuzzing...");
    }
    // Unlimited fuzz loop: use the standard fuzz_loop (no time/iteration limits)
    if args.verbose > 0 {
        info("main", "Running unlimited fuzzing loop");
    }

    // This call will run indefinitely (or until the restarter/manager stops it)
    match fuzzer.fuzz_loop(
        &mut stages,
        &mut differential_executor,
        &mut state,
        &mut mgr,
    ) {
        Ok(_) => return Ok(()),
        Err(e) => {
            error("main", &format!("Error in fuzz loop: {}", e));
            return Err(Box::new(e));
        }
    }
}
