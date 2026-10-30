use crate::{
    feedbacks::pairfuzz_feedback::PairFuzzFeedback,
    feedbacks::semantic::{ResultPolicy, SemanticValue},
    feedbacks::signature::{
        compute_signature_v1, compute_signature_v2, DifferentialSignature, ExitClass,
    },
    observers::ReturnValueObserver,
    utils::logging::{error, info, warn},
};
use libafl::{
    events::{AwaitRestartSafe, SendExiting},
    executors::{DiffExitKind, ExitKind, HasObservers},
    inputs::BytesInput,
    observers::MapObserver,
    ExecutesInput,
};
use libafl_bolts::{
    tuples::{Handle, MatchNameRef},
    AsIter,
};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::{
    borrow::Cow,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

const SIGNATURE_REPETITIONS: usize = 3;

#[derive(Debug, Serialize)]
struct SignatureReplayRecord {
    path: String,
    signatures: Vec<String>,
    return_pairs: Vec<[Option<SemanticValue>; 2]>,
    stable: bool,
    differential: bool,
    discovery_elapsed_nanos: Option<u64>,
}

pub(crate) fn collect_replay_files(input_dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(input_dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !path.is_file() || name.starts_with('.') || name.ends_with(".metadata") {
            continue;
        }
        files.push(path);
    }
    Ok(files)
}

pub(crate) fn sort_replay_files(files: &mut [PathBuf]) {
    files.sort();
}

fn exit_class(kind: DiffExitKind) -> ExitClass {
    match kind {
        DiffExitKind::Ok => ExitClass::Ok,
        DiffExitKind::Crash => ExitClass::Crash,
        DiffExitKind::Timeout => ExitClass::Timeout,
        DiffExitKind::Oom => ExitClass::Oom,
        DiffExitKind::Diff => ExitClass::Other,
    }
}

fn exit_class_pair(kind: ExitKind) -> (ExitClass, ExitClass) {
    match kind {
        ExitKind::Diff { primary, secondary } => (exit_class(primary), exit_class(secondary)),
        ExitKind::Ok => (ExitClass::Ok, ExitClass::Ok),
        ExitKind::Crash => (ExitClass::Crash, ExitClass::Crash),
        ExitKind::Timeout => (ExitClass::Timeout, ExitClass::Timeout),
        ExitKind::Oom => (ExitClass::Oom, ExitClass::Oom),
    }
}

fn semantic_difference(
    policy: ResultPolicy,
    exit_a: ExitClass,
    exit_b: ExitClass,
    rv_a: Option<SemanticValue>,
    rv_b: Option<SemanticValue>,
) -> bool {
    exit_a != exit_b || policy.differs(rv_a, rv_b)
}

fn legacy_result(value: Option<SemanticValue>) -> Result<Option<i32>, libafl::Error> {
    value
        .map(|value| {
            u32::try_from(value)
                .map(|legacy| legacy as i32)
                .map_err(|_| {
                    libafl::Error::illegal_argument(
                        "signature v1 cannot encode a semantic value above u32::MAX",
                    )
                })
        })
        .transpose()
}

fn persisted_elapsed_nanos(path: &Path) -> Option<u64> {
    let direct = path.with_file_name(format!("{}.metadata", path.file_name()?.to_string_lossy()));
    let hidden = path.with_file_name(format!(".{}.metadata", path.file_name()?.to_string_lossy()));
    let collision_prefix = format!(".{}_", path.file_name()?.to_string_lossy());
    let mut candidates = vec![direct, hidden];
    if let Some(parent) = path.parent() {
        if let Ok(entries) = fs::read_dir(parent) {
            candidates.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|candidate| {
                        candidate
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| {
                                name.starts_with(&collision_prefix) && name.ends_with(".metadata")
                            })
                    }),
            );
        }
    }

    fn find_nanos(value: &Value) -> Option<u64> {
        match value {
            Value::Object(object) => object
                .get("nanos")
                .and_then(Value::as_u64)
                .or_else(|| object.values().find_map(find_nanos)),
            Value::Array(values) => values.iter().find_map(find_nanos),
            _ => None,
        }
    }

    candidates.into_iter().find_map(|candidate| {
        let payload: Value = serde_json::from_slice(&fs::read(candidate).ok()?).ok()?;
        find_nanos(&payload)
    })
}

fn signature_observation<C, EX>(
    feedback: &PairFuzzFeedback<C>,
    executor: &EX,
    exit_kind: ExitKind,
    policy: ResultPolicy,
    signature_version: u8,
) -> Result<Option<(DifferentialSignature, [Option<SemanticValue>; 2], bool)>, libafl::Error>
where
    C: MapObserver + for<'it> AsIter<'it, Item = C::Entry>,
    C::Entry: Into<u8> + Copy,
    EX: HasObservers,
    EX::Observers: MatchNameRef,
{
    let observers = executor.observers();
    let Some(map_a_handle) = feedback.map_refs.first() else {
        return Ok(None);
    };
    let Some(map_b_handle) = feedback.map_refs.get(1) else {
        return Ok(None);
    };
    let Some(map_a) = observers.get(map_a_handle) else {
        return Ok(None);
    };
    let Some(map_b) = observers.get(map_b_handle) else {
        return Ok(None);
    };
    let bytes_a: Vec<u8> = map_a.as_iter().map(|value| (*value).into()).collect();
    let bytes_b: Vec<u8> = map_b.as_iter().map(|value| (*value).into()).collect();
    let rv_a_handle = Handle::<ReturnValueObserver>::new(Cow::Borrowed("first_return"));
    let rv_b_handle = Handle::<ReturnValueObserver>::new(Cow::Borrowed("second_return"));
    let rv_a = observers
        .get(&rv_a_handle)
        .and_then(ReturnValueObserver::last_value);
    let rv_b = observers
        .get(&rv_b_handle)
        .and_then(ReturnValueObserver::last_value);
    let (exit_a, exit_b) = exit_class_pair(exit_kind);
    let signature = match signature_version {
        1 => compute_signature_v1(
            exit_a,
            exit_b,
            legacy_result(rv_a)?,
            legacy_result(rv_b)?,
            &bytes_a,
            &bytes_b,
        ),
        2 => compute_signature_v2(policy, exit_a, exit_b, rv_a, rv_b, &bytes_a, &bytes_b),
        _ => {
            return Err(libafl::Error::illegal_argument(
                "unsupported signature version",
            ))
        }
    };
    Ok(Some((
        signature,
        [rv_a, rv_b],
        semantic_difference(policy, exit_a, exit_b, rv_a, rv_b),
    )))
}

