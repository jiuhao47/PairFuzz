use libafl::common::HasMetadata;
use libafl::state::HasStartTime;
use libafl::{
    corpus::{Corpus, InMemoryCorpus, InMemoryOnDiskCorpus},
    events::SimpleRestartingEventManager,
    executors::DiffExecutor,
    fuzzer::{Fuzzer, StdFuzzer},
    generators::RandPrintablesGenerator,
    mutators::{havoc_mutations::havoc_mutations, scheduled::HavocScheduledMutator},
    schedulers::QueueScheduler,
    stages::StdMutationalStage,
    state::{HasCorpus, StdState},
};
use libafl_bolts::shmem::{ShMemProvider, StdShMemProvider};
use libafl_bolts::{current_time, nonzero, rands::StdRand, tuples::tuple_list};
use std::{fs, time::Duration};

use crate::observers::ReturnValueObserver;
// CLI args and logging helpers
use crate::driver::args::Args;
use crate::utils::logging::{error, info, warn};

pub(super) fn normalize_fuzz_result<T>(
    result: Result<T, libafl::Error>,
) -> Result<(), libafl::Error> {
    match result {
        Ok(_) | Err(libafl::Error::ShuttingDown) => Ok(()),
        Err(error) => Err(error),
    }
}

#[expect(clippy::too_many_lines)]
pub(crate) fn run_legacy_client(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    crate::feedbacks::signature::register_metadata_types();

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

    // Startup configuration logs were removed to reduce verbosity.
    // Main fuzzing setup follows.

    // Fuzzing configuration
    // num_edges: coverage map size (from driver::constants::MAP_SIZE)
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
    let rv_exec_observer_a = ReturnValueObserver::new("first_return");
    let rv_exec_observer_b = ReturnValueObserver::new("second_return");

    let monitors = crate::driver::monitor::build_monitors(
        args.log_dir.as_deref(),
        args.json_heartbeat_secs,
        args.simple_monitor_enabled(),
    );

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
        crate::driver::target::names(crate::driver::target::TargetRole::A).coverage_temp_env,
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
        crate::driver::target::names(crate::driver::target::TargetRole::B).coverage_temp_env,
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
    // Allocate a boxed objective buffer and keep ownership to avoid leaks; a raw pointer is
    // passed to the observer while the Box remains owned here.
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

    let mut feedback = crate::feedbacks::factory::build_feedback(
        args.mode,
        &map_observer_a,
        &map_observer_b,
        &rv_feedback_observer_a,
        &rv_feedback_observer_b,
    );

    let mut objective = crate::feedbacks::factory::build_objective(
        &objective_observer,
        &rv_exec_observer_a,
        &rv_exec_observer_b,
    );

    let (maybe_state, mut mgr) =
        match SimpleRestartingEventManager::launch(monitors, &mut shmem_provider, true) {
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
    let corpus = InMemoryCorpus::new();
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
    // Clone `feedback` here so we can retain an owned copy for replay collection later.
    let mut fuzzer = StdFuzzer::new(scheduler, feedback.clone(), objective);

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

    let executor_a = crate::driver::target::build_executor(
        crate::driver::target::TargetSpec {
            role: crate::driver::target::TargetRole::A,
            program: args.prog_a.as_deref(),
            coverage_id: cov_shm_id_a.as_deref(),
            coverage_shmem: _cov_shmem_a.as_ref(),
            return_shmem: rv_shmem_a,
            persistent_requested: args.persistent_enabled(),
            verbose: args.verbose,
            timeout: Duration::from_millis(args.timeout_ms),
            num_edges,
        },
        map_observer_a,
        rv_exec_observer_a,
        &mut shmem_provider,
        &mut tokens,
    );

    let executor_b = crate::driver::target::build_executor(
        crate::driver::target::TargetSpec {
            role: crate::driver::target::TargetRole::B,
            program: args.prog_b.as_deref(),
            coverage_id: cov_shm_id_b.as_deref(),
            coverage_shmem: _cov_shmem_b.as_ref(),
            return_shmem: rv_shmem_b,
            persistent_requested: args.persistent_enabled(),
            verbose: args.verbose,
            timeout: Duration::from_millis(args.timeout_ms),
            num_edges,
        },
        map_observer_b,
        rv_exec_observer_b,
        &mut shmem_provider,
        &mut tokens,
    );

    // Create a ShmemComboUpdater that copies (A,B) coverage into the objective
    // buffer. Move shmem objects into the updater to keep them alive for the run.
    let shmem_a = _cov_shmem_a.take();
    let shmem_b = _cov_shmem_b.take();
    // Move the objective backing into the updater so it owns and will free it.
    let updater = crate::observers::shmem::ShmemComboUpdater::new(
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

    // Generator that produces printable byte arrays with max size 32
    let mut generator = RandPrintablesGenerator::new(nonzero!(32));

    // Handle initial seeds similar to libafl-fuzz: load from `-i` when required,
    // otherwise generate random initial inputs.
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

    // Add tokens metadata, record start time and mark that seed loading is complete
    state.add_metadata(tokens);
    *state.start_time_mut() = current_time();

    // Setup a mutational stage using HavocScheduledMutator
    let mutator = HavocScheduledMutator::new(havoc_mutations());
    // Use a single mutational stage
    let mut stages = tuple_list!(StdMutationalStage::new(mutator));

    if args.mode == crate::driver::args::RunMode::ReplayCov {
        crate::driver::replay::run_replay_cov(
            args.input_dir.as_deref(),
            args.log_dir.as_deref(),
            num_edges,
            &feedback,
            &mut state,
            &mut fuzzer,
            &mut differential_executor,
            &mut mgr,
        )?;
        return Ok(());
    }

    if args.verbose > 0 {
        info("main", "Starting fuzzing...");
    }
    // Unlimited fuzz loop: call the standard fuzzer loop (runs until stopped)
    if args.verbose > 0 {
        info("main", "Running unlimited fuzzing loop");
    }

    // This call will run indefinitely (or until the restarter/manager stops it)
    match normalize_fuzz_result(fuzzer.fuzz_loop(
        &mut stages,
        &mut differential_executor,
        &mut state,
        &mut mgr,
    )) {
        Ok(()) => return Ok(()),
        Err(e) => {
            error("main", &format!("Error in fuzz loop: {}", e));
            return Err(Box::new(e));
        }
    }
}

// Log a concise snapshot of the current working state and parsed args.
// Keep messages English-only per repository policy and use existing
// logging helpers (info/warn/error) so output goes to the configured logger.
pub(crate) fn log_working_state(args: &Args) {
    use std::env;
    info("main", "--- Working state ---");

    // Current working directory
    match env::current_dir() {
        Ok(pwd) => info("main", &format!("cwd: {:?}", pwd)),
        Err(e) => warn("main", &format!("cwd: <error: {}>", e)),
    }

    // Important environment variables
    let asan = env::var("ASAN_OPTIONS").unwrap_or_else(|_| "<not set>".to_string());
    info("main", &format!("env.ASAN_OPTIONS: {}", asan));

    // CLI args (one-per-line for readability)
    info("main", &format!("args.input_dir: {:?}", args.input_dir));
    info("main", &format!("args.output_dir: {:?}", args.output_dir));
    info("main", &format!("args.log_dir: {:?}", args.log_dir));
    info("main", &format!("args.verbose: {}", args.verbose));
    info("main", &format!("args.timeout_ms: {}", args.timeout_ms));
    info("main", &format!("args.mode: {}", args.mode));
    info("main", &format!("args.pipeline: {:?}", args.pipeline));
    info("main", &format!("args.schedule: {:?}", args.schedule));
    info("main", &format!("args.mutator: {:?}", args.mutator));
    info(
        "main",
        &format!("args.campaign_seed: {:?}", args.campaign_seed),
    );
    info("main", &format!("args.dicts: {:?}", args.dicts));
    info(
        "main",
        &format!("args.stats_interval_secs: {}", args.stats_interval_secs),
    );
    info(
        "main",
        &format!("args.persistent_mode: {}", args.persistent_mode),
    );
    info(
        "main",
        &format!("effective persistent: {}", args.persistent_enabled()),
    );
    info(
        "main",
        &format!("args.simple_monitor_mode: {}", args.simple_monitor_mode),
    );
    info(
        "main",
        &format!(
            "effective simple monitor enabled: {}",
            args.simple_monitor_enabled()
        ),
    );
    info("main", "--- End working state ---");

    // Check target program paths existence if provided (separate block)
    if let Some(prog_a) = &args.prog_a {
        if prog_a.exists() {
            info("main", &format!("prog-a: exists: {:?}", prog_a));
        } else {
            warn("main", &format!("prog-a: missing: {:?}", prog_a));
        }
    } else {
        info("main", "prog-a: not provided");
    }

    if let Some(prog_b) = &args.prog_b {
        if prog_b.exists() {
            info("main", &format!("prog-b: exists: {:?}", prog_b));
        } else {
            warn("main", &format!("prog-b: missing: {:?}", prog_b));
        }
    } else {
        info("main", "prog-b: not provided");
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_fuzz_result;

    #[test]
    fn shutdown_is_a_clean_fuzz_loop_result() {
        assert!(normalize_fuzz_result::<()>(Err(libafl::Error::shutting_down())).is_ok());
    }

    #[test]
    fn ordinary_fuzz_loop_errors_are_preserved() {
        let result = normalize_fuzz_result::<()>(Err(libafl::Error::illegal_state("broken")));
        assert!(result.is_err());
    }
}
