use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Run-time coverage/rv modes used by the driver
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum ModeOption {
    None,
    Single,
    Combo,
    MaxMin,
}

impl std::fmt::Display for ModeOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModeOption::None => write!(f, "none"),
            ModeOption::Single => write!(f, "single"),
            ModeOption::Combo => write!(f, "combo"),
            ModeOption::MaxMin => write!(f, "maxmin"),
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "diffray")]
#[command(about = "一个差分模糊测试器，用于发现两个程序实现之间的差异")]
pub struct Args {
    /// 初始种子输入文件夹
    #[arg(short = 'i', long = "input", help = "初始种子输入文件夹路径")]
    pub input_dir: Option<PathBuf>,

    /// 最长模糊测试时间（秒）
    #[arg(
        short = 't',
        long = "max_time",
        help = "最长模糊测试时间（秒）",
        conflicts_with = "max_iterations"
    )]
    pub max_time: Option<u64>,

    /// 最大执行次数
    #[arg(
        short = 'n',
        long = "max-iterations",
        help = "最大执行次数",
        conflicts_with = "max_time"
    )]
    pub max_iterations: Option<u64>,

    /// 输出差分输入的文件夹
    #[arg(
        short = 'o',
        long = "output",
        help = "输出差分输入的文件夹路径",
        default_value = "./diffs"
    )]
    pub output_dir: PathBuf,
    /// 主 corpus 存储位置（默认会持久化到磁盘）
    #[arg(
        long = "corpus",
        help = "主 corpus 存储目录",
        default_value = "./corpus"
    )]
    pub corpus_dir: PathBuf,

    /// 发现差分输入时不停止模糊测试器
    #[arg(
        short = 'c',
        long = "continue",
        help = "发现差分输入时不停止模糊测试器"
    )]
    pub continue_on_crash: bool,

    /// 日志目录存储位置（将把日志文件写入该目录）
    #[arg(
        short = 'l',
        long = "log",
        help = "日志目录存储位置（将把日志文件写入该目录）"
    )]
    pub log_dir: Option<PathBuf>,

    /// 详细输出
    #[arg(short = 'v', long = "verbose", help = "启用详细输出")]
    pub verbose: bool,

    /// 单次执行超时时间（毫秒）
    #[arg(
        long = "timeout",
        help = "单次执行超时时间（毫秒）",
        default_value = "5000"
    )]
    pub timeout_ms: u64,
    /// coverage 模式: none|single|combo
    #[arg(long = "cov-mode", help = "coverage 模式: none|single|combo", default_value_t = ModeOption::Combo)]
    pub cov_mode: ModeOption,
    /// return-value 模式: none|single|combo
    #[arg(long = "rv-mode", help = "return-value 模式: none|single|combo", default_value_t = ModeOption::Combo)]
    pub rv_mode: ModeOption,
}
