use clap::Parser;
use libafl::events::{AwaitRestartSafe, SendExiting};
use libafl::monitors::OnDiskJsonMonitor;
use libafl::ExecutesInput;
use libafl::{
    corpus::{Corpus, InMemoryCorpus, InMemoryOnDiskCorpus},
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
use libafl::executors::HasObservers;
use libafl::executors::StdChildArgs;
use libafl::observers::MapObserver;
use libafl::state::HasStartTime;
use libafl_bolts::shmem::{ShMemProvider, StdShMemProvider};
use libafl_bolts::tuples::MatchNameRef;
use libafl_bolts::AsIter;
use libafl_bolts::StdTargetArgs;
use libafl_bolts::{current_time, nonzero, rands::StdRand, tuples::tuple_list};
use mimalloc::MiMalloc;
use serde_json::json;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
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
    // Parse CLI arguments and configure logging to use `args.log_dir` so
    // logs are written into the user-specified directory instead of the repo root.
    let args = Args::parse();
    init_from_verbose(args.verbose, args.log_dir.clone());
    // Emit a compact summary of arguments/environment for diagnostics.
    log_working_state(&args);
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
        // removed verbose log: "Logs will be written to directory..." per request
    } else if args.verbose > 0 {
        info("main", "Detailed output enabled");
    }

    // Run the main logic in a fallible function to centralize error handling
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
    let mut rv_exec_observer_a = ReturnValueObserver::new("first_return");
    let mut rv_exec_observer_b = ReturnValueObserver::new("second_return");

    // Monitoring setup: how stats are displayed/recorded.
    // (previously had an always-on predicate here; replaced by a controlled predicate)

    let monitors = {
        // Place the fuzzer JSON stats inside the log directory if provided.
        let json_log_path = args
            .log_dir
            .as_ref()
            .map(|d| d.join("fuzzer_stats.json"))
            .unwrap_or_else(|| PathBuf::from("fuzzer_stats.json"));
        // JSON monitor: 15s heartbeat and skip writes caused by new testcase/objective.
        // We implement the predicate as a plain function that uses an atomic to
        // store the last write time (seconds) so the predicate can be a function
        // pointer (which implements Debug) and won't capture a closure type.
        static LAST_JSON_WRITE: AtomicU64 = AtomicU64::new(0);
        static JSON_HEARTBEAT_SECS: AtomicU64 = AtomicU64::new(15);

        // Store configured heartbeat from CLI into the atomic so the predicate
        // can read it. `args` is in scope here.
        JSON_HEARTBEAT_SECS.store(args.json_heartbeat_secs, Ordering::Relaxed);

        fn json_monitor_predicate(
            _client_stats_manager: &mut libafl::monitors::stats::ClientStatsManager,
        ) -> bool {
            let now = current_time().as_secs();
            let last = LAST_JSON_WRITE.load(Ordering::Relaxed);
            let heartbeat = JSON_HEARTBEAT_SECS.load(Ordering::Relaxed);
            // heartbeat interval
            if now.saturating_sub(last) < heartbeat {
                return false;
            }

            LAST_JSON_WRITE.store(now, Ordering::Relaxed);
            true
        }

        let json_mon = OnDiskJsonMonitor::new(
            json_log_path,
            json_monitor_predicate as fn(&mut libafl::monitors::stats::ClientStatsManager) -> bool,
        );
        // Also include a simple stdout monitor; controlled by the CLI flag `--no-simple-monitor`.
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
        crate::driver::args::RunMode::SingleRvCov => {
            crate::feedbacks::diffray_feedback::DiffrayFeedback::new_single_rv_cov(
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
        crate::driver::args::RunMode::ReplayCov => {
            // Replay mode uses the same coverage-only feedback for now;
            // the execution driver will handle replay semantics later.
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
            // Enable child debug output when verbosity is very high (-vvv)
            .debug_child(args.verbose >= 3)
            .coverage_map_size(num_edges)
            .timeout(Duration::from_millis(args.timeout_ms));

        // Collect child envs to inject into the builder in a single pass.
        let mut child_envs_a: Vec<(&str, String)> = Vec::new();

        // If a per-target coverage shmem was allocated, add its id to the child's env list
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
                    // Queue env vars for injection into the child's environment
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
            // Attach the shmem to the executor-side return-value observer
            rv_exec_observer_a.set_shmem(s);
            info(
                "main",
                "Attached rv shmem to executor observer A (first_return)",
            );
            // note: rv_shmem snapshot logging removed by request
        }

        // If a per-target coverage shmem exists, write its id into the parent env
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

        // Apply collected child envs to the builder in one pass
        for (k, v) in child_envs_a.drain(..) {
            builder_a = builder_a.env(k, v);
        }

        // Remove temporary parent environment variables to avoid pollution
        for key in &["__DIFFRAY_RV_SHM_ID_A", "__DIFFRAY_RV_SHM_ID_A_SIZE"] {
            let _ = std::env::remove_var(key);
        }

        // Build executor A with dynamic coverage map sizing; exit on failure.
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
        // Validate target binary for executor B and detect its capabilities
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

        // Determine persistent setting for B: CLI flags override detected capability.
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
            // Enable child debug output when verbosity is very high (-vvv)
            .debug_child(args.verbose >= 3)
            .coverage_map_size(num_edges)
            .timeout(Duration::from_millis(args.timeout_ms));

        // Collect child envs for B to inject later in one pass
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
            // note: rv_shmem snapshot logging removed by request
        }

        // If a per-target coverage shmem exists, write its id into the parent env
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

        // Apply collected child envs to the builder
        for (k, v) in child_envs_b.drain(..) {
            builder_b = builder_b.env(k, v);
        }

        // Remove temporary parent environment variables
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

    // Create a ShmemComboUpdater that copies (A,B) coverage into the objective
    // buffer. Move shmem objects into the updater to keep them alive for the run.
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
        // Replay mode: enumerate files in `input_dir`, sort by birth time,
        // execute each input once (using fuzzer.execute_input) and append
        // a JSONL record compatible with `fuzzer_stats.json` for each run.
        if let Some(input_dir) = &args.input_dir {
            if !input_dir.exists() || !input_dir.is_dir() {
                error(
                    "main",
                    &format!(
                        "Replay mode: specified input path is not a directory: {:?}",
                        input_dir
                    ),
                );
                std::process::exit(1);
            }

            // Collect files (non-recursive) with best-effort birth time
            let mut files: Vec<(PathBuf, f64)> = Vec::new();
            for entry in
                fs::read_dir(input_dir).unwrap_or_else(|e| panic!("read_dir failed: {}", e))
            {
                if let Ok(ent) = entry {
                    let p = ent.path();
                    if !p.is_file() {
                        continue;
                    }
                    let md = match fs::metadata(&p) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    // Best-effort: prefer modified() (mtime) which is reliable on this FS,
                    // fallback to created(), else now(). Using modified() avoids cases
                    // where created() returns a constant/invalid value.
                    let st = md
                        .modified()
                        .or_else(|_| md.created())
                        .unwrap_or_else(|_| SystemTime::now());
                    let secs = st
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_else(|_| Duration::from_secs(0))
                        .as_secs_f64();
                    files.push((p, secs));
                }
            }
            if files.is_empty() {
                info("main", "ReplayCov: no input files found in directory");
                return Ok(());
            }

            // Sort by birth time (ascending), tie-break by path
            files.sort_by(|a, b| {
                match a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal) {
                    std::cmp::Ordering::Equal => a.0.cmp(&b.0),
                    ord => ord,
                }
            });

            let t0 = files.first().map(|f| f.1).unwrap_or(0.0);

            // Prepare JSON log path (same as monitor uses)
            let json_log_path = args
                .log_dir
                .as_ref()
                .map(|d| d.join("fuzzer_stats.json"))
                .unwrap_or_else(|| PathBuf::from("fuzzer_stats.json"));

            // Ensure parent dir exists
            if let Some(parent) = json_log_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            // Initialize running union maps for each feedback map so we can
            // report a non-decreasing global coverage value (union of seen hits).
            // For each map referenced by the feedback, create a union buffer
            // pre-filled with the observer's `initial()` value so comparisons
            // to detect non-initial entries work correctly.
            let mut union_maps: Vec<Vec<u8>> = Vec::new();
            for handle in feedback.map_refs.iter() {
                if let Some(obs) = differential_executor.observers().get(handle) {
                    let initial = obs.initial();
                    let initial_u8: u8 = initial.into();
                    union_maps.push(vec![initial_u8; num_edges]);
                } else {
                    // fallback: zero-initialized if observer missing
                    union_maps.push(vec![0u8; num_edges]);
                }
            }

            // Iterate and execute
            let mut seen = 0usize;
            for (idx, (path, birth)) in files.into_iter().enumerate() {
                // read file
                let bytes = match fs::read(&path) {
                    Ok(b) => b,
                    Err(e) => {
                        warn(
                            "main",
                            &format!("Skipping unreadable file {:?}: {}", path, e),
                        );
                        continue;
                    }
                };
                // Build input and execute
                let input = libafl::inputs::BytesInput::from(bytes.as_slice());
                match fuzzer.execute_input(&mut state, &mut differential_executor, &mut mgr, &input)
                {
                    Ok(_exit) => {
                        // after execution, update running-union for each map and
                        // compute union non-initial counts. This produces a
                        // non-decreasing coverage metric over replayed inputs.
                        let mut user_stats = serde_json::Map::new();
                        let num_edges_usize = num_edges as u64;
                        for (midx, handle) in feedback.map_refs.iter().enumerate() {
                            if let Some(obs) = differential_executor.observers().get(handle) {
                                let initial = obs.initial();
                                let initial_u8: u8 = initial.into();
                                // update union buffer with this execution's observations
                                for (i, item_ref) in obs.as_iter().enumerate() {
                                    let cur: u8 = (*item_ref).into();
                                    // take max so any non-initial value is preserved
                                    union_maps[midx][i] = union_maps[midx][i].max(cur);
                                }

                                // count union non-initial entries
                                let union_cnt: u64 = union_maps[midx]
                                    .iter()
                                    .filter(|&&v| v != initial_u8)
                                    .count()
                                    as u64;

                                // name from feedback
                                let name = feedback
                                    .names
                                    .get(midx)
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| format!("map-{}", midx));
                                // report Max to indicate this is a running/global count
                                let ratio = json!({"aggregator_op":"Max","value":{"Ratio":[union_cnt, num_edges_usize]}});
                                user_stats.insert(name, ratio);
                            }
                        }

                        // Compose JSONL record
                        let rel_secs = birth - t0;
                        let rec = json!({
                            "client_stats": {"0": {"corpus_size": idx + 1, "enabled": true, "executions": idx + 1, "user_stats": user_stats}},
                            "clients": 1,
                            "corpus": idx + 1,
                            "executions": idx + 1,
                            "run_time": {"secs": rel_secs}
                        });

                        // append to file
                        let mut f = OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&json_log_path)
                            .unwrap_or_else(|e| panic!("failed open json log: {}", e));
                        writeln!(f, "{}", rec.to_string()).ok();
                        seen += 1;
                    }
                    Err(e) => {
                        warn("main", &format!("Execution error for {:?}: {}", path, e));
                    }
                }
            }

            info("main", &format!("ReplayCov: processed {} inputs", seen));
            // Ensure the event manager can perform shutdown actions (e.g., flush monitors,
            // notify restarter). This prevents background monitor/restarter state from
            // keeping the process alive after replaying inputs.
            if let Err(e) = mgr.on_shutdown() {
                warn("main", &format!("Error during manager shutdown: {}", e));
            }
            // Also allow the manager to finalize any restart-safety work
            // (no-op for SimpleRestartingEventManager but kept for completeness).
            mgr.await_restart_safe();

            return Ok(());
        } else {
            error("main", "ReplayCov mode requires --input <dir>");
            std::process::exit(1);
        }
    }

    if args.verbose > 0 {
        info("main", "Starting fuzzing...");
    }
    // Unlimited fuzz loop: call the standard fuzzer loop (runs until stopped)
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

// Log a concise snapshot of the current working state and parsed args.
// Keep messages English-only per repository policy and use existing
// logging helpers (info/warn/error) so output goes to the configured logger.
fn log_working_state(args: &Args) {
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
