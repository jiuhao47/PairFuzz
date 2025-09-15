use crate::observers::ReturnValueObserver;
use crate::utils::logging::debug_log;
use libafl::feedbacks::{Feedback, StateInitializer};
use libafl::{
    executors::{DiffExitKind, ExitKind},
    Error, HasMetadata,
};
use libafl_bolts::tuples::{Handle, Handled, MatchName, MatchNameRef};
use libafl_bolts::Named;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
// removed disk-write imports; CrashAnyObjective will only annotate testcase metadata

// CrashAnyObjective removed: functionality merged into DiffRayObjective.
/// Example metadata struct that will be stored in a testcase's metadata map.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExitInfo {
    pub note: std::string::String,
    /// optional return-value from harness A
    pub rv_a: Option<i32>,
    /// optional return-value from harness B
    pub rv_b: Option<i32>,
    /// optional structured crash detail
    pub crash: Option<CrashInfo>,
}

/// Structured crash detail to ease automated analysis
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CrashInfo {
    /// human-friendly label, e.g. "primary_crash" or "secondary_crash" or raw
    pub label: String,
    /// primary executor exit kind (if applicable)
    pub primary: Option<String>,
    /// secondary executor exit kind (if applicable)
    pub secondary: Option<String>,
}

// Make it a SerdeAny so it can be stored in SerdeAnyMap
libafl_bolts::impl_serdeany!(ExitInfo);

/// Return-value based objective. Two strategies supported:
/// - "zero_nonzero": treat pair (0,_non0_) or (_non0_,0) as diff
/// - "not_equal": any != is considered diff
// ReturnValueObjective removed: functionality merged into DiffRayObjective.

/// Combined objective: prefer Crash detection, otherwise fall back to return-value diff.
/// This ensures that when a Crash occurs we short-circuit and do not attempt
/// the return-value checks which may rely on observers not present in the crash path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffRayObjective {
    observer_handles: Vec<Handle<ReturnValueObserver>>,
    name: Cow<'static, str>,
    mode: Cow<'static, str>,
    // return-value remembered state
    last_rv_a: Option<i32>,
    last_rv_b: Option<i32>,
    rv_pending: bool,
    // crash remembered state
    last_crash: Option<String>,
    crash_pending: bool,
}

