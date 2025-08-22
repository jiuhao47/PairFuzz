use serde::Serialize;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct CorpusLog<'a> {
    ts: f64,
    module: &'a str,
    len: usize,
    exit_kind: &'a str,
    hex_prefix: &'a str,
}

/// Write a single JSON line to `run/log/debug.log` containing metadata and a short hex prefix of the seed.
pub fn log_corpus_line(
    module: &str,
    len: usize,
    exit_kind: &str,
    hex_prefix: &str,
) -> io::Result<()> {
    let path = Path::new("run/log/debug.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let rec = CorpusLog {
        ts,
        module,
        len,
        exit_kind,
        hex_prefix,
    };
    let s = serde_json::to_string(&rec).unwrap_or_else(|_| String::from("{}"));
    writeln!(file, "{}", s)?;
    Ok(())
}

/// Append a human-readable debug line to `run/log/debug.log` with a timestamp and module name.
pub fn debug_log(module: &str, message: &str) -> io::Result<()> {
    let path = Path::new("run/log/debug.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    // Format: [secs.millis] [module] message
    writeln!(file, "{:.3} [{}] {}", ts, module, message)?;
    Ok(())
}
