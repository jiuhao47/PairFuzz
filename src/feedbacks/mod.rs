//! Collection of feedbacks for pairfuzz: return-value and coverage variants
pub mod collect;
pub mod differential_objective;
pub mod factory;
pub mod hist_stats;
pub mod metadata;
pub mod objectives;
pub mod pairfuzz_feedback;
pub mod semantic;
pub mod signature;
pub use objectives::{CrashObjective, DiffObjective};
