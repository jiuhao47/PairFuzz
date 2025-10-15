//! Collection of feedbacks for diffray: return-value and coverage variants
pub mod collect;
pub mod diffray_feedback;
pub mod hist_stats;
pub mod metadata;
pub mod objectives;
pub mod two_state_map;
pub use objectives::{CrashObjective, DiffObjective};
