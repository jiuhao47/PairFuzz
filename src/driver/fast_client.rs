use std::{fs, time::Duration};

use libafl::{
    common::HasMetadata,
    corpus::{CachedOnDiskCorpus, OnDiskCorpus},
    events::SimpleRestartingEventManager,
    executors::DiffExecutor,
    feedbacks::MaxMapFeedback,
    fuzzer::{Fuzzer, StdFuzzer},
    generators::RandPrintablesGenerator,
    inputs::BytesInput,
    mutators::{havoc_mutations, tokens_mutations, HavocScheduledMutator, StdMOptMutator, Tokens},
    observers::{CanTrack, TimeObserver},
    schedulers::{
        powersched::PowerSchedule, IndexesLenTimeMinimizerScheduler, StdWeightedScheduler,
    },
    stages::{
        afl_stats::{AflStatsStage, CalibrationTime, FuzzTime},
        time_tracker::TimeTrackingStageWrapper,
        CalibrationStage, StdPowerMutationalStage,
    },
    state::{HasStartTime, StdState},
};
use libafl_bolts::{
    core_affinity::CoreId,
    current_time, nonzero,
    rands::StdRand,
    shmem::{ShMemProvider, StdShMemProvider},
    tuples::{tuple_list, Merge},
};

use crate::{
    driver::{
        args::{Args, MutatorChoice, ScheduleChoice},
        client::normalize_fuzz_result,
        pipeline::{ClientIdentity, FastMutationalStage},
        target::{build_executor, names, TargetRole, TargetSpec},
    },
    observers::ReturnValueObserver,
};

