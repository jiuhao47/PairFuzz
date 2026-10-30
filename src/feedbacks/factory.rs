use crate::{
    driver::args::RunMode,
    feedbacks::{
        differential_objective::DifferentialObjective, pairfuzz_feedback::PairFuzzFeedback,
        semantic::ResultPolicy, CrashObjective, DiffObjective,
    },
    observers::ReturnValueObserver,
};
use libafl::{
    feedback_and_fast, feedback_or,
    feedbacks::{EagerOrFeedback, FastAndFeedback, MaxMapFeedback},
    observers::{CanTrack, MapObserver},
};
use libafl_bolts::{AsIter, Named, ToSlice};

pub(crate) fn build_feedback<C>(
    mode: RunMode,
    map_a: &C,
    map_b: &C,
    rv_a: &ReturnValueObserver,
    rv_b: &ReturnValueObserver,
) -> PairFuzzFeedback<C>
where
    C: CanTrack + MapObserver + for<'it> AsIter<'it, Item = C::Entry> + Named,
    C::Entry: Into<u8> + Copy,
{
    match mode {
        RunMode::SinglePl => PairFuzzFeedback::new_single_pl(map_a, map_b),
        RunMode::ComboPl => PairFuzzFeedback::new_combo_pl(map_a, map_b),
        RunMode::SingleRv => PairFuzzFeedback::new_single_rv(rv_a, rv_b, map_a, map_b),
        RunMode::SingleRvCov => PairFuzzFeedback::new_single_rv_cov(rv_a, rv_b, map_a, map_b),
        RunMode::ComboRv => PairFuzzFeedback::new_combo_rv(rv_a, rv_b, map_a, map_b),
        RunMode::ComboCov => PairFuzzFeedback::new_combo_cov(map_a, map_b),
        RunMode::SingleCov | RunMode::ReplayCov => PairFuzzFeedback::new_single_cov(map_a, map_b),
    }
}

pub(crate) fn build_fast_objective<A, AO, B, BO>(
    map_a: &A,
    map_b: &B,
    rv_a: &ReturnValueObserver,
    rv_b: &ReturnValueObserver,
    policy: ResultPolicy,
) -> DifferentialObjective<A, AO, B, BO>
where
    A: Named + AsRef<AO>,
    B: Named + AsRef<BO>,
{
    DifferentialObjective::new(map_a, map_b, rv_a, rv_b, policy)
}

pub(crate) type Objective<C> =
    FastAndFeedback<EagerOrFeedback<CrashObjective, DiffObjective>, MaxMapFeedback<C, C>>;

pub(crate) fn build_objective<C>(
    objective_map: &C,
    rv_a: &ReturnValueObserver,
    rv_b: &ReturnValueObserver,
) -> Objective<C>
where
    C: CanTrack
        + AsRef<C>
        + Named
        + MapObserver<Entry = u8>
        + for<'it> ToSlice<'it, Entry = u8>
        + for<'it> AsIter<'it, Item = u8>,
{
    let difference = DiffObjective::new("zero_nonzero", &[rv_a, rv_b]);
    let map_difference = MaxMapFeedback::new(objective_map);
    feedback_and_fast!(
        feedback_or!(CrashObjective::new(), difference),
        map_difference
    )
}

#[cfg(test)]
mod tests {
    use super::{build_feedback, build_objective};
    use crate::{
        driver::args::RunMode,
        feedbacks::{pairfuzz_feedback::Mode, semantic::ResultPolicy},
        observers::ReturnValueObserver,
    };
    use libafl::observers::{HitcountsMapObserver, StdMapObserver};
    use libafl_bolts::Named;

    #[test]
    fn feedback_factory_maps_every_run_mode() {
        let map_a = HitcountsMapObserver::new(StdMapObserver::differential_owned(
            "first-edges",
            vec![0u8; 8],
        ));
        let map_b = HitcountsMapObserver::new(StdMapObserver::differential_owned(
            "second-edges",
            vec![0u8; 8],
        ));
        let rv_a = ReturnValueObserver::new("first_return");
        let rv_b = ReturnValueObserver::new("second_return");

        let cases = [
            (RunMode::SingleCov, Mode::SingleCov),
            (RunMode::SingleRv, Mode::SingleRv),
            (RunMode::SinglePl, Mode::SinglePL),
            (RunMode::SingleRvCov, Mode::SingleRvCov),
            (RunMode::ReplayCov, Mode::SingleCov),
            (RunMode::ComboCov, Mode::ComboCov),
            (RunMode::ComboRv, Mode::ComboRv),
            (RunMode::ComboPl, Mode::ComboPL),
        ];

        for (run_mode, expected) in cases {
            let feedback = build_feedback(run_mode, &map_a, &map_b, &rv_a, &rv_b);
            assert_eq!(feedback.mode, expected, "run mode {run_mode}");
        }
    }

    #[test]
    fn objective_factory_preserves_zero_nonzero_coverage_gate() {
        let objective_map = HitcountsMapObserver::new(StdMapObserver::differential_owned(
            "objective-edges",
            vec![0u8; 16],
        ));
        let rv_a = ReturnValueObserver::new("first_return");
        let rv_b = ReturnValueObserver::new("second_return");

        let objective = build_objective(&objective_map, &rv_a, &rv_b);
        let name = objective.name();
        let crash = name.find("CrashObjective").unwrap();
        let diff = name.find("DiffObjective(zero_nonzero)").unwrap();
        let map = name.find("objective-edges").unwrap();

        assert!(name.starts_with("Fast AND (Eager OR ("));
        assert!(crash < diff && diff < map);
    }

