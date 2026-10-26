use crate::{
    feedbacks::diffray_feedback::DiffrayFeedback,
    utils::logging::{error, info, warn},
};
use libafl::{
    events::{AwaitRestartSafe, SendExiting},
    executors::HasObservers,
    inputs::BytesInput,
    observers::MapObserver,
    ExecutesInput,
};
use libafl_bolts::{tuples::MatchNameRef, AsIter};
use serde_json::{json, Map, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn timestamp_seconds(time: SystemTime) -> f64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

pub(crate) fn collect_replay_files(input_dir: &Path) -> std::io::Result<Vec<(PathBuf, f64)>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(input_dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let time = metadata
            .modified()
            .or_else(|_| metadata.created())
            .unwrap_or_else(|_| SystemTime::now());
        files.push((path, timestamp_seconds(time)));
    }
    Ok(files)
}

pub(crate) fn sort_replay_files(files: &mut [(PathBuf, f64)]) {
    files.sort_by(
        |a, b| match a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal) {
            std::cmp::Ordering::Equal => a.0.cmp(&b.0),
            ordering => ordering,
        },
    );
}

pub(crate) fn replay_record(
    index: usize,
    relative_seconds: f64,
    user_stats: Map<String, Value>,
) -> Value {
    json!({
        "client_stats": {"0": {"corpus_size": index + 1, "enabled": true, "executions": index + 1, "user_stats": user_stats}},
        "clients": 1,
        "corpus": index + 1,
        "executions": index + 1,
        "run_time": {"secs": relative_seconds}
    })
}

#[expect(clippy::too_many_arguments)]
pub(crate) fn run_replay_cov<C, EM, EX, F, S>(
    input_dir: Option<&Path>,
    log_dir: Option<&Path>,
    num_edges: usize,
    feedback: &DiffrayFeedback<C>,
    state: &mut S,
    fuzzer: &mut F,
    executor: &mut EX,
    manager: &mut EM,
) -> Result<(), Box<dyn std::error::Error>>
where
    C: MapObserver + for<'it> AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    EM: AwaitRestartSafe + SendExiting,
    EX: HasObservers,
    EX::Observers: MatchNameRef,
    F: ExecutesInput<EX, EM, BytesInput, S>,
{
    let Some(input_dir) = input_dir else {
        error("main", "ReplayCov mode requires --input <dir>");
        std::process::exit(1);
    };
    if !input_dir.exists() || !input_dir.is_dir() {
        error(
            "main",
            &format!(
                "Replay mode: specified input path is not a directory: {:?}",
                input_dir
            ),
        );
        std::process::exit(1);
    }

    let mut files =
        collect_replay_files(input_dir).unwrap_or_else(|err| panic!("read_dir failed: {err}"));
    if files.is_empty() {
        info("main", "ReplayCov: no input files found in directory");
        return Ok(());
    }

    sort_replay_files(&mut files);
    let initial_time = files.first().map(|file| file.1).unwrap_or(0.0);
    let json_log_path = crate::driver::monitor::json_log_path(log_dir);
    if let Some(parent) = json_log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut union_maps = Vec::new();
    for handle in &feedback.map_refs {
        if let Some(observer) = executor.observers().get(handle) {
            let initial: u8 = observer.initial().into();
            union_maps.push(vec![initial; num_edges]);
        } else {
            union_maps.push(vec![0u8; num_edges]);
        }
    }

    let mut seen = 0usize;
    for (index, (path, timestamp)) in files.into_iter().enumerate() {
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                warn(
                    "main",
                    &format!("Skipping unreadable file {:?}: {}", path, err),
                );
                continue;
            }
        };
        let input = BytesInput::from(bytes.as_slice());
        match fuzzer.execute_input(state, executor, manager, &input) {
            Ok(_) => {
                let mut user_stats = Map::new();
                let num_edges = num_edges as u64;
                for (map_index, handle) in feedback.map_refs.iter().enumerate() {
                    if let Some(observer) = executor.observers().get(handle) {
                        let initial: u8 = observer.initial().into();
                        for (index, item) in observer.as_iter().enumerate() {
                            let current: u8 = (*item).into();
                            union_maps[map_index][index] =
                                union_maps[map_index][index].max(current);
                        }

                        let union_count = union_maps[map_index]
                            .iter()
                            .filter(|&&value| value != initial)
                            .count() as u64;
                        let name = feedback
                            .names
                            .get(map_index)
                            .map(ToString::to_string)
                            .unwrap_or_else(|| format!("map-{map_index}"));
                        let ratio = json!({
                            "aggregator_op": "Max",
                            "value": {"Ratio": [union_count, num_edges]}
                        });
                        user_stats.insert(name, ratio);
                    }
                }

                let record = replay_record(index, timestamp - initial_time, user_stats);
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&json_log_path)
                    .unwrap_or_else(|err| panic!("failed open json log: {err}"));
                writeln!(file, "{record}").ok();
                seen += 1;
            }
            Err(err) => warn("main", &format!("Execution error for {:?}: {}", path, err)),
        }
    }

    info("main", &format!("ReplayCov: processed {seen} inputs"));
    if let Err(err) = manager.on_shutdown() {
        warn("main", &format!("Error during manager shutdown: {err}"));
    }
    manager.await_restart_safe();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{collect_replay_files, replay_record, sort_replay_files, timestamp_seconds};
    use serde_json::{json, Map};
    use std::{
        fs,
        path::PathBuf,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn timestamp_before_epoch_is_zero() {
        assert_eq!(timestamp_seconds(UNIX_EPOCH - Duration::from_secs(1)), 0.0);
    }

    #[test]
    fn sort_replay_files_orders_timestamp_then_path() {
        let mut files = vec![
            (PathBuf::from("z"), 1.0),
            (PathBuf::from("b"), 0.5),
            (PathBuf::from("a"), 1.0),
        ];

        sort_replay_files(&mut files);

        assert_eq!(files[0], (PathBuf::from("b"), 0.5));
        assert_eq!(files[1], (PathBuf::from("a"), 1.0));
        assert_eq!(files[2], (PathBuf::from("z"), 1.0));
    }

    #[test]
    fn replay_record_matches_existing_schema() {
        let mut user_stats = Map::new();
        user_stats.insert("first-edges".to_owned(), json!({"value": 3}));

        let record = replay_record(2, 1.25, user_stats.clone());

        assert_eq!(record["clients"], 1);
        assert_eq!(record["corpus"], 3);
        assert_eq!(record["executions"], 3);
        assert_eq!(record["run_time"]["secs"], 1.25);
        assert_eq!(record["client_stats"]["0"]["corpus_size"], 3);
        assert_eq!(
            record["client_stats"]["0"]["user_stats"],
            serde_json::Value::Object(user_stats)
        );
    }

    #[test]
    fn collect_replay_files_is_non_recursive() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "diffray-replay-test-{}-{unique}",
            std::process::id()
        ));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("direct"), b"direct").unwrap();
        fs::write(nested.join("ignored"), b"nested").unwrap();

        let result = collect_replay_files(&root).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, root.join("direct"));
        fs::remove_dir_all(&root).unwrap();
    }
}
