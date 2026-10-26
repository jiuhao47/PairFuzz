use clap::Parser;
use mimalloc::MiMalloc;
use std::fs;

mod driver;
mod feedbacks;
mod observers;
mod utils;

use crate::driver::args::Args;
use crate::utils::logging::{error, info, init_from_verbose, set_log_dir};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub fn main() {
    let args = Args::parse();
    init_from_verbose(args.verbose, args.log_dir.clone());
    driver::runtime::log_working_state(&args);
    unsafe { std::env::set_var("ASAN_OPTIONS", "abort_on_error=1:detect_leaks=false") }

    if let Some(log_dir) = &args.log_dir {
        if let Err(e) = fs::create_dir_all(log_dir) {
            error(
                "main",
                &format!("Failed to create log directory {:?}: {}", log_dir, e),
            );
            std::process::exit(1);
        };
        set_log_dir(Some(log_dir.clone()));
    } else if args.verbose > 0 {
        info("main", "Detailed output enabled");
    }

    if let Err(e) = driver::runtime::run(args) {
        error("main", &format!("Fatal error: {}", e));
        std::process::exit(1);
    }
}
