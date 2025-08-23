use libafl::executors::ExitKind;
use libafl::feedbacks::{Feedback, StateInitializer};
use libafl::Error;
use libafl_bolts::Named;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NoopFeedback {
    name: Cow<'static, str>,
}

#[allow(dead_code)]
impl NoopFeedback {
    pub(crate) fn new() -> Self {
        Self {
            name: Cow::from("NoopFeedback"),
        }
    }
}

impl<S> StateInitializer<S> for NoopFeedback {}

impl Named for NoopFeedback {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<EM, I, OT, S> Feedback<EM, I, OT, S> for NoopFeedback {
    fn is_interesting(
        &mut self,
        _state: &mut S,
        _mgr: &mut EM,
        _input: &I,
        _observers: &OT,
        _exit: &ExitKind,
    ) -> Result<bool, Error> {
        Ok(false)
    }

    fn last_result(&self) -> Result<bool, Error> {
        Ok(false)
    }
}