pub(crate) fn run_fast_client(
    args: &Args,
    identity: ClientIdentity,
) -> Result<(), Box<dyn std::error::Error>> {
    crate::feedbacks::signature::register_metadata_types();
    let paths = identity.fast_paths();
    fs::create_dir_all(paths.queue.parent().expect("queue has client parent"))?;

    let num_edges = crate::driver::constants::MAP_SIZE;
    let mut shmem_provider = StdShMemProvider::new()?;
    let (cov_shmem_a, ptr_a, cov_id_a) =
        crate::observers::shmem::rv_shmem::try_alloc_cov_shmem_with_ptr(
            &mut shmem_provider,
            num_edges,
            names(TargetRole::A).coverage_temp_env,
        )?;
    let (cov_shmem_b, ptr_b, cov_id_b) =
        crate::observers::shmem::rv_shmem::try_alloc_cov_shmem_with_ptr(
            &mut shmem_provider,
            num_edges,
            names(TargetRole::B).coverage_temp_env,
        )?;

    let map_observer_a =
        unsafe { crate::driver::target::coverage_observer("first-edges", ptr_a, num_edges) }
            .track_indices();
    let map_observer_b =
        unsafe { crate::driver::target::coverage_observer("second-edges", ptr_b, num_edges) };
    let time_observer = TimeObserver::new("time");
    let rv_observer_a = ReturnValueObserver::new("first_return");
    let rv_observer_b = ReturnValueObserver::new("second_return");

    let calibration_feedback = MaxMapFeedback::with_name("fast-a", &map_observer_a);
    let calibration = TimeTrackingStageWrapper::<CalibrationTime, _, _>::new(
        CalibrationStage::new(&calibration_feedback),
    );
    let mut feedback = crate::driver::pipeline::build_fast_feedback(
        &map_observer_a,
        &map_observer_b,
        &time_observer,
    );
    let mut objective = crate::driver::pipeline::build_direct_objective(
        &map_observer_a,
        &map_observer_b,
        &rv_observer_a,
        &rv_observer_b,
        args.result_policy,
    );
    let scheduler_observer_a = map_observer_a.clone();

    let monitors = crate::driver::monitor::build_monitors(
        args.log_dir.as_deref(),
        args.json_heartbeat_secs,
        args.simple_monitor_enabled(),
    );
    let (maybe_state, mut mgr) =
        match SimpleRestartingEventManager::launch(monitors, &mut shmem_provider, true) {
            Ok(result) => result,
            Err(libafl::Error::ShuttingDown) => return Ok(()),
            Err(error) => return Err(Box::new(error)),
        };

    let mut rv_shmem_a = None;
    let mut rv_shmem_b = None;
    if args.prog_a.is_some() || args.prog_b.is_some() {
        rv_shmem_a = Some(
            crate::observers::shmem::rv_shmem::try_new_shmem_and_write_env(
                &mut shmem_provider,
                crate::observers::return_value::RESULT_TRANSPORT_SIZE,
                "__PAIRFUZZ_RV_SHM_ID_A",
            )?,
        );
        rv_shmem_b = Some(
            crate::observers::shmem::rv_shmem::try_new_shmem_and_write_env(
                &mut shmem_provider,
                crate::observers::return_value::RESULT_TRANSPORT_SIZE,
                "__PAIRFUZZ_RV_SHM_ID_B",
            )?,
        );
    }
    let mut tokens = Tokens::new().add_from_files(&args.dicts)?;
    let executor_a = build_executor(
        TargetSpec {
            role: TargetRole::A,
            program: args.prog_a.as_deref(),
            coverage_id: Some(&cov_id_a),
            coverage_shmem: Some(&cov_shmem_a),
            return_shmem: rv_shmem_a,
            persistent_requested: args.persistent_enabled(),
            verbose: args.verbose,
            timeout: Duration::from_millis(args.timeout_ms),
            num_edges,
        },
        map_observer_a,
        rv_observer_a,
        &mut shmem_provider,
        &mut tokens,
    );
    let executor_b = build_executor(
        TargetSpec {
            role: TargetRole::B,
            program: args.prog_b.as_deref(),
            coverage_id: Some(&cov_id_b),
            coverage_shmem: Some(&cov_shmem_b),
            return_shmem: rv_shmem_b,
            persistent_requested: args.persistent_enabled(),
            verbose: args.verbose,
            timeout: Duration::from_millis(args.timeout_ms),
            num_edges,
        },
        map_observer_b,
        rv_observer_b,
        &mut shmem_provider,
        &mut tokens,
    );
    let mut differential_executor =
        DiffExecutor::new(executor_a, executor_b, tuple_list!(time_observer));

    let mut state = maybe_state.unwrap_or_else(|| {
        StdState::new(
            StdRand::with_seed(identity.rng_seed),
            CachedOnDiskCorpus::<BytesInput>::new(&paths.queue, 1000).unwrap(),
            OnDiskCorpus::<BytesInput>::new(&paths.solutions).unwrap(),
            &mut feedback,
            &mut objective,
        )
        .unwrap()
    });
    let schedule = match args.schedule {
        ScheduleChoice::Explore => PowerSchedule::explore(),
        ScheduleChoice::Fast => PowerSchedule::fast(),
    };
    let weighted =
        StdWeightedScheduler::with_schedule(&mut state, &scheduler_observer_a, Some(schedule));
    let scheduler = IndexesLenTimeMinimizerScheduler::new(&scheduler_observer_a, weighted);
    let mut fuzzer = StdFuzzer::new(scheduler, feedback, objective);

    let mut generator = RandPrintablesGenerator::new(nonzero!(32));
    if state.must_load_initial_inputs() {
        if let Some(input_dir) = &args.input_dir {
            state.load_initial_inputs(
                &mut fuzzer,
                &mut differential_executor,
                &mut mgr,
                std::slice::from_ref(input_dir),
            )?;
        } else {
            state.generate_initial_inputs(
                &mut fuzzer,
                &mut differential_executor,
                &mut generator,
                &mut mgr,
                8,
            )?;
        }
    }
    state.add_metadata(tokens.clone());
    *state.start_time_mut() = current_time();

    let stats = AflStatsStage::builder()
        .stats_file(paths.stats)
        .plot_file(paths.plot)
        .core_id(CoreId(
            identity.core_id.unwrap_or(identity.stable_index as usize),
        ))
        .report_interval(Duration::from_secs(args.stats_interval_secs))
        .map_feedback(&calibration_feedback)
        .uses_autotokens(true)
        .tokens(&tokens)
        .banner("pairfuzz".to_owned())
        .version(env!("CARGO_PKG_VERSION").to_owned())
        .exec_timeout(args.timeout_ms)
        .target_mode("differential-forkserver".to_owned())
        .build()?;
    let mutation = match args.mutator {
        MutatorChoice::Havoc => {
            FastMutationalStage::Havoc(StdPowerMutationalStage::<_, _, BytesInput, _, _, _>::new(
                HavocScheduledMutator::new(havoc_mutations().merge(tokens_mutations())),
            ))
        }
        MutatorChoice::MOpt => {
            FastMutationalStage::MOpt(StdPowerMutationalStage::<_, _, BytesInput, _, _, _>::new(
                StdMOptMutator::new(
                    &mut state,
                    havoc_mutations().merge(tokens_mutations()),
                    7,
                    5,
                )?,
            ))
        }
    };
    let mutation = TimeTrackingStageWrapper::<FuzzTime, _, _>::new(mutation);
    let mut stages = tuple_list!(calibration, mutation, stats);

    normalize_fuzz_result(fuzzer.fuzz_loop(
        &mut stages,
        &mut differential_executor,
        &mut state,
        &mut mgr,
    ))
    .map_err(Into::into)
}
