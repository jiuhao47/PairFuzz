//! Collection of feedbacks for diffray: return-value and coverage variants
pub mod collect;
pub mod diffray_feedback;
pub mod factory;
pub mod hist_stats;
pub mod metadata;
pub mod objectives;
pub use objectives::{CrashObjective, DiffObjective};