fn reset_replay_maps<C, EX>(
    feedback: &PairFuzzFeedback<C>,
    executor: &mut EX,
) -> Result<(), libafl::Error>
where
    C: MapObserver,
    EX: HasObservers,
    EX::Observers: MatchNameRef,
{
    let mut observers = executor.observers_mut();
    for handle in &feedback.map_refs {
        if let Some(observer) = observers.get_mut(handle) {
            observer.reset_map()?;
        }
    }
    Ok(())
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
    feedback: &PairFuzzFeedback<C>,
    state: &mut S,
    fuzzer: &mut F,
    executor: &mut EX,
    manager: &mut EM,
    policy: ResultPolicy,
    signature_version: u8,
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
    let replay_started = Instant::now();
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
    let mut signature_records = Vec::new();
    for (index, path) in files.into_iter().enumerate() {
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
        let mut signatures = Vec::with_capacity(SIGNATURE_REPETITIONS);
        let mut return_pairs = Vec::with_capacity(SIGNATURE_REPETITIONS);
        let mut differential_outcomes = Vec::with_capacity(SIGNATURE_REPETITIONS);
        let mut last_execution_succeeded = false;
        for _ in 0..SIGNATURE_REPETITIONS {
            reset_replay_maps(feedback, executor)?;
            match fuzzer.execute_input(state, executor, manager, &input) {
                Ok(exit_kind) => {
                    last_execution_succeeded = true;
                    if let Some((signature, return_pair, differential)) = signature_observation(
                        feedback,
                        executor,
                        exit_kind,
                        policy,
                        signature_version,
                    )? {
                        signatures.push(signature.to_hex());
                        return_pairs.push(return_pair);
                        differential_outcomes.push(differential);
                    }
                }
                Err(err) => {
                    warn("main", &format!("Execution error for {:?}: {}", path, err));
                    last_execution_succeeded = false;
                    break;
                }
            }
        }

        if last_execution_succeeded {
            let mut user_stats = Map::new();
            let num_edges = num_edges as u64;
            for (map_index, handle) in feedback.map_refs.iter().enumerate() {
                if let Some(observer) = executor.observers().get(handle) {
                    let initial: u8 = observer.initial().into();
                    for (index, item) in observer.as_iter().enumerate() {
                        let current: u8 = (*item).into();
                        union_maps[map_index][index] = union_maps[map_index][index].max(current);
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

            let record = replay_record(index, replay_started.elapsed().as_secs_f64(), user_stats);
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&json_log_path)
                .unwrap_or_else(|err| panic!("failed open json log: {err}"));
            writeln!(file, "{record}").ok();
            seen += 1;
        }

        let stable = signatures.len() == SIGNATURE_REPETITIONS
            && signatures.windows(2).all(|pair| pair[0] == pair[1])
            && return_pairs.windows(2).all(|pair| pair[0] == pair[1])
            && differential_outcomes
                .windows(2)
                .all(|pair| pair[0] == pair[1]);
        let differential = stable && differential_outcomes.first() == Some(&true);
        signature_records.push(SignatureReplayRecord {
            path: path.to_string_lossy().into_owned(),
            signatures,
            return_pairs,
            stable,
            differential,
            discovery_elapsed_nanos: persisted_elapsed_nanos(&path),
        });
    }

    let signature_index_path = log_dir
        .unwrap_or_else(|| Path::new("."))
        .join("replay-signatures.json");
    if let Some(parent) = signature_index_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &signature_index_path,
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "signature_version": signature_version,
            "result_policy": match policy {
                ResultPolicy::ZeroNonzero => "zero-nonzero",
                ResultPolicy::Exact => "exact",
            },
            "records": signature_records,
        }))?,
    )?;

    info(
        "main",
        &format!(
            "ReplayCov: processed {seen} inputs; signatures: {:?}",
            signature_index_path
        ),
    );
    if let Err(err) = manager.on_shutdown() {
        warn("main", &format!("Error during manager shutdown: {err}"));
    }
    manager.await_restart_safe();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{collect_replay_files, replay_record, sort_replay_files};
    use serde_json::{json, Map};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn sort_replay_files_orders_by_path() {
        let mut files = vec![PathBuf::from("z"), PathBuf::from("b"), PathBuf::from("a")];

        sort_replay_files(&mut files);

        assert_eq!(
            files,
            [PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("z")]
        );
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
            "pairfuzz-replay-test-{}-{unique}",
            std::process::id()
        ));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("direct"), b"direct").unwrap();
        fs::write(root.join(".lock"), b"lock").unwrap();
        fs::write(root.join("direct.metadata"), b"metadata").unwrap();
        fs::write(nested.join("ignored"), b"nested").unwrap();

        let result = collect_replay_files(&root).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], root.join("direct"));
        fs::remove_dir_all(&root).unwrap();
    }
}
