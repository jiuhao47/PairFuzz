use crate::observers::ReturnValueObserver;
use crate::utils::logging::debug;
use libafl::executors::ExitKind;
use libafl::feedbacks::{Feedback, StateInitializer};
use libafl::Error;
use libafl_bolts::tuples::{Handle, Handled, MatchNameRef};
use libafl_bolts::Named;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;

/// Combination feedback: existing behavior - a tuple of return values across observers
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReturnValueComboFeedback {
    observer_handles: Vec<Handle<ReturnValueObserver>>,
    seen: HashSet<Vec<Option<i32>>>,
    name: Cow<'static, str>,
    last: bool,
}

impl<S> StateInitializer<S> for ReturnValueComboFeedback {}

impl Named for ReturnValueComboFeedback {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl ReturnValueComboFeedback {
    pub fn new_from_observers(observers: &[&ReturnValueObserver]) -> Self {
        let handles: Vec<Handle<ReturnValueObserver>> =
            observers.iter().map(|o| o.handle()).collect();
        let names: Vec<String> = handles.iter().map(|h| h.name().to_string()).collect();
        let name = Cow::from(format!("ReturnValueCombo({})", names.join(",")));
        Self {
            observer_handles: handles,
            seen: HashSet::new(),
            name,
            last: false,
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
        let mut key: Vec<Option<i32>> = Vec::with_capacity(self.observer_handles.len());
        for h in &self.observer_handles {
            let obs = observers.get(h).ok_or_else(|| {
                Error::illegal_state(
                    "Observer referenced by ReturnValueComboFeedback not found in observers",
                )
            })?;
            key.push(obs.last_value());
        }

        let log_msg = format!("New return value combo found: {:?}", key);
        let inserted = self.seen.insert(key);
        #[cfg(feature = "track_hit_feedbacks")]
        {
            self.last = inserted;
        }
            if inserted {
                debug("return_value_feedback", &log_msg);
                Ok(true)
            } else {
            Ok(false)
        }
    }

    fn last_result(&self) -> Result<bool, Error> {
        #[cfg(feature = "track_hit_feedbacks")]
        {
            Ok(self.last)
        }
        #[cfg(not(feature = "track_hit_feedbacks"))]
        {
            Ok(false)
        }
    }
}

/// Single feedback: each distinct return value from any observer is interesting.
/// The feedback records observed values per-observer but treats them independently.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReturnValueSingleFeedback {
    observer_handles: Vec<Handle<ReturnValueObserver>>,
    seen: HashSet<(String, Option<i32>)>,
    name: Cow<'static, str>,
    last: bool,
}

/// Runtime-selectable wrapper that can be None, Single or Combo.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ReturnValueFeedback {
    None(Cow<'static, str>),
    Single(ReturnValueSingleFeedback),
    Combo(ReturnValueComboFeedback),
}

impl ReturnValueFeedback {
    pub fn new(mode: &str, observers: &[&ReturnValueObserver]) -> Self {
        match mode {
            "none" => ReturnValueFeedback::None(Cow::from("ReturnValueNone")),
            "single" => ReturnValueFeedback::Single(ReturnValueSingleFeedback::new_from_observers(
                observers,
            )),
            _ => {
                ReturnValueFeedback::Combo(ReturnValueComboFeedback::new_from_observers(observers))
            }
        }
    }
}

impl<S> StateInitializer<S> for ReturnValueFeedback {}

impl Named for ReturnValueFeedback {
    fn name(&self) -> &Cow<'static, str> {
        match self {
            ReturnValueFeedback::None(n) => n,
            ReturnValueFeedback::Single(s) => s.name(),
            ReturnValueFeedback::Combo(c) => c.name(),
        }
    }
}

impl<EM, I, OT, S> Feedback<EM, I, OT, S> for ReturnValueFeedback
where
    OT: libafl_bolts::tuples::MatchName,
{
    fn is_interesting(
        &mut self,
        state: &mut S,
        manager: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error> {
        match self {
            ReturnValueFeedback::None(_) => Ok(false),
            ReturnValueFeedback::Single(f) => {
                f.is_interesting(state, manager, input, observers, exit_kind)
            }
            ReturnValueFeedback::Combo(f) => {
                f.is_interesting(state, manager, input, observers, exit_kind)
            }
        }
    }

    fn last_result(&self) -> Result<bool, Error> {
        if cfg!(feature = "track_hit_feedbacks") {
            match self {
                ReturnValueFeedback::None(_) => Ok(false),
                ReturnValueFeedback::Single(s) => Ok(s.last),
                ReturnValueFeedback::Combo(c) => Ok(c.last),
            }
        } else {
            Ok(false)
        }
    }
}

impl<S> StateInitializer<S> for ReturnValueSingleFeedback {}

impl Named for ReturnValueSingleFeedback {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl ReturnValueSingleFeedback {
    pub fn new_from_observers(observers: &[&ReturnValueObserver]) -> Self {
        let handles: Vec<Handle<ReturnValueObserver>> =
            observers.iter().map(|o| o.handle()).collect();
        let names: Vec<String> = handles.iter().map(|h| h.name().to_string()).collect();
        let name = Cow::from(format!("ReturnValueSingle({})", names.join(",")));
        Self {
            observer_handles: handles,
            seen: HashSet::new(),
            name,
            last: false,
        }
    }
}

impl<EM, I, OT, S> Feedback<EM, I, OT, S> for ReturnValueSingleFeedback
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
        for h in &self.observer_handles {
            let obs = observers.get(h).ok_or_else(|| {
                Error::illegal_state(
                    "Observer referenced by ReturnValueSingleFeedback not found in observers",
                )
            })?;
            let key = (h.name().to_string(), obs.last_value());
            let log_msg = format!("New return value single found: {:?}", key);
            let inserted = self.seen.insert(key);
            #[cfg(feature = "track_hit_feedbacks")]
            {
                self.last = inserted;
            }
            if inserted {
                debug("return_value_feedback", &log_msg);
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn last_result(&self) -> Result<bool, Error> {
        #[cfg(feature = "track_hit_feedbacks")]
        {
            Ok(self.last)
        }
        #[cfg(not(feature = "track_hit_feedbacks"))]
        {
            Ok(false)
        }
    }
}

#[cfg(feature = "track_hit_feedbacks")]
impl ReturnValueFeedback {
    fn last_result(&self) -> Result<bool, Error> {
        match self {
            ReturnValueFeedback::None(_) => Ok(false),
            ReturnValueFeedback::Single(s) => Ok(s.last),
            ReturnValueFeedback::Combo(c) => Ok(c.last),
        }
    }
}