    #[test]
    fn fast_objective_factory_uses_atomic_objective() {
        let map_a = HitcountsMapObserver::new(StdMapObserver::differential_owned(
            "first-edges",
            vec![0u8; 8],
        ));
        let map_b = HitcountsMapObserver::new(StdMapObserver::differential_owned(
            "second-edges",
            vec![0u8; 8],
        ));
        let rv_a = ReturnValueObserver::new("first_return");
        let rv_b = ReturnValueObserver::new("second_return");

        let objective = crate::driver::pipeline::build_direct_objective(
            &map_a,
            &map_b,
            &rv_a,
            &rv_b,
            ResultPolicy::ZeroNonzero,
        );

        assert_eq!(objective.name().as_ref(), "DifferentialObjective");
    }
}

#[cfg(test)]
mod lifecycle_regressions {
    use super::*;
    use libafl::{
        corpus::{InMemoryCorpus, Testcase},
        events::NopEventManager,
        executors::ExitKind,
        feedbacks::Feedback,
        inputs::BytesInput,
        observers::{HitcountsMapObserver, Observer, StdMapObserver},
        state::StdState,
        HasMetadata,
    };
    use libafl_bolts::{
        rands::StdRand,
        shmem::{ShMemProvider, StdShMemProvider},
        tuples::tuple_list,
    };

    #[test]
    fn rejected_legacy_candidates_do_not_annotate_later_solutions() {
        for rejected_crash in [true, false] {
            let map =
                HitcountsMapObserver::new(StdMapObserver::differential_owned("obj", vec![1_u8, 0]));
            let a = ReturnValueObserver::new("a");
            let b = ReturnValueObserver::new("b");
            let mut objective = build_objective(&map, &a, &b);
            let mut feedback = ();
            let mut state = StdState::new(
                StdRand::with_seed(1),
                InMemoryCorpus::<BytesInput>::new(),
                InMemoryCorpus::new(),
                &mut feedback,
                &mut objective,
            )
            .unwrap();
            let input = BytesInput::new(vec![0]);
            let mut manager = NopEventManager::new();
            let mut provider = StdShMemProvider::new().unwrap();
            // Accept one initial crash, then reject a semantic/crash candidate on its covered map.
            for (index, (crash, coverage)) in [
                (true, vec![1_u8, 0]),
                (rejected_crash, vec![1_u8, 0]),
                (!rejected_crash, vec![0_u8, 1]),
            ]
            .into_iter()
            .enumerate()
            {
                let map =
                    HitcountsMapObserver::new(StdMapObserver::differential_owned("obj", coverage));
                let mut a = ReturnValueObserver::new("a");
                let mut b = ReturnValueObserver::new("b");
                for (obs, value) in [(&mut a, 0_u64), (&mut b, 1_u64)] {
                    let mut shm = provider.new_shmem(16).unwrap();
                    shm[..8].copy_from_slice(&value.to_ne_bytes());
                    shm[8..16].copy_from_slice(&1_u64.to_ne_bytes());
                    obs.set_shmem(shm).unwrap();
                    obs.post_exec(
                        &mut state,
                        &input,
                        &if crash { ExitKind::Crash } else { ExitKind::Ok },
                    )
                    .unwrap();
                }
                let observers = tuple_list!(map, a, b);
                let exit = if crash { ExitKind::Crash } else { ExitKind::Ok };
                let interesting = objective
                    .is_interesting(&mut state, &mut manager, &input, &observers, &exit)
                    .unwrap();
                assert_eq!(interesting, index != 1);
                if interesting {
                    let mut testcase = Testcase::new(input.clone());
                    objective
                        .append_metadata(&mut state, &mut manager, &observers, &mut testcase)
                        .unwrap();
                    let info = testcase
                        .metadata::<crate::feedbacks::objectives::ExitInfo>()
                        .unwrap();
                    assert_eq!(
                        info.crash.is_some(),
                        crash,
                        "stale crash metadata at {index}"
                    );
                    assert_eq!(
                        info.rv_a,
                        if crash { None } else { Some(0) },
                        "stale RV metadata at {index}"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod missing_result_regression {
    use super::*;
    use libafl::{
        corpus::InMemoryCorpus, events::NopEventManager, executors::ExitKind, feedbacks::Feedback,
        inputs::BytesInput, observers::Observer, state::StdState,
    };
    use libafl_bolts::{
        rands::StdRand,
        shmem::{ShMemProvider, StdShMemProvider},
        tuples::tuple_list,
    };
    #[test]
    fn legacy_zero_nonzero_distinguishes_missing_from_published_zero() {
        let a = ReturnValueObserver::new("a");
        let mut b = ReturnValueObserver::new("b");
        let mut provider = StdShMemProvider::new().unwrap();
        let mut shm = provider.new_shmem(16).unwrap();
        shm[..8].copy_from_slice(&0_u64.to_ne_bytes());
        shm[8..16].copy_from_slice(&1_u64.to_ne_bytes());
        b.set_shmem(shm).unwrap();
        let mut objective = DiffObjective::new("zero_nonzero", &[&a, &b]);
        let mut feedback = ();
        let mut state = StdState::new(
            StdRand::with_seed(1),
            InMemoryCorpus::<BytesInput>::new(),
            InMemoryCorpus::new(),
            &mut feedback,
            &mut objective,
        )
        .unwrap();
        let input = BytesInput::new(vec![0]);
        b.post_exec(&mut state, &input, &ExitKind::Ok).unwrap();
        let observers = tuple_list!(a, b);
        assert!(objective
            .is_interesting(
                &mut state,
                &mut NopEventManager::new(),
                &input,
                &observers,
                &ExitKind::Ok
            )
            .unwrap());
    }
}