impl DiffRayObjective {
    pub fn new<M: Into<Cow<'static, str>>>(mode: M, observers: &[&ReturnValueObserver]) -> Self {
        let handles: Vec<Handle<ReturnValueObserver>> =
            observers.iter().map(|o| o.handle()).collect();
        let mode_cow: Cow<'static, str> = mode.into();
        let name = Cow::from(format!("DiffRayObjective({})", mode_cow));
        let _ = debug_log(
            "DiffRayObjective",
            &format!(
                "constructed with rv handles: {}",
                handles
                    .iter()
                    .map(|h| h.name().to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        );
        Self {
            observer_handles: handles,
            name,
            mode: mode_cow,
            last_rv_a: None,
            last_rv_b: None,
            rv_pending: false,
            last_crash: None,
            crash_pending: false,
        }
    }
}

impl<S> StateInitializer<S> for DiffRayObjective {}

impl<EM, I, OT, S> Feedback<EM, I, OT, S> for DiffRayObjective
where
    OT: MatchName,
    I: libafl::inputs::Input,
{
    fn is_interesting(
        &mut self,
        _state: &mut S,
        _manager: &mut EM,
        _input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error> {
        // First: detect crashes/diff-crash and short-circuit
        // Treat Crash and Timeout (and diffs that contain them) as short-circuiting
        // events that should immediately be considered interesting and prevent
        // further return-value checks which may rely on observers not present.
        let is_crash = match exit_kind {
            ExitKind::Crash | ExitKind::Timeout => true,
            ExitKind::Diff { primary, secondary } => {
                matches!(primary, DiffExitKind::Crash)
                    || matches!(secondary, DiffExitKind::Crash)
                    || matches!(primary, DiffExitKind::Timeout)
                    || matches!(secondary, DiffExitKind::Timeout)
            }
            _ => false,
        };
        if is_crash {
            self.last_crash = Some(format!("{:?}", exit_kind));
            self.crash_pending = true;
            let _ = debug_log(
                "DiffRayObjective",
                &format!(
                    "detected crash (self={:p}, crash_pending={})",
                    self, self.crash_pending
                ),
            );
            return Ok(true);
        }

        // If no crash, fall back to return-value objective logic
        if self.observer_handles.len() < 2 {
            return Ok(false);
        }

        // Only log when an expected observer is missing to avoid noisy logs on
        // every execution where observers are present.
        for (i, h) in self.observer_handles.iter().enumerate() {
            if observers.get(h).is_none() {
                let _ = debug_log(
                    "DiffRayObjective",
                    &format!(
                        "missing observer handle[{}] name={} present=false",
                        i,
                        h.name()
                    ),
                );
            }
        }

        let a = observers
            .get(&self.observer_handles[0])
            .ok_or_else(|| Error::illegal_state("rv observer a missing"))?;
        let b = observers
            .get(&self.observer_handles[1])
            .ok_or_else(|| Error::illegal_state("rv observer b missing"))?;

        let va = a.last_value();
        let vb = b.last_value();

        let is = match self.mode.as_ref() {
            "zero_nonzero" => {
                (va == Some(0) && vb.map(|x| x != 0).unwrap_or(false))
                    || (vb == Some(0) && va.map(|x| x != 0).unwrap_or(false))
            }
            _ => va != vb,
        };

        if is {
            self.last_rv_a = va;
            self.last_rv_b = vb;
            self.rv_pending = true;
            let _ = debug_log(
                "DiffRayObjective",
                &format!(
                    "rv objective hit: {:?} vs {:?} (mode={})",
                    va, vb, self.mode
                ),
            );
        }
        Ok(is)
    }

    fn append_metadata(
        &mut self,
        _state: &mut S,
        _manager: &mut EM,
        _observers: &OT,
        testcase: &mut libafl::corpus::Testcase<I>,
    ) -> Result<(), Error> {
        // If crash_pending, write crash metadata and clear
        if self.crash_pending {
            let crash_info = self.last_crash.as_ref().map(|raw| {
                let primary = if raw.contains("primary") {
                    Some(raw.clone())
                } else {
                    None
                };
                let secondary = if raw.contains("secondary") {
                    Some(raw.clone())
                } else {
                    None
                };
                CrashInfo {
                    label: raw.clone(),
                    primary,
                    secondary,
                }
            });
            let info = ExitInfo {
                note: String::from("crash_any"),
                rv_a: None,
                rv_b: None,
                crash: crash_info,
            };
            let _ = debug_log(
                "DiffRayObjective",
                "attempting to insert crash_any metadata",
            );
            match testcase.metadata_map_mut().try_insert_boxed(Box::new(info)) {
                Ok(_) => {
                    self.crash_pending = false;
                    self.last_crash = None;
                    let _ = debug_log("DiffRayObjective", "wrote crash_any metadata");
                }
                Err(e) => {
                    let _ = debug_log(
                        "DiffRayObjective",
                        &format!("crash_any metadata insert failed/skipped: {:?}", e),
                    );
                }
            }
            return Ok(());
        }

        // Otherwise if rv_pending, write rv metadata
        if !self.rv_pending {
            return Ok(());
        }
        let info = ExitInfo {
            note: format!("rv_{}", self.mode),
            rv_a: self.last_rv_a,
            rv_b: self.last_rv_b,
            crash: None,
        };
        let _ = debug_log("DiffRayObjective", "attempting to insert rv metadata");
        match testcase.metadata_map_mut().try_insert_boxed(Box::new(info)) {
            Ok(_) => {
                self.rv_pending = false;
                self.last_rv_a = None;
                self.last_rv_b = None;
                let _ = debug_log(
                    "DiffRayObjective",
                    &format!("wrote rv metadata: rv_{}", self.mode),
                );
            }
            Err(e) => {
                let _ = debug_log(
                    "DiffRayObjective",
                    &format!("rv metadata insert failed/skipped: {:?}", e),
                );
            }
        }
        Ok(())
    }

    fn last_result(&self) -> Result<bool, Error> {
        Ok(false)
    }
}

impl Named for DiffRayObjective {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}
