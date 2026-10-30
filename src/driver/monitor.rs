use libafl::monitors::{stats::ClientStatsManager, Monitor, OnDiskJsonMonitor, SimpleMonitor};
use libafl_bolts::{current_time, tuples::tuple_list};
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static LAST_JSON_WRITE: AtomicU64 = AtomicU64::new(0);
static JSON_HEARTBEAT_SECS: AtomicU64 = AtomicU64::new(15);

pub(crate) fn json_log_path(log_dir: Option<&Path>) -> PathBuf {
    log_dir
        .map(|dir| dir.join("fuzzer_stats.json"))
        .unwrap_or_else(|| PathBuf::from("fuzzer_stats.json"))
}

fn json_monitor_predicate(_client_stats_manager: &mut ClientStatsManager) -> bool {
    let now = current_time().as_secs();
    let last = LAST_JSON_WRITE.load(Ordering::Relaxed);
    let heartbeat = JSON_HEARTBEAT_SECS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < heartbeat {
        return false;
    }

    LAST_JSON_WRITE.store(now, Ordering::Relaxed);
    true
}

pub(crate) fn build_monitors(
    log_dir: Option<&Path>,
    heartbeat_secs: u64,
    simple_enabled: bool,
) -> impl Monitor + Debug {
    JSON_HEARTBEAT_SECS.store(heartbeat_secs, Ordering::Relaxed);

    let json_monitor = OnDiskJsonMonitor::new(
        json_log_path(log_dir),
        json_monitor_predicate as fn(&mut ClientStatsManager) -> bool,
    );
    let simple_monitor = SimpleMonitor::new(move |message| {
        if simple_enabled {
            println!("{message}");
        }
    });

    tuple_list!(json_monitor, simple_monitor)
}

#[cfg(test)]
mod tests {
    use super::json_log_path;
    use std::path::{Path, PathBuf};

    #[test]
    fn json_log_path_defaults_to_working_directory() {
        assert_eq!(json_log_path(None), PathBuf::from("fuzzer_stats.json"));
    }

    #[test]
    fn json_log_path_uses_log_directory() {
        assert_eq!(
            json_log_path(Some(Path::new("/tmp/pairfuzz-monitor-test"))),
            PathBuf::from("/tmp/pairfuzz-monitor-test/fuzzer_stats.json")
        );
    }
}
