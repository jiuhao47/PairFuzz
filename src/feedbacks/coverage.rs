//! Coverage feedback helpers.
//!
//! This file intentionally keeps the helpers minimal because `MaxMapFeedback` and
//! the various map observer types are highly generic across features. In `main.rs` we
//! create a concrete `coverage_pair` using two `MaxMapFeedback::new(...)` calls and
//! combine them with `feedback_or_fast!` so both branches have the same concrete type.
//! See `src/main.rs` for how `cov-mode` is handled.
use libafl::observers::StdMapObserver;
use libafl_targets::DifferentialAFLMapSwapObserver;

/// Build a DifferentialAFLMapSwapObserver from two StdMapObserver mutable references.
/// The function is generic over the differential flags used by StdMapObserver.
pub fn make_map_swapper<'a, 'b, const D1: bool, const D2: bool>(
    first: &mut StdMapObserver<'a, u8, D1>,
    second: &mut StdMapObserver<'b, u8, D2>,
) -> DifferentialAFLMapSwapObserver<'a, 'b> {
    DifferentialAFLMapSwapObserver::new(first, second)
}
