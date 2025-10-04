use crate::utils::logging::{debug, info};
use libafl::Error;
use std::os::unix::fs::PermissionsExt;
use std::{fs::read, path::PathBuf};

const PERSIST_SIG: &str = "##SIG_AFL_PERSISTENT##\0";
const DEFER_SIG: &str = "##SIG_AFL_DEFER_FORKSRV##\0";
const ASAN_SYM_1: &str = "__asan_init";
const LSAN_SYM: &str = "__lsan_init";

/// Validation result describing detected target capabilities.
pub struct TargetCapabilities {
    pub persistent: bool,
    pub defer: bool,
    pub has_asan: bool,
    pub has_lsan: bool,
    pub instrumented: bool,
}

/// Validate a target binary path and return detected capabilities.
/// Mirrors important checks from executor::check_binary but structured here for reuse in the driver.
pub fn check_binary(
    path: &mut PathBuf,
    skip_instrument_check: bool,
    shmem_env_var: &str,
) -> Result<TargetCapabilities, Error> {
    info(
        "driver::bin_check",
        &format!("validating target binary '{}'", path.display()),
    );

    // Basic metadata checks
    let metadata = path.metadata()?;
    let is_reg = path.is_file();
    let bin_size = metadata.len();
    let is_executable = metadata.permissions().mode() & 0o111 != 0;
    debug(
        "driver::bin_check",
        &format!(
            "metadata: is_file={} executable={} size={}",
            is_reg, is_executable, bin_size
        ),
    );
    if !is_reg || !is_executable || bin_size < 4 {
        return Err(Error::illegal_argument(format!(
            "Program '{}' not found or not executable",
            path.display()
        )));
    }

    if skip_instrument_check {
        info(
            "driver::bin_check",
            "skipping instrumentation checks as requested",
        );
        // Treat as instrumented when skipping checks to preserve previous semantics
        return Ok(TargetCapabilities {
            persistent: false,
            defer: false,
            has_asan: false,
            has_lsan: false,
            instrumented: true,
        });
    }

    // Read the file content (small binary check) and look for instrumentation markers
    let data = read(path.as_path())?;
    // Simple heuristics: check for common instrumentation token
    let found = data
        .windows(shmem_env_var.len())
        .any(|w| w == shmem_env_var.as_bytes());
    debug(
        "driver::bin_check",
        &format!(
            "instrumentation token '{}' found = {}",
            shmem_env_var, found
        ),
    );
    // detect sanitizer symbols and persistent/defer markers
    let has_asan = data
        .windows(ASAN_SYM_1.len())
        .any(|w| w == ASAN_SYM_1.as_bytes());
    let has_lsan = data
        .windows(LSAN_SYM.len())
        .any(|w| w == LSAN_SYM.as_bytes());
    if has_asan || has_lsan {
        info(
            "driver::bin_check",
            &format!(
                "binary '{}' appears to include ASAN/LSAN symbols (asan={},lsan={})",
                path.display(),
                has_asan,
                has_lsan
            ),
        );
    }
    let persistent = data
        .windows(PERSIST_SIG.len())
        .any(|w| w == PERSIST_SIG.as_bytes());
    let defer = data
        .windows(DEFER_SIG.len())
        .any(|w| w == DEFER_SIG.as_bytes());
    if persistent {
        info(
            "driver::bin_check",
            &format!(
                "binary '{}' appears to be persistent (PERSIST_SIG present)",
                path.display()
            ),
        );
    }
    if defer {
        info(
            "driver::bin_check",
            &format!(
                "binary '{}' requests deferred forkserver (DEFER_SIG present)",
                path.display()
            ),
        );
    }

    if !found {
        return Err(Error::illegal_argument(
            "target binary is not instrumented correctly".to_string(),
        ));
    }

    info(
        "driver::bin_check",
        &format!(
            "binary '{}' validated (instrumented={})",
            path.display(),
            found
        ),
    );
    Ok(TargetCapabilities {
        persistent,
        defer,
        has_asan,
        has_lsan,
        instrumented: found,
    })
}
