use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Append a human-readable debug line to `run/log/debug.log` (or provided `path`) with a timestamp and module name.
pub fn debug_log(module: &str, message: &str) -> io::Result<()> {
    debug_log_opt(None, module, message)
}

/// Write debug message to an optional path. If `path_opt` is `None` the default `run/log/debug.log` is used.
pub fn debug_log_opt(path_opt: Option<&Path>, module: &str, message: &str) -> io::Result<()> {
    let path_buf: PathBuf = match path_opt {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from("run/log/debug.log"),
    };

    if let Some(parent) = path_buf.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path_buf)?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    // Format: [secs.millis] [module] message
    writeln!(file, "{:.3} [{}] {}", ts, module, message)?;
    Ok(())
}

/// Initialize the global logger based on an optional verbose count or an explicit RUST_LOG string.
///
/// Behavior:
/// - If the environment variable `RUST_LOG` is already set, this function does nothing.
/// - Otherwise, if `level_override` is Some(s), it will set `RUST_LOG` to that string.
/// - Otherwise, it will map `verbose` (u8) to a default level: 0 -> info, 1 -> debug, >=2 -> trace.
/// After ensuring `RUST_LOG` is set, it calls `env_logger::init()` to initialize the logger.
pub fn init_from_verbose(verbose: u8, level_override: Option<&str>) {
    if std::env::var("RUST_LOG").is_err() {
        if let Some(lvl) = level_override {
            std::env::set_var("RUST_LOG", lvl);
        } else {
            // Map verbose count to default RUST_LOG level:
            // - 0 or 1 => info
            // - 2 => debug
            // - 3 or more => trace
            let level = match verbose {
                0 | 1 => "info",
                2 => "debug",
                _ => "trace",
            };
            std::env::set_var("RUST_LOG", level);
        }
    }
    // Only initialize env_logger when verbose level is at least 1.
    // This avoids initializing the global logger when the user hasn't requested
    // additional verbosity. Respect any pre-existing RUST_LOG value above.
    if verbose > 1 {
        let _ = env_logger::try_init();
    }
}
