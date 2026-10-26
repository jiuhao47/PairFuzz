use std::path::PathBuf;

use crate::driver::args::Args;

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
