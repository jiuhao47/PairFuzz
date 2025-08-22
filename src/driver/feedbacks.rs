use crate::driver::args::ModeOption;
use crate::feedbacks::ReturnValueFeedback;
use crate::observers::ReturnValueObserver;

/// Wrap a feedback with the project's `LogMapFeedback` wrapper.
pub fn wrap_log<F>(fb: F, module: &'static str) -> crate::feedbacks::coverage::LogMapFeedback<F> {
    crate::feedbacks::coverage::LogMapFeedback::new(fb, module)
}

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
