use crate::feedbacks::semantic::ResultPolicy;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Unified run mode for the fuzzer. Replaces separate --cov-mode/--rv-mode/--pl-mode.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum RunMode {
    SingleCov,
    SingleRv,
    SinglePl,
    SingleRvCov,
    /// Replay coverage for a directory of inputs (no fuzz loop)
    ReplayCov,
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
            RunMode::SingleRvCov => write!(f, "single-rv-cov"),
            RunMode::ReplayCov => write!(f, "replay-cov"),
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

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum, Debug)]
pub enum PipelineChoice {
    Legacy,
    Fast,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum, Debug)]
pub enum ScheduleChoice {
    Explore,
    Fast,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum, Debug)]
pub enum MutatorChoice {
    Havoc,
    #[value(name = "mopt")]
    MOpt,
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
    /// Execution pipeline. Legacy remains the default until benchmark selection is complete.
    #[arg(long, value_enum, default_value_t = PipelineChoice::Legacy)]
    pub pipeline: PipelineChoice,
    /// AFL-style weighted power schedule used by the fast pipeline.
    #[arg(long, value_enum, default_value_t = ScheduleChoice::Explore)]
    pub schedule: ScheduleChoice,
    /// Mutation scheduler used by the fast pipeline.
    #[arg(long, value_enum, default_value_t = MutatorChoice::Havoc)]
    pub mutator: MutatorChoice,
    /// Campaign seed used to derive a stable, client-specific RNG stream.
    #[arg(long)]
    pub campaign_seed: Option<u64>,
    /// Stable logical core identity used for deterministic per-client seed derivation.
    #[arg(long)]
    pub client_core_id: Option<u32>,
    /// AFL-compatible dictionary file. May be supplied more than once.
    #[arg(long = "dict", action = clap::ArgAction::Append)]
    pub dicts: Vec<PathBuf>,
    /// Interval between AFL-compatible statistics updates.
    #[arg(long, default_value_t = 15u64)]
    pub stats_interval_secs: u64,
    /// Semantic result comparison policy.
    #[arg(long, value_enum, default_value_t = ResultPolicy::ZeroNonzero)]
    pub result_policy: ResultPolicy,
    /// Behavioral signature format used by replay output.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=2), default_value_t = 2)]
    pub signature_version: u8,
    // legacy flags removed: use --persistent-mode and --simple-monitor instead
}

#[cfg(test)]
mod fast_pipeline_tests {
    use super::{Args, MutatorChoice, PipelineChoice, ScheduleChoice};
    use crate::feedbacks::semantic::ResultPolicy;
    use clap::Parser;

    #[test]
    fn pipeline_choices_parse() {
        let legacy = Args::try_parse_from(["diffray"]);
        assert_eq!(legacy.unwrap().pipeline, PipelineChoice::Legacy);
        let fast = Args::try_parse_from(["diffray", "--pipeline", "fast"]);
        assert_eq!(fast.unwrap().pipeline, PipelineChoice::Fast);
    }

    #[test]
    fn schedule_choices_parse() {
        let explore = Args::try_parse_from(["diffray"]);
        assert_eq!(explore.unwrap().schedule, ScheduleChoice::Explore);
        let fast = Args::try_parse_from(["diffray", "--schedule", "fast"]);
        assert_eq!(fast.unwrap().schedule, ScheduleChoice::Fast);
    }

    #[test]
    fn mutator_choices_parse() {
        let havoc = Args::try_parse_from(["diffray"]);
        assert_eq!(havoc.unwrap().mutator, MutatorChoice::Havoc);
        let mopt = Args::try_parse_from(["diffray", "--mutator", "mopt"]);
        assert_eq!(mopt.unwrap().mutator, MutatorChoice::MOpt);
    }

    #[test]
    fn client_core_identity_parses() {
        let args = Args::try_parse_from(["diffray", "--client-core-id", "31"]).unwrap();
        assert_eq!(args.client_core_id, Some(31));
    }

    #[test]
    fn semantic_policy_exact_rejects_legacy_fuzzing_before_startup() {
        let exact = Args::try_parse_from(["diffray", "--result-policy", "exact"]).unwrap();
        assert_eq!(exact.result_policy, ResultPolicy::Exact);
        assert!(exact.validate_semantic_options().is_err());

        let fast =
            Args::try_parse_from(["diffray", "--pipeline", "fast", "--result-policy", "exact"])
                .unwrap();
        assert!(fast.validate_semantic_options().is_ok());
    }
}

impl Args {
    pub fn validate_semantic_options(&self) -> Result<(), String> {
        if self.result_policy == ResultPolicy::Exact
            && self.pipeline == PipelineChoice::Legacy
            && self.mode != RunMode::ReplayCov
        {
            return Err(
                "--result-policy exact requires --pipeline fast or --mode replay-cov".into(),
            );
        }
        if self.signature_version == 1 && self.result_policy != ResultPolicy::ZeroNonzero {
            return Err("--signature-version 1 requires --result-policy zero-nonzero".into());
        }
        Ok(())
    }

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
