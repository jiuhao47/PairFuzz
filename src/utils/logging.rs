use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Log level for diffray logger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        }
    }
}

static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static GLOBAL_LEVEL: OnceLock<Mutex<LogLevel>> = OnceLock::new();
static LOG_DIR: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

fn get_log_file_mutex() -> &'static Mutex<Option<File>> {
    LOG_FILE.get_or_init(|| Mutex::new(None))
}

fn get_global_level_mutex() -> &'static Mutex<LogLevel> {
    GLOBAL_LEVEL.get_or_init(|| Mutex::new(LogLevel::Info))
}

fn open_log_file() -> io::Result<File> {
    // Determine path: if LOG_DIR is set, use that directory, else repo root
    let dir_opt = LOG_DIR
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap()
        .clone();
    let path = match dir_opt {
        Some(p) => p.join("diffray.log"),
        None => PathBuf::from("diffray.log"),
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// Set the directory where diffray.log will be written. Pass `None` to use repo root.
pub fn set_log_dir(dir: Option<PathBuf>) {
    let mutex = LOG_DIR.get_or_init(|| Mutex::new(None));
    let mut guard = mutex.lock().unwrap();
    *guard = dir;
    // Reset LOG_FILE so next write opens the file at new location
    if let Some(mf) = LOG_FILE.get() {
        let mut fguard = mf.lock().unwrap();
        *fguard = None;
    }
}

/// Set global minimum log level.
pub fn set_level(level: LogLevel) {
    let mut g = get_global_level_mutex().lock().unwrap();
    *g = level;
}

/// Get global minimum log level.
pub fn get_level() -> LogLevel {
    *get_global_level_mutex().lock().unwrap()
}

/// Internal: format timestamp as seconds.millis since UNIX_EPOCH
fn now_ts() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Log a message at the given level with a module tag. This synchronously writes
/// to stderr/stdout and appends to `diffray.log` in the repository root.
pub fn log(level: LogLevel, module: &str, msg: &str) {
    // Fast path: check level
    if level > get_level() {
        return;
    }

    let ts = now_ts();
    let line = format!("{:.3} [{}] {}: {}\n", ts, level.as_str(), module, msg);

    // Print to terminal: errors/warn -> stderr, others -> stdout
    if level <= LogLevel::Warn {
        let _ = std::io::stderr().write_all(line.as_bytes());
    } else {
        let _ = std::io::stdout().write_all(line.as_bytes());
    }

    // Append to file (synchronously)
    let guard_mutex = get_log_file_mutex();
    let mut guard = guard_mutex.lock().unwrap();
    if guard.is_none() {
        if let Ok(f) = open_log_file() {
            *guard = Some(f);
        }
    }
    if let Some(f) = guard.as_mut() {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
}

/// Convenience macros / functions
pub fn error(module: &str, msg: &str) {
    log(LogLevel::Error, module, msg)
}
pub fn warn(module: &str, msg: &str) {
    log(LogLevel::Warn, module, msg)
}
pub fn info(module: &str, msg: &str) {
    log(LogLevel::Info, module, msg)
}
pub fn debug(module: &str, msg: &str) {
    log(LogLevel::Debug, module, msg)
}
#[allow(dead_code)]
pub fn trace(module: &str, msg: &str) {
    log(LogLevel::Trace, module, msg)
}

/// Initialize logger level from an environment variable like DIFFRAY_LOG (optional),
/// or leave default INFO. Returns the chosen level.
pub fn init_from_env() -> LogLevel {
    if let Ok(s) = std::env::var("DIFFRAY_LOG") {
        let s_low = s.to_lowercase();
        let lvl = match s_low.as_str() {
            "error" => LogLevel::Error,
            "warn" | "warning" => LogLevel::Warn,
            "info" => LogLevel::Info,
            "debug" => LogLevel::Debug,
            "trace" => LogLevel::Trace,
            _ => LogLevel::Info,
        };
        set_level(lvl);
        lvl
    } else {
        get_level()
    }
}

/// Initialize logger from `-v` style verbosity count and optional log directory.
///
/// Verbosity mapping:
/// - verbose == 0: use DIFFRAY_LOG env var if present, otherwise keep default (Info)
/// - verbose == 1: Info
/// - verbose == 2: Debug
/// - verbose >=3: Trace
///
/// If `dir` is Some(path) the log directory will be set via `set_log_dir`.
pub fn init_from_verbose(verbose: u8, dir: Option<PathBuf>) -> LogLevel {
    if let Some(d) = dir {
        set_log_dir(Some(d));
    }

    let lvl = match verbose {
        0 => {
            // honor DIFFRAY_LOG if present, otherwise keep current
            init_from_env()
        }
        1 => LogLevel::Info,
        2 => LogLevel::Debug,
        _ => LogLevel::Trace,
    };
    set_level(lvl);
    lvl
}
