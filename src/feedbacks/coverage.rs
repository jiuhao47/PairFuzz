//! Coverage feedback helpers.
//!
//! This file intentionally keeps the helpers minimal because `MaxMapFeedback` and
//! the various map observer types are highly generic across features. In `main.rs` we
//! create a concrete `coverage_pair` using two `MaxMapFeedback::new(...)` calls and
//! combine them with `feedback_or_fast!` so both branches have the same concrete type.
//! See `src/main.rs` for how `cov-mode` is handled.

use libafl::executors::ExitKind;
use libafl::feedbacks::Feedback;
use libafl::feedbacks::StateInitializer;
use libafl::observers::StdMapObserver;
use libafl::Error;
use libafl_bolts::Named;
use libafl_targets::DifferentialAFLMapSwapObserver;
use std::borrow::Cow;

/// Generic runtime wrapper for coverage feedback selection.
/// F is the concrete feedback type (for example a CombinedFeedback of two MaxMapFeedbacks).
#[allow(dead_code)]
pub enum CoverageFeedback<F> {
    None(Cow<'static, str>),
    Map(F),
}

/// Build a DifferentialAFLMapSwapObserver from two StdMapObserver mutable references.
/// The function is generic over the differential flags used by StdMapObserver.
pub fn make_map_swapper<'a, 'b, const D1: bool, const D2: bool>(
    first: &mut StdMapObserver<'a, u8, D1>,
    second: &mut StdMapObserver<'b, u8, D2>,
) -> DifferentialAFLMapSwapObserver<'a, 'b> {
    DifferentialAFLMapSwapObserver::new(first, second)
}

impl<F, S> StateInitializer<S> for CoverageFeedback<F>
where
    F: StateInitializer<S>,
{
    fn init_state(&mut self, state: &mut S) -> Result<(), Error> {
        match self {
            CoverageFeedback::None(_) => Ok(()),
            CoverageFeedback::Map(f) => f.init_state(state),
        }
    }
}

impl<F> Named for CoverageFeedback<F>
where
    F: Named,
{
    fn name(&self) -> &Cow<'static, str> {
        match self {
            CoverageFeedback::None(n) => n,
            CoverageFeedback::Map(f) => f.name(),
        }
    }
}

impl<EM, I, OT, S, F> Feedback<EM, I, OT, S> for CoverageFeedback<F>
where
    OT: libafl_bolts::tuples::MatchName,
    F: Feedback<EM, I, OT, S>,
{
    fn is_interesting(
        &mut self,
        state: &mut S,
        mgr: &mut EM,
        input: &I,
        observers: &OT,
        exit_kind: &ExitKind,
    ) -> Result<bool, Error> {
        match self {
            CoverageFeedback::None(_) => Ok(false),
            CoverageFeedback::Map(f) => f.is_interesting(state, mgr, input, observers, exit_kind),
        }
    }
}
