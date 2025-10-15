use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Unified run mode for the fuzzer. Replaces separate --cov-mode/--rv-mode/--pl-mode.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum RunMode {
    SingleCov,
    SingleRv,
    SinglePl,
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
            RunMode::ComboCov => write!(f, "combo-cov"),
            RunMode::ComboRv => write!(f, "combo-rv"),
            RunMode::ComboPl => write!(f, "combo-pl"),
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
    #[arg(
        long = "corpus",
        help = "Primary corpus storage directory",
        default_value = "./corpus"
    )]
    pub corpus_dir: PathBuf,
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
    /// Enable forkserver persistent mode (default: enabled). If the target binary does not
    /// support persistent mode, the builder will automatically fall back.
    #[arg(
        long = "persistent",
        help = "Enable forkserver persistent mode (default: enabled).",
        default_value_t = true
    )]
    pub persistent: bool,
    /// Explicitly disable persistent mode (equivalent to `--persistent=false`). When both
    /// `--persistent` and `--no-persistent` are provided, `--no-persistent` takes precedence.
    #[arg(
        long = "no-persistent",
        help = "Explicitly disable persistent mode (equivalent to --persistent=false)."
    )]
    pub no_persistent: bool,
    /// Disable the simple stdout monitor. By default a SimpleMonitor that prints to stdout
    /// is included alongside the on-disk JSON monitor. Use this flag to suppress that stdout
    /// monitor (useful for background/CI runs).
    #[arg(
        long = "no-simple-monitor",
        help = "Disable the simple stdout monitor (SimpleMonitor)"
    )]
    pub no_simple_monitor: bool,
}

impl Args {
    /// Return the effective persistent setting. `--no-persistent` takes precedence if set.
    pub fn persistent_enabled(&self) -> bool {
        if self.no_persistent {
            false
        } else {
            self.persistent
        }
    }

    /// Return whether the simple stdout monitor should be enabled. The `--no-simple-monitor`
    /// flag takes precedence.
    pub fn simple_monitor_enabled(&self) -> bool {
        !self.no_simple_monitor
    }
}
