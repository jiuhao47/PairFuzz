use crate::driver::{
    args::Args,
    client::run_legacy_client,
    fast_client::run_fast_client,
    pipeline::{self, ClientIdentity},
};

pub(crate) fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let identity = ClientIdentity::single_client(&args);
    if pipeline::fast_client_requested(&args) {
        run_fast_client(&args, identity)
    } else {
        run_legacy_client(&args)
    }
}

pub(crate) fn log_working_state(args: &Args) {
    crate::driver::client::log_working_state(args);
}
