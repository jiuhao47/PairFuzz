use std::path::PathBuf;

use crate::driver::args::Args;
use crate::{
    feedbacks::differential_objective::DifferentialObjective, observers::ReturnValueObserver,
};
use libafl_bolts::Named;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClientIdentity {
    pub(crate) stable_index: u32,
    pub(crate) core_id: Option<usize>,
    pub(crate) rng_seed: u64,
    pub(crate) output_dir: PathBuf,
}

impl ClientIdentity {
    pub(crate) fn single_client(args: &Args) -> Self {
        Self {
            stable_index: 0,
            core_id: None,
            rng_seed: 0,
            output_dir: args.output_dir.clone(),
        }
    }
}

pub(crate) const fn fast_client_requested() -> bool {
    false
}

pub(crate) fn build_direct_objective<C>(
    map_a: &C,
    map_b: &C,
    rv_a: &ReturnValueObserver,
    rv_b: &ReturnValueObserver,
) -> DifferentialObjective<C>
where
    C: Named,
{
    crate::feedbacks::factory::build_fast_objective(map_a, map_b, rv_a, rv_b)
}
