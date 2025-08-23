// Module root for `harness`.
// Implementation lives in `harness.rs` to keep `mod.rs` small and focused on exports.
pub mod harness;

pub use harness::{first_harness, second_harness};
