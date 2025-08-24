use crate::driver::args::ModeOption;
use crate::feedbacks::two_state_map::TwoStateMapFeedback;
use crate::feedbacks::ReturnValueFeedback;
use crate::observers::ReturnValueObserver;

/// Build the ReturnValueFeedback wrapper according to selected mode.
pub fn make_rv_wrapper(
    mode: ModeOption,
    a: &ReturnValueObserver,
    b: &ReturnValueObserver,
) -> ReturnValueFeedback {
    match mode {
        ModeOption::None => ReturnValueFeedback::new("none", &[a, b]),
        ModeOption::Single => ReturnValueFeedback::new("single", &[a, b]),
        ModeOption::Combo => ReturnValueFeedback::new("combo", &[a, b]),
        ModeOption::MaxMin => ReturnValueFeedback::new("maxmin", &[a, b]),
    }
}
pub fn make_cov_wrapper<C>(
    mode: ModeOption,
    map_observer_a: &C,
    map_observer_b: &C,
) -> (TwoStateMapFeedback<C>, TwoStateMapFeedback<C>)
where
    C: libafl::observers::MapObserver + for<'it> libafl_bolts::AsIter<'it, Item = C::Entry>,
{
    let (check_high_a, check_low_a, check_high_b, check_low_b) = match mode {
        ModeOption::MaxMin => (true, false, false, true),
        ModeOption::None => (false, false, false, false),
        ModeOption::Single => (true, false, true, false),
        ModeOption::Combo => (true, false, true, false),
    };

    let high_delta_a = 1u8;
    let low_delta_a = 1u8;
    let high_delta_b = 1u8;
    let low_delta_b = 1u8;

    let coverage_fb_a = TwoStateMapFeedback::new_with_params(
        map_observer_a,
        high_delta_a,
        low_delta_a,
        check_high_a,
        check_low_a,
    );
    let coverage_fb_b = TwoStateMapFeedback::new_with_params(
        map_observer_b,
        high_delta_b,
        low_delta_b,
        check_high_b,
        check_low_b,
    );

    // Note: pre-allocation must be performed once state is available; caller should call
    // `ensure_initialized_with_observer` with a real `state` to allocate metadata.

    (coverage_fb_a, coverage_fb_b)
}
