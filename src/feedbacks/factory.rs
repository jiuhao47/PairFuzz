use crate::{
    driver::args::RunMode,
    feedbacks::{diffray_feedback::DiffrayFeedback, CrashObjective, DiffObjective},
    observers::ReturnValueObserver,
};
use libafl::{
    feedback_and_fast, feedback_or_fast,
    feedbacks::{FastAndFeedback, FastOrFeedback, MaxMapFeedback},
    observers::{CanTrack, MapObserver},
};
use libafl_bolts::{AsIter, Named, ToSlice};

pub(crate) fn build_feedback<C>(
    mode: RunMode,
    map_a: &C,
    map_b: &C,
    rv_a: &ReturnValueObserver,
    rv_b: &ReturnValueObserver,
) -> DiffrayFeedback<C>
where
    C: CanTrack + MapObserver + for<'it> AsIter<'it, Item = C::Entry> + Named,
    C::Entry: Into<u8> + Copy,
{
    match mode {
        RunMode::SinglePl => DiffrayFeedback::new_single_pl(map_a, map_b),
        RunMode::ComboPl => DiffrayFeedback::new_combo_pl(map_a, map_b),
        RunMode::SingleRv => DiffrayFeedback::new_single_rv(rv_a, rv_b, map_a, map_b),
        RunMode::SingleRvCov => DiffrayFeedback::new_single_rv_cov(rv_a, rv_b, map_a, map_b),
        RunMode::ComboRv => DiffrayFeedback::new_combo_rv(rv_a, rv_b, map_a, map_b),
        RunMode::ComboCov => DiffrayFeedback::new_combo_cov(map_a, map_b),
        RunMode::SingleCov | RunMode::ReplayCov => DiffrayFeedback::new_single_cov(map_a, map_b),
    }
}

pub(crate) type Objective<C> =
    FastAndFeedback<FastOrFeedback<CrashObjective, DiffObjective>, MaxMapFeedback<C, C>>;

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
        feedback_or_fast!(CrashObjective::new(), difference),
        map_difference
    )
}

#[cfg(test)]
mod tests {
    use super::{build_feedback, build_objective};
    use crate::{
        driver::args::RunMode, feedbacks::diffray_feedback::Mode, observers::ReturnValueObserver,
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
    fn objective_factory_preserves_zero_nonzero_fast_composition() {
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

        assert!(name.starts_with("Fast AND (Fast OR ("));
        assert!(crash < diff && diff < map);
    }
}
