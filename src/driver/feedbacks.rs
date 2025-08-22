use crate::driver::args::ModeOption;
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
