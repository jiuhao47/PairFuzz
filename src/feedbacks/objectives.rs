use crate::utils::logging::debug;
use crate::{feedbacks::semantic::SemanticValue, observers::ReturnValueObserver};
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

/// Example metadata struct that will be stored in a testcase's metadata map.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExitInfo {
    pub note: std::string::String,
    /// optional return-value from harness A
    #[serde(with = "crate::feedbacks::semantic::legacy_option")]
    pub rv_a: Option<SemanticValue>,
    /// optional return-value from harness B
    #[serde(with = "crate::feedbacks::semantic::legacy_option")]
    pub rv_b: Option<SemanticValue>,
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

/// Crash-only objective: detects crashes/timeouts/diff-crashes and writes crash metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashObjective {
    name: Cow<'static, str>,
    last_crash: Option<CrashInfo>,
    crash_pending: bool,
}

impl CrashObjective {
    pub fn new() -> Self {
        let name = Cow::from("CrashObjective");
        Self {
            name,
            last_crash: None,
            crash_pending: false,
        }
    }
}

impl<S> StateInitializer<S> for CrashObjective {}

impl<EM, I, OT, S> Feedback<EM, I, OT, S> for CrashObjective
where
    OT: MatchName,
    I: libafl::inputs::Input,
{
    fn is_interesting(
        &mut self,
        _state: &mut S,
        _manager: &mut EM,
        _input: &I,
        _observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error> {
        self.crash_pending = false;
        self.last_crash = None;
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
            // Build a structured CrashInfo so metadata stores concise primary/secondary
            let label = format!("{:?}", exit_kind);
            let crash_info = match exit_kind {
                ExitKind::Diff { primary, secondary } => CrashInfo {
                    label: label.clone(),
                    primary: Some(format!("{:?}", primary)),
                    secondary: Some(format!("{:?}", secondary)),
                },
                _ => CrashInfo {
                    label: label.clone(),
                    primary: Some(label.clone()),
                    secondary: None,
                },
            };
            self.last_crash = Some(crash_info);
            self.crash_pending = true;
            debug(
                "CrashObjective",
                &format!(
                    "detected crash (self={:p}, crash_pending={})",
                    self, self.crash_pending
                ),
            );
            return Ok(true);
        }
        Ok(false)
    }

    fn append_metadata(
        &mut self,
        _state: &mut S,
        _manager: &mut EM,
        _observers: &OT,
        testcase: &mut libafl::corpus::Testcase<I>,
    ) -> Result<(), Error> {
        if !self.crash_pending {
            return Ok(());
        }
        // Use the structured CrashInfo we stored earlier
        let crash_info = self.last_crash.clone();
        let info = ExitInfo {
            note: String::from("crash_any"),
            rv_a: None,
            rv_b: None,
            crash: crash_info,
        };
        debug("CrashObjective", "attempting to insert crash_any metadata");
        // If an ExitInfo already exists, merge the crash info into it instead
        let md_map = testcase.metadata_map_mut();
        if let Some(existing) = md_map.get_mut::<ExitInfo>() {
            // preserve any existing rv values; overwrite/insert crash info
            existing.crash = info.crash;
            // prefer an existing note if present; otherwise set to crash_any
            if existing.note.is_empty() {
                existing.note = String::from("crash_any");
            }
            self.crash_pending = false;
            self.last_crash = None;
            debug(
                "CrashObjective",
                "merged crash_any metadata into existing ExitInfo",
            );
        } else {
            match md_map.try_insert_boxed(Box::new(info)) {
                Ok(_) => {
                    self.crash_pending = false;
                    self.last_crash = None;
                    debug("CrashObjective", "wrote crash_any metadata");
                }
                Err(e) => {
                    debug(
                        "CrashObjective",
                        &format!("crash_any metadata insert failed/skipped: {:?}", e),
                    );
                }
            }
        }
        Ok(())
    }

    fn last_result(&self) -> Result<bool, Error> {
        Ok(false)
    }
}

impl Named for CrashObjective {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

/// Return-value based objective. Two strategies supported:
/// - "zero_nonzero": treat pair (0,_non0_) or (_non0_,0) as diff
/// - "not_equal": any != is considered diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffObjective {
    observer_handles: Vec<Handle<ReturnValueObserver>>,
    name: Cow<'static, str>,
    mode: Cow<'static, str>,
    // return-value remembered state
    #[serde(with = "crate::feedbacks::semantic::legacy_option")]
    last_rv_a: Option<SemanticValue>,
    #[serde(with = "crate::feedbacks::semantic::legacy_option")]
    last_rv_b: Option<SemanticValue>,
    rv_pending: bool,
}

