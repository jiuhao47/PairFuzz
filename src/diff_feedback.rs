//! Lightweight abstraction for "diff" style feedbacks.
//! This module defines a marker trait `DiffFeedback` which can be implemented by feedbacks
//! that represent "differences" between two executions (e.g., return-value combos, exit kinds,
//! coverage diffs, ...). The trait is intentionally empty and exists for organization and
//! readability; it can be extended later with helper methods.

use libafl::executors::ExitKind;
use libafl::feedbacks::Feedback;
use libafl::Error;
use libafl_bolts::tuples::MatchName;

/// Marker trait for diff-style feedbacks.
pub trait DiffFeedback<EM, I, OT, S>: Feedback<EM, I, OT, S> {}

// Blanket impl helper: any Feedback can opt-in by adding a simple impl line like
// `impl<EM,I,OT,S> DiffFeedback<EM,I,OT,S> for ReturnValueComboFeedback {}` in their module.

// Keep this module minimal: it only provides the trait and documentation.
