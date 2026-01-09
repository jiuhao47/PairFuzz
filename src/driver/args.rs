use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Unified run mode for the fuzzer. Replaces separate --cov-mode/--rv-mode/--pl-mode.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum RunMode {
    SingleCov,
    SingleRv,
    SinglePl,
    SingleRvPath,
    ComboCov,
    ComboRv,
    ComboPl,
}

impl std::fmt::Display for RunMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunMode::SingleCov => write!(f, "single-cov"),
            RunMode::SingleRv => write!(f, "single-rv"),
            RunMode::SinglePl => write!(f, "single-pl"),
            RunMode::SingleRvPath => write!(f, "single-rv-path"),
            RunMode::ComboCov => write!(f, "combo-cov"),
            RunMode::ComboRv => write!(f, "combo-rv"),
            RunMode::ComboPl => write!(f, "combo-pl"),
        }
    }
}

// Persistent mode enum (top-level, like RunMode)
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum PersistentMode {
    Auto,
    Enabled,
    Disabled,
}

impl std::fmt::Display for PersistentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistentMode::Auto => write!(f, "auto"),
            PersistentMode::Enabled => write!(f, "enabled"),
            PersistentMode::Disabled => write!(f, "disabled"),
        }
    }
}

// Simple monitor enum (top-level)
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum SimpleMonitorMode {
    Auto,
    Enabled,
    Disabled,
}

impl std::fmt::Display for SimpleMonitorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimpleMonitorMode::Auto => write!(f, "auto"),
            SimpleMonitorMode::Enabled => write!(f, "enabled"),
            SimpleMonitorMode::Disabled => write!(f, "disabled"),
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "diffray")]
#[command(
    about = "A differential fuzzer for finding behavioral differences between two implementations"
)]
pub struct Args {
    /// Initial seed input directory.
    #[arg(
        short = 'i',
        long = "input",
        help = "Initial seed input directory path"
    )]
    pub input_dir: Option<PathBuf>,
    /// Output directory for differential inputs.
    #[arg(
        short = 'o',
        long = "output",
        help = "Output directory for differential inputs",
        default_value = "./diffs"
    )]
    pub output_dir: PathBuf,
    /// Primary corpus storage directory (persisted to disk by default).
    // NOTE: `--corpus` flag removed per request. If a persistent corpus path
    // is required elsewhere, use the configured output or default locations.
    /// Directory to write log files into (if specified).
    #[arg(
        short = 'l',
        long = "log",
        help = "Directory to write log files into (if specified)."
    )]
    pub log_dir: Option<PathBuf>,

    /// Increase verbosity. Repeat `-v` to increase level (e.g. -v, -vv, -vvv).
    #[arg(short = 'v', long = "verbose", help = "Increase verbosity. Repeat to increase level.", action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Single-execution timeout in milliseconds.
    #[arg(
        long = "timeout",
        help = "Single-execution timeout in milliseconds.",
        default_value = "5000"
    )]
    pub timeout_ms: u64,
    /// JSON monitor heartbeat interval in seconds. Defaults to 15s.
    #[arg(
        long = "json-heartbeat-secs",
        help = "JSON monitor heartbeat interval in seconds",
        default_value_t = 15u64
    )]
    pub json_heartbeat_secs: u64,
    /// Path to the first implementation executable (used in forkserver mode).
    #[arg(
        long = "prog-a",
        help = "Path to the first implementation executable (used in forkserver mode)."
    )]
    pub prog_a: Option<PathBuf>,
    /// Path to the second implementation executable (used in forkserver mode).
    #[arg(
        long = "prog-b",
        help = "Path to the second implementation executable (used in forkserver mode)."
    )]
    pub prog_b: Option<PathBuf>,
    /// Run mode: single-cov|single-rv|single-pl|combo-cov|combo-rv|combo-pl
    #[arg(long = "mode", help = "Run mode: single-cov|single-rv|single-pl|combo-cov|combo-rv|combo-pl", default_value_t = RunMode::SingleCov)]
    pub mode: RunMode,

    #[arg(
        long = "persistent-mode",
        help = "Persistent mode: auto|enabled|disabled (auto preserves previous default)",
        value_enum,
        default_value_t = PersistentMode::Auto
    )]
    pub persistent_mode: PersistentMode,

    /// (legacy flags removed) Use `--persistent-mode` instead.
    #[arg(long = "simple-monitor", help = "Simple monitor: auto|enabled|disabled", value_enum, default_value_t = SimpleMonitorMode::Auto)]
    pub simple_monitor_mode: SimpleMonitorMode,
    // legacy flags removed: use --persistent-mode and --simple-monitor instead
}

impl Args {
    /// Return the effective persistent setting. `--no-persistent` takes precedence if set.
    pub fn persistent_enabled(&self) -> bool {
        // Consult the enum: Auto -> default true, Enabled -> true, Disabled -> false
        match self.persistent_mode {
            PersistentMode::Auto => true, // historical default was enabled
            PersistentMode::Enabled => true,
            PersistentMode::Disabled => false,
        }
    }

    /// Return whether the simple stdout monitor should be enabled. The `--no-simple-monitor`
    /// flag takes precedence.
    pub fn simple_monitor_enabled(&self) -> bool {
        match self.simple_monitor_mode {
            SimpleMonitorMode::Auto => true, // default: enabled
            SimpleMonitorMode::Enabled => true,
            SimpleMonitorMode::Disabled => false,
        }
    }
}
