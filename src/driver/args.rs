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

    // (已移除) 最大运行时间和最大迭代次数参数已删除 -- 不再由 driver 使用
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

    // (已移除) continue_on_crash 参数已删除 -- 若需保留该行为，请重新添加并实现逻辑
    /// 日志目录存储位置（将把日志文件写入该目录）
    #[arg(
        short = 'l',
        long = "log",
        help = "日志目录存储位置（将把日志文件写入该目录）"
    )]
    pub log_dir: Option<PathBuf>,

    /// 详细输出，重复使用 `-v` 可增加详细级别（-v, -vv, -vvv）
    #[arg(short = 'v', long = "verbose", help = "启用详细输出，重复使用增加详细级别", action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// 单次执行超时时间（毫秒）
    #[arg(
        long = "timeout",
        help = "单次执行超时时间（毫秒）",
        default_value = "5000"
    )]
    pub timeout_ms: u64,
    /// coverage 模式: none|single|combo
    #[arg(long = "cov-mode", help = "coverage 模式: none|single|combo", default_value_t = ModeOption::Single)]
    pub cov_mode: ModeOption,
    /// return-value 模式: none|single|combo
    #[arg(long = "rv-mode", help = "return-value 模式: none|single|combo", default_value_t = ModeOption::None)]
    pub rv_mode: ModeOption,
}
