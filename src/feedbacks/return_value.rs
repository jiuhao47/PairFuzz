use crate::observers::ReturnValueObserver;
// module-level debug helper `return_value_debug_log` will call into crate::utils::logging::debug
use libafl::corpus::Testcase;
use libafl::executors::ExitKind;
use libafl::feedbacks::{Feedback, StateInitializer};
use libafl::Error;
use libafl_bolts::tuples::{Handle, Handled, MatchNameRef};
use libafl_bolts::Named;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU8, Ordering};

/// Combination feedback: existing behavior - a tuple of return values across observers
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReturnValueComboFeedback {
    observer_handles: Vec<Handle<ReturnValueObserver>>,
    seen: HashSet<Vec<Option<i32>>>,
    name: Cow<'static, str>,
    last: bool,
}

// Metadata persisted into `State` so that return-value history survives restarts/crashes
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReturnValueComboStats {
    pub seen: HashSet<Vec<Option<i32>>>,
}
libafl_bolts::impl_serdeany!(ReturnValueComboStats);

impl<S> StateInitializer<S> for ReturnValueComboFeedback
where
    S: libafl::HasNamedMetadata,
{
    fn init_state(&mut self, state: &mut S) -> Result<(), Error> {
        // If metadata already exists, load it into the in-memory cache; otherwise register empty metadata
        let named = state.named_metadata_map();
        if let Some(md) = named.get::<ReturnValueComboStats>(&self.name) {
            self.seen = md.seen.clone();
            return_value_debug_log(
                "return_value_feedback",
                &format!(
                    "init_state: loaded ReturnValueComboStats for '{}' with {} entries",
                    self.name,
                    self.seen.len()
                ),
            );
        } else {
            state.add_named_metadata_checked(
                &self.name,
                ReturnValueComboStats {
                    seen: HashSet::new(),
                },
            )?;
            return_value_debug_log(
                "return_value_feedback",
                &format!(
                    "init_state: created empty ReturnValueComboStats for '{}'",
                    self.name
                ),
            );
        }
        Ok(())
    }
}

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

impl libafl::feedbacks::HasObserverHandle for ReturnValueComboFeedback {
    type Observer = ReturnValueObserver;

    fn observer_handle(&self) -> &Handle<Self::Observer> {
        // Return the first handle. The feedback is constructed from a non-empty
        // observer slice in practice (see callers in main.rs). If this ever
        // happens to be empty, that's a bug in the caller, so panic to make
        // the problem obvious during development.
        &self
            .observer_handles
            .get(0)
            .expect("ReturnValueComboFeedback has no observer handles")
    }
}

// Module-level gated debug logger for ReturnValueFeedback
// Enable via environment variable DIFFRAY_RETURN_VALUE_DEBUG=1 or DIFFRAY_RETURN_VALUE_DEBUG=true
fn return_value_debug_log(tag: &str, msg: &str) {
    if return_value_debug_enabled() {
        crate::utils::logging::debug(tag, msg);
    }
}

static RETURN_VALUE_DEBUG_FLAG: AtomicU8 = AtomicU8::new(0);

fn return_value_debug_enabled() -> bool {
    match RETURN_VALUE_DEBUG_FLAG.load(Ordering::Relaxed) {
        0 => {
            let enabled = match std::env::var("DIFFRAY_RETURN_VALUE_DEBUG") {
                Ok(v) => {
                    let ve = v.to_lowercase();
                    ve == "1" || ve == "true"
                }
                Err(_) => false,
            };
            RETURN_VALUE_DEBUG_FLAG.store(if enabled { 2 } else { 1 }, Ordering::Relaxed);
            enabled
        }
        1 => false,
        2 => true,
        _ => false,
    }
}

