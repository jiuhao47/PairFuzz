use crate::diff_feedback::DiffFeedback;
use crate::observers::ReturnValueObserver;
use libafl::executors::ExitKind;
use libafl::feedbacks::{Feedback, StateInitializer};
use libafl::Error;
use libafl_bolts::tuples::{Handle, Handled, MatchNameRef};
use libafl_bolts::Named;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;

/// Feedback that marks a testcase as interesting when it produces a new combination of
/// return values across the provided ReturnValueObservers.
///
/// Supports any number (>=1) of homogenous `ReturnValueObserver`s. For a single observer, any
/// new return value is considered interesting. For multiple observers, any new vector/tuple of
/// observed return values is considered interesting.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReturnValueComboFeedback {
    observer_handles: Vec<Handle<ReturnValueObserver>>,
    /// Set of already seen combinations
    seen: HashSet<Vec<Option<i32>>>,
    name: Cow<'static, str>,
}

impl<S> StateInitializer<S> for ReturnValueComboFeedback {}

impl Named for ReturnValueComboFeedback {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl ReturnValueComboFeedback {
    /// Create from a slice of observer references.
    pub fn new_from_observers(observers: &[&ReturnValueObserver]) -> Self {
        let handles: Vec<Handle<ReturnValueObserver>> =
            observers.iter().map(|o| o.handle()).collect();
        let names: Vec<String> = handles.iter().map(|h| h.name().to_string()).collect();
        let name = Cow::from(format!("ReturnValueCombo({})", names.join(",")));
        Self {
            observer_handles: handles,
            seen: HashSet::new(),
            name,
        }
    }

    /// Create from a vector of handles (if you already have them)
    pub fn new_from_handles(handles: Vec<Handle<ReturnValueObserver>>) -> Self {
        let names: Vec<String> = handles.iter().map(|h| h.name().to_string()).collect();
        let name = Cow::from(format!("ReturnValueCombo({})", names.join(",")));
        Self {
            observer_handles: handles,
            seen: HashSet::new(),
            name,
        }
    }
}

impl<EM, I, OT, S> Feedback<EM, I, OT, S> for ReturnValueComboFeedback
where
    OT: libafl_bolts::tuples::MatchName,
{
    fn is_interesting(
        &mut self,
        _state: &mut S,
        _manager: &mut EM,
        _input: &I,
        observers: &OT,
        _exit_kind: &ExitKind,
    ) -> Result<bool, Error> {
        // collect current observed values from each observer handle
        let mut key: Vec<Option<i32>> = Vec::with_capacity(self.observer_handles.len());
        pub use crate::feedbacks::ReturnValueComboFeedback;
        let obs = observers.get(h).ok_or_else(|| {
