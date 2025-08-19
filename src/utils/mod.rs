use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Append a debug line to `run/log/debug.log` with a timestamp and module name.
///
/// This function is safe to call from multiple places; it opens the file in append mode for each
/// call. It returns `Ok(())` on success and an `io::Error` on failure.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_debug_log() {
        let _ = std::fs::remove_file("run/log/debug.log");
        debug_log("testmod", "hello").expect("write");
        let s = std::fs::read_to_string("run/log/debug.log").unwrap();
        assert!(s.contains("testmod"));
        assert!(s.contains("hello"));
    }
}