impl<EM, I, OT, S> Feedback<EM, I, OT, S> for ReturnValueComboFeedback
where
    OT: libafl_bolts::tuples::MatchName,
    S: libafl::HasNamedMetadata,
{
    fn is_interesting(
        &mut self,
        state: &mut S,
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

        // Check persisted State metadata first: if this combo is already persisted,
        // don't treat it as new. This keeps is_interesting aligned with append_metadata.
        let named = state.named_metadata_map();
        if let Some(md) = named.get::<ReturnValueComboStats>(&self.name) {
            if md.seen.contains(&key) {
                return Ok(false);
            }
        }

        let log_msg = format!("New return value combo found: {:?}", key);
        let inserted = self.seen.insert(key);
        #[cfg(feature = "track_hit_feedbacks")]
        {
            self.last = inserted;
        }
        if inserted {
            return_value_debug_log("return_value_feedback", &log_msg);
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

    fn append_metadata(
        &mut self,
        state: &mut S,
        _manager: &mut EM,
        _observers: &OT,
        _testcase: &mut Testcase<I>,
    ) -> Result<(), Error> {
        let stats_mut = state
            .named_metadata_map_mut()
            .get_mut::<ReturnValueComboStats>(&self.name)
            .ok_or_else(|| {
                Error::illegal_argument("ReturnValueComboFeedback: stats metadata missing")
            })?;
        let before = stats_mut.seen.len();
        for v in self.seen.iter() {
            stats_mut.seen.insert(v.clone());
        }
        let after = stats_mut.seen.len();
        return_value_debug_log(
            "return_value_feedback",
            &format!(
                "append_metadata: merged ReturnValueComboStats for '{}' (before={} after={})",
                self.name, before, after
            ),
        );
        Ok(())
    }
}

/// Single feedback: each distinct return value from any observer is interesting.
/// The feedback records observed values per-observer but treats them independently.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReturnValueSingleFeedback {
    observer_handles: Vec<Handle<ReturnValueObserver>>,
    // in-memory cache; persisted copy lives in State metadata
    seen: HashSet<(String, Option<i32>)>,
    name: Cow<'static, str>,
    last: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReturnValueSingleStats {
    pub seen: HashSet<(String, Option<i32>)>,
}
libafl_bolts::impl_serdeany!(ReturnValueSingleStats);

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

impl<S> StateInitializer<S> for ReturnValueFeedback
where
    S: libafl::HasNamedMetadata,
{
    fn init_state(&mut self, state: &mut S) -> Result<(), Error> {
        match self {
            ReturnValueFeedback::None(_) => Ok(()),
            ReturnValueFeedback::Single(s) => s.init_state(state),
            ReturnValueFeedback::Combo(c) => c.init_state(state),
        }
    }
}

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
    S: libafl::HasNamedMetadata,
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

    fn append_metadata(
        &mut self,
        state: &mut S,
        manager: &mut EM,
        observers: &OT,
        testcase: &mut Testcase<I>,
    ) -> Result<(), Error> {
        match self {
            ReturnValueFeedback::None(_) => Ok(()),
            ReturnValueFeedback::Single(s) => {
                s.append_metadata(state, manager, observers, testcase)
            }
            ReturnValueFeedback::Combo(c) => c.append_metadata(state, manager, observers, testcase),
        }
    }
}

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

impl libafl::feedbacks::HasObserverHandle for ReturnValueSingleFeedback {
    type Observer = ReturnValueObserver;

    fn observer_handle(&self) -> &Handle<Self::Observer> {
        &self
            .observer_handles
            .get(0)
            .expect("ReturnValueSingleFeedback has no observer handles")
    }
}

impl libafl::feedbacks::HasObserverHandle for ReturnValueFeedback {
    type Observer = ReturnValueObserver;

    fn observer_handle(&self) -> &Handle<Self::Observer> {
        match self {
            ReturnValueFeedback::None(_) => {
                panic!("ReturnValueFeedback::None has no observer handle")
            }
            ReturnValueFeedback::Single(s) => s.observer_handle(),
            ReturnValueFeedback::Combo(c) => c.observer_handle(),
        }
    }
}

// ... existing code ...

impl<S> StateInitializer<S> for ReturnValueSingleFeedback
where
    S: libafl::HasNamedMetadata,
{
    fn init_state(&mut self, state: &mut S) -> Result<(), Error> {
        let named = state.named_metadata_map();
        if let Some(md) = named.get::<ReturnValueSingleStats>(&self.name) {
            self.seen = md.seen.clone();
        } else {
            state.add_named_metadata_checked(
                &self.name,
                ReturnValueSingleStats {
                    seen: HashSet::new(),
                },
            )?;
        }
        Ok(())
    }
}

impl<EM, I, OT, S> libafl::feedbacks::Feedback<EM, I, OT, S> for ReturnValueSingleFeedback
where
    OT: libafl_bolts::tuples::MatchName,
    S: libafl::HasNamedMetadata,
{
    fn is_interesting(
        &mut self,
        state: &mut S,
        _manager: &mut EM,
        _input: &I,
        observers: &OT,
        _exit_kind: &ExitKind,
    ) -> Result<bool, Error> {
        // We'll modify in-memory cache and persist in append_metadata; short-lived borrows only
        for h in &self.observer_handles {
            if let Some(obs) = observers.get(h) {
                let obs = obs;
                let key = (h.name().to_string(), obs.last_value());
                let log_msg = format!("New return value single found: {:?}", key);

                // Check persisted State metadata first: if this single value is already
                // persisted, skip reporting it as new to keep behavior aligned.
                let named = state.named_metadata_map();
                if let Some(md) = named.get::<ReturnValueSingleStats>(&self.name) {
                    if md.seen.contains(&key) {
                        continue;
                    }
                }

                let inserted = self.seen.insert(key.clone());
                #[cfg(feature = "track_hit_feedbacks")]
                {
                    self.last = inserted;
                }
                if inserted {
                    return_value_debug_log("return_value_feedback", &log_msg);
                    return Ok(true);
                }
            } else {
                // observer missing: treat as non-interesting for this handle
                continue;
            }
        }
        Ok(false)
    }

    fn append_metadata(
        &mut self,
        state: &mut S,
        _manager: &mut EM,
        _observers: &OT,
        _testcase: &mut Testcase<I>,
    ) -> Result<(), Error> {
        // Persist the in-memory seen set into State metadata
        let stats_mut = state
            .named_metadata_map_mut()
            .get_mut::<ReturnValueSingleStats>(&self.name)
            .ok_or_else(|| {
                Error::illegal_argument("ReturnValueSingleFeedback: stats metadata missing")
            })?;
        // merge sets
        for v in self.seen.iter() {
            stats_mut.seen.insert(v.clone());
        }
        Ok(())
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

// removed duplicate impl: single-feedback implementation with state-aware methods is above

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