impl DiffObjective {
    pub fn new<M: Into<Cow<'static, str>>>(mode: M, observers: &[&ReturnValueObserver]) -> Self {
        let handles: Vec<Handle<ReturnValueObserver>> =
            observers.iter().map(|o| o.handle()).collect();
        let mode_cow: Cow<'static, str> = mode.into();
        let name = Cow::from(format!("DiffObjective({})", mode_cow));
        debug(
            "DiffObjective",
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
        }
    }
}

impl<S> StateInitializer<S> for DiffObjective {}

impl<EM, I, OT, S> Feedback<EM, I, OT, S> for DiffObjective
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
        _exit_kind: &ExitKind,
    ) -> Result<bool, Error> {
        self.rv_pending = false;
        self.last_rv_a = None;
        self.last_rv_b = None;
        if self.observer_handles.len() < 2 {
            return Ok(false);
        }

        for (i, h) in self.observer_handles.iter().enumerate() {
            if observers.get(h).is_none() {
                debug(
                    "DiffObjective",
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
            "zero_nonzero" => crate::feedbacks::semantic::ResultPolicy::ZeroNonzero.differs(va, vb),
            _ => va != vb,
        };

        if is {
            self.last_rv_a = va;
            self.last_rv_b = vb;
            self.rv_pending = true;
            debug(
                "DiffObjective",
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
        if !self.rv_pending {
            return Ok(());
        }
        let info = ExitInfo {
            note: format!("rv_{}", self.mode),
            rv_a: self.last_rv_a,
            rv_b: self.last_rv_b,
            crash: None,
        };
        debug("DiffObjective", "attempting to insert rv metadata");
        // Merge into existing ExitInfo if present to avoid "key exists" errors
        let md_map = testcase.metadata_map_mut();
        if let Some(existing) = md_map.get_mut::<ExitInfo>() {
            // Merge return-value fields. If existing values are None, fill them.
            if existing.rv_a.is_none() {
                existing.rv_a = self.last_rv_a;
            } else if self.last_rv_a.is_some() {
                existing.rv_a = self.last_rv_a; // prefer latest rv if present
            }
            if existing.rv_b.is_none() {
                existing.rv_b = self.last_rv_b;
            } else if self.last_rv_b.is_some() {
                existing.rv_b = self.last_rv_b;
            }
            // Merge note: if existing note already describes a crash, append rv note;
            // otherwise prefer the rv note.
            let rv_note = format!("rv_{}", self.mode);
            if existing.note.is_empty() {
                existing.note = rv_note;
            } else if !existing.note.starts_with("rv_") {
                if existing.note != rv_note {
                    existing.note = format!("{}+{}", existing.note, rv_note);
                }
            } else {
                existing.note = rv_note;
            }
            self.rv_pending = false;
            self.last_rv_a = None;
            self.last_rv_b = None;
            debug(
                "DiffObjective",
                &format!(
                    "merged rv metadata into existing ExitInfo: rv_{}",
                    self.mode
                ),
            );
        } else {
            match md_map.try_insert_boxed(Box::new(info)) {
                Ok(_) => {
                    self.rv_pending = false;
                    self.last_rv_a = None;
                    self.last_rv_b = None;
                    debug(
                        "DiffObjective",
                        &format!("wrote rv metadata: rv_{}", self.mode),
                    );
                }
                Err(e) => {
                    debug(
                        "DiffObjective",
                        &format!("rv metadata insert failed/skipped: {:?}", e),
                    );
                }
            }
        }
        Ok(())
    }

    fn last_result(&self) -> Result<bool, Error> {
        Ok(false)
    }
}

impl Named for DiffObjective {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

#[cfg(test)]
mod semantic_transport_tests {
    use super::ExitInfo;

    #[test]
    fn semantic_transport_deserializes_legacy_signed_results_losslessly() {
        let negative: ExitInfo = serde_json::from_value(serde_json::json!({
            "note": "legacy",
            "rv_a": -1,
            "rv_b": 0,
            "crash": null
        }))
        .unwrap();
        assert_eq!(negative.rv_a, Some(0xffff_ffff_u64));
        assert_eq!(negative.rv_b, Some(0_u64));

        let absent: ExitInfo = serde_json::from_value(serde_json::json!({
            "note": "legacy",
            "rv_a": null,
            "rv_b": 0,
            "crash": null
        }))
        .unwrap();
        assert_eq!(absent.rv_a, None);
        assert_eq!(absent.rv_b, Some(0_u64));
    }
}
