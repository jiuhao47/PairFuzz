#!/usr/bin/env python3
"""Run and summarize reproducible DiffRay performance campaigns."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import time
from typing import Any

try:
    from scripts.real_json_demo import results as real_json_results
except ModuleNotFoundError:
    from real_json_demo import results as real_json_results


class ManifestError(RuntimeError):
    """The performance workload manifest is invalid or incomplete."""


class CampaignManifest:
    """Validated immutable inputs for one family of performance trials."""

    __slots__ = (
        "schema_version",
        "workload",
        "warmup_seconds",
        "trial_seconds",
        "trials",
        "campaign_seed",
        "rng_seed_controlled",
        "repository_root",
        "seed_dir",
        "target_a",
        "target_b",
        "configurations",
        "configuration_modes",
    )

    def __init__(
        self,
        *,
        schema_version: int,
        workload: str,
        warmup_seconds: float,
        trial_seconds: float,
        trials: int,
        campaign_seed: int,
        rng_seed_controlled: bool,
        repository_root: Path,
        seed_dir: Path,
        target_a: Path,
        target_b: Path,
        configurations: tuple[str, ...],
        configuration_modes: dict[str, str],
    ) -> None:
        self.schema_version = schema_version
        self.workload = workload
        self.warmup_seconds = warmup_seconds
        self.trial_seconds = trial_seconds
        self.trials = trials
        self.campaign_seed = campaign_seed
        self.rng_seed_controlled = rng_seed_controlled
        self.repository_root = repository_root
        self.seed_dir = seed_dir
        self.target_a = target_a
        self.target_b = target_b
        self.configurations = configurations
        self.configuration_modes = configuration_modes

    @classmethod
    def load(cls, path: Path) -> CampaignManifest:
        path = path.resolve()
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise ManifestError(f"cannot read campaign manifest {path}: {error}") from error
        if not isinstance(payload, dict):
            raise ManifestError("campaign manifest must be a JSON object")

        schema_version = payload.get("schema_version")
        if type(schema_version) is not int or schema_version != 1:
            raise ManifestError("unsupported campaign manifest schema")

        repository_root = payload.get("repository_root")
        if not isinstance(repository_root, str) or not repository_root:
            raise ManifestError("repository_root must be a non-empty string")
        root = (path.parent / repository_root).resolve()

        workload = payload.get("workload")
        if not isinstance(workload, str) or not workload:
            raise ManifestError("workload must be a non-empty string")

        warmup_seconds = _positive_finite(payload.get("warmup_seconds"), "warmup_seconds")
        trial_seconds = _positive_finite(payload.get("trial_seconds"), "trial_seconds")

        trials = payload.get("trials")
        if type(trials) is not int or trials <= 0:
            raise ManifestError("trials must be a positive integer")

        campaign_seed = payload.get("campaign_seed")
        if (
            type(campaign_seed) is not int
            or campaign_seed < 0
            or campaign_seed > (1 << 64) - 1
        ):
            raise ManifestError("campaign_seed must be an unsigned 64-bit integer")

        rng_seed_controlled = payload.get("rng_seed_controlled")
        if type(rng_seed_controlled) is not bool:
            raise ManifestError("rng_seed_controlled must be a boolean")

        seed_dir = _repository_path(root, payload.get("seed_dir"), "seed_dir")
        if not seed_dir.is_dir():
            raise ManifestError(f"seed directory does not exist: {seed_dir}")
        target_a = _repository_path(root, payload.get("target_a"), "target_a")
        target_b = _repository_path(root, payload.get("target_b"), "target_b")

        raw_configurations = payload.get("configurations")
        if not isinstance(raw_configurations, dict) or not raw_configurations:
            raise ManifestError("configurations must be a non-empty object")
        for name, configuration in raw_configurations.items():
            if not isinstance(name, str) or not name or not isinstance(configuration, dict):
                raise ManifestError("each configuration must have a name and object value")
            if not isinstance(configuration.get("mode"), str):
                raise ManifestError(f"configuration {name} requires a mode")

        return cls(
            schema_version=schema_version,
            workload=workload,
            warmup_seconds=warmup_seconds,
            trial_seconds=trial_seconds,
            trials=trials,
            campaign_seed=campaign_seed,
            rng_seed_controlled=rng_seed_controlled,
            repository_root=root,
            seed_dir=seed_dir,
            target_a=target_a,
            target_b=target_b,
            configurations=tuple(sorted(raw_configurations)),
            configuration_modes={
                name: raw_configurations[name]["mode"]
                for name in sorted(raw_configurations)
            },
        )


class ProcessRecord:
    """Measured lifetime and resource use of one pinned campaign process."""

    __slots__ = (
        "command",
        "cpu",
        "exit_code",
        "completed_early",
        "timed_out",
        "term_sent",
        "kill_sent",
        "wall_seconds",
        "cpu_seconds",
        "peak_rss_kb",
    )

    def __init__(
        self,
        *,
        command: tuple[str, ...],
        cpu: int,
        exit_code: int,
        completed_early: bool,
        timed_out: bool,
        term_sent: bool,
        kill_sent: bool,
        wall_seconds: float,
        cpu_seconds: float,
        peak_rss_kb: int,
    ) -> None:
        self.command = command
        self.cpu = cpu
        self.exit_code = exit_code
        self.completed_early = completed_early
        self.timed_out = timed_out
        self.term_sent = term_sent
        self.kill_sent = kill_sent
        self.wall_seconds = wall_seconds
        self.cpu_seconds = cpu_seconds
        self.peak_rss_kb = peak_rss_kb

    def as_dict(self) -> dict[str, Any]:
        return {
            "command": list(self.command),
            "cpu": self.cpu,
            "exit_code": self.exit_code,
            "completed_early": self.completed_early,
            "timed_out": self.timed_out,
            "term_sent": self.term_sent,
            "kill_sent": self.kill_sent,
            "wall_seconds": self.wall_seconds,
            "cpu_seconds": self.cpu_seconds,
            "peak_rss_kb": self.peak_rss_kb,
        }


class TrialPaths:
    """Filesystem locations owned by one isolated measured trial."""

    __slots__ = (
        "root",
        "differences",
        "event_log",
        "fuzzer_log",
        "environment_json",
        "trial_json",
    )

    def __init__(self, root: Path) -> None:
        self.root = root
        self.differences = root / "diffs"
        self.event_log = root / "log"
        self.fuzzer_log = root / "fuzzer.log"
        self.environment_json = root / "environment.json"
        self.trial_json = root / "trial.json"


class ReplayRecord:
    """Three-way reproduction outcome for one unique raw input."""

    __slots__ = ("sha256", "paths", "return_pairs", "stable", "differential")

    def __init__(
        self,
        *,
        sha256: str,
        paths: tuple[str, ...],
        return_pairs: tuple[tuple[int, int], ...],
        stable: bool,
        differential: bool,
    ) -> None:
        self.sha256 = sha256
        self.paths = paths
        self.return_pairs = return_pairs
        self.stable = stable
        self.differential = differential

    def as_dict(self) -> dict[str, Any]:
        return {
            "sha256": self.sha256,
            "paths": list(self.paths),
            "return_pairs": [list(pair) for pair in self.return_pairs],
            "stable": self.stable,
            "differential": self.differential,
        }


class TrialRecord:
    """Metrics and retained raw artifacts for one measured trial."""

    __slots__ = (
        "configuration",
        "trial_index",
        "paths",
        "wall_seconds",
        "client_count",
        "executions",
        "cpu_seconds",
        "peak_rss_kb",
        "stable_nonduplicate_differences",
        "unstable_solutions",
        "byte_duplicates",
        "rng_seed_controlled",
    )

    def __init__(
        self,
        *,
        configuration: str,
        trial_index: int,
        paths: TrialPaths,
        wall_seconds: float,
        client_count: int,
        executions: int,
        cpu_seconds: float,
        peak_rss_kb: int,
        stable_nonduplicate_differences: int,
        unstable_solutions: int,
        byte_duplicates: int,
        rng_seed_controlled: bool,
    ) -> None:
        self.configuration = configuration
        self.trial_index = trial_index
        self.paths = paths
        self.wall_seconds = wall_seconds
        self.client_count = client_count
        self.executions = executions
        self.cpu_seconds = cpu_seconds
        self.peak_rss_kb = peak_rss_kb
        self.stable_nonduplicate_differences = stable_nonduplicate_differences
        self.unstable_solutions = unstable_solutions
        self.byte_duplicates = byte_duplicates
        self.rng_seed_controlled = rng_seed_controlled

    def as_dict(self) -> dict[str, Any]:
        return {
            "configuration": self.configuration,
            "trial_index": self.trial_index,
            "raw_directory": str(self.paths.root),
            "fuzzer_log": str(self.paths.fuzzer_log),
            "environment_json": str(self.paths.environment_json),
            "trial_json": str(self.paths.trial_json),
            "wall_seconds": self.wall_seconds,
            "client_count": self.client_count,
            "executions": self.executions,
            "cpu_seconds": self.cpu_seconds,
            "peak_rss_kb": self.peak_rss_kb,
            "stable_nonduplicate_differences": self.stable_nonduplicate_differences,
            "unstable_solutions": self.unstable_solutions,
            "byte_duplicates": self.byte_duplicates,
            "rng_seed_controlled": self.rng_seed_controlled,
        }


def _positive_finite(value: Any, field: str) -> float:
    if type(value) not in (int, float) or not math.isfinite(value) or value <= 0:
        raise ManifestError(f"{field} must be a finite positive number")
    return float(value)


def _repository_path(root: Path, value: Any, field: str) -> Path:
    if not isinstance(value, str) or not value:
        raise ManifestError(f"{field} must be a non-empty string")
    candidate = (root / value).resolve()
    try:
        candidate.relative_to(root)
    except ValueError as error:
        raise ManifestError(f"{field} escapes repository root: {value}") from error
    return candidate


def rotate_configurations(names: tuple[str, ...], trial_index: int) -> tuple[str, ...]:
    ordered = tuple(sorted(names))
    if not ordered:
        return ()
    offset = trial_index % len(ordered)
    return ordered[offset:] + ordered[:offset]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def create_trial(root: Path, configuration: str, trial_index: int) -> TrialPaths:
    if not configuration or Path(configuration).name != configuration:
        raise ValueError("configuration must be one path component")
    if type(trial_index) is not int or trial_index < 0:
        raise ValueError("trial_index must be a non-negative integer")

    parent = root.resolve() / configuration
    parent.mkdir(parents=True, exist_ok=True)
    suffix = 0
    while True:
        name = f"trial-{trial_index}" if suffix == 0 else f"trial-{trial_index}-{suffix}"
        trial_root = parent / name
        try:
            trial_root.mkdir()
            break
        except FileExistsError:
            suffix += 1

    paths = TrialPaths(trial_root)
    paths.differences.mkdir()
    paths.event_log.mkdir()
    return paths


def replay_raw_solutions(
    solution_dir: Path, repetitions: int = 3
) -> list[ReplayRecord]:
    if type(repetitions) is not int or repetitions < 3:
        raise ValueError("repetitions must be an integer of at least three")
    if not solution_dir.exists():
        return []

    by_hash: dict[str, list[Path]] = {}
    for path in sorted(solution_dir.iterdir()):
        if (
            not path.is_file()
            or path.name.startswith(".")
            or path.name.endswith(".metadata")
        ):
            continue
        by_hash.setdefault(sha256_file(path), []).append(path)

    records = []
    for digest, paths in sorted(by_hash.items()):
        data = paths[0].read_bytes()
        outcomes = tuple(tuple(real_json_results(data)) for _ in range(repetitions))
        if any(len(pair) != 2 for pair in outcomes):
            raise RuntimeError("real JSON replay must return exactly two statuses")
        stable = len(set(outcomes)) == 1
        differential = stable and outcomes[0][0] != outcomes[0][1]
        records.append(
            ReplayRecord(
                sha256=digest,
                paths=tuple(str(path) for path in paths),
                return_pairs=outcomes,
                stable=stable,
                differential=differential,
            )
        )
    return records


def aggregate_trials(records: list[TrialRecord]) -> dict[str, Any]:
    configurations: dict[str, dict[str, Any]] = {}
    for name in sorted({record.configuration for record in records}):
        selected = [record for record in records if record.configuration == name]
        allocated_cpu_hours = sum(
            record.client_count * record.wall_seconds for record in selected
        ) / 3600.0
        wall_seconds = sum(record.wall_seconds for record in selected)
        stable = sum(record.stable_nonduplicate_differences for record in selected)
        executions = sum(record.executions for record in selected)
        configurations[name] = {
            "trial_count": len(selected),
            "allocated_cpu_hours": allocated_cpu_hours,
            "measured_process_cpu_hours": sum(
                record.cpu_seconds for record in selected
            )
            / 3600.0,
            "executions": executions,
            "executions_per_second": executions / wall_seconds,
            "stable_nonduplicate_differences": stable,
            "stable_nonduplicate_differences_per_cpu_hour": (
                stable / allocated_cpu_hours
            ),
            "unstable_solutions": sum(
                record.unstable_solutions for record in selected
            ),
            "byte_duplicates": sum(record.byte_duplicates for record in selected),
            "peak_rss_kb": max(record.peak_rss_kb for record in selected),
            "trials": [record.as_dict() for record in selected],
        }
    return {
        "schema_version": 1,
        "behavioral_signature": "pending-v1-coverage-replay",
        "configurations": configurations,
    }


def clean_environment(root: Path) -> dict[str, str]:
    environment = os.environ.copy()
    for name in tuple(environment):
        if name.startswith(("__AFL_", "__DIFFRAY_")):
            del environment[name]
    environment["ASAN_OPTIONS"] = "abort_on_error=1:detect_leaks=false"
    environment["CARGO_TARGET_DIR"] = str(root.resolve() / "target")
    return environment


def run_bounded(
    command: list[str],
    cwd: Path,
    env: dict[str, str],
    duration: float,
    cpu: int,
    log_path: Path,
) -> ProcessRecord:
    """Run one CPU-pinned process for at most duration and reap its process group."""
    duration = _positive_finite(duration, "duration")
    if type(cpu) is not int or cpu < 0:
        raise ValueError("cpu must be a non-negative integer")
    if not command:
        raise ValueError("command must not be empty")

    cwd = cwd.resolve()
    log_path.parent.mkdir(parents=True, exist_ok=True)
    launched = ["taskset", "--cpu-list", str(cpu), *command]
    started = time.monotonic()
    peak_cpu_seconds = 0.0
    peak_rss_kb = 0
    term_sent = False
    kill_sent = False
    timed_out = False

    with log_path.open("wb") as output:
        process = subprocess.Popen(
            launched,
            cwd=cwd,
            env=env,
            stdout=output,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        while True:
            sample_rss_kb, sample_cpu_seconds = _sample_process_tree(process.pid)
            peak_rss_kb = max(peak_rss_kb, sample_rss_kb)
            peak_cpu_seconds = max(peak_cpu_seconds, sample_cpu_seconds)
            status = process.poll()
            elapsed = time.monotonic() - started
            if status is not None:
                break
            if elapsed >= duration:
                timed_out = True
                break
            time.sleep(min(0.1, duration - elapsed))

        if timed_out:
            term_sent = _signal_group(process.pid, signal.SIGTERM)
            grace_deadline = time.monotonic() + 0.25
            while _group_exists(process.pid) and time.monotonic() < grace_deadline:
                sample_rss_kb, sample_cpu_seconds = _sample_process_tree(process.pid)
                peak_rss_kb = max(peak_rss_kb, sample_rss_kb)
                peak_cpu_seconds = max(peak_cpu_seconds, sample_cpu_seconds)
                time.sleep(0.01)
            if _group_exists(process.pid):
                kill_sent = _signal_group(process.pid, signal.SIGKILL)
            process.wait()
            reap_deadline = time.monotonic() + 1.0
            while (
                _group_has_live_processes(process.pid)
                and time.monotonic() < reap_deadline
            ):
                time.sleep(0.01)
        else:
            process.wait()

    wall_seconds = time.monotonic() - started
    return ProcessRecord(
        command=tuple(command),
        cpu=cpu,
        exit_code=process.returncode,
        completed_early=not timed_out,
        timed_out=timed_out,
        term_sent=term_sent,
        kill_sent=kill_sent,
        wall_seconds=wall_seconds,
        cpu_seconds=peak_cpu_seconds,
        peak_rss_kb=peak_rss_kb,
    )


def _signal_group(process_group: int, requested_signal: int) -> bool:
    try:
        os.killpg(process_group, requested_signal)
    except ProcessLookupError:
        return False
    return True


def _group_exists(process_group: int) -> bool:
    try:
        os.killpg(process_group, 0)
    except ProcessLookupError:
        return False
    return True


def _group_has_live_processes(process_group: int) -> bool:
    for stat_path in Path("/proc").glob("[0-9]*/stat"):
        try:
            raw = stat_path.read_text(encoding="ascii")
            fields = raw[raw.rfind(")") + 2 :].split()
            if int(fields[2]) == process_group and fields[0] != "Z":
                return True
        except (FileNotFoundError, ProcessLookupError):
            continue
    return False


def _sample_process_tree(root_pid: int) -> tuple[int, float]:
    ticks = os.sysconf("SC_CLK_TCK")
    page_kb = os.sysconf("SC_PAGE_SIZE") // 1024
    processes: dict[int, tuple[int, float, int]] = {}
    for stat_path in Path("/proc").glob("[0-9]*/stat"):
        try:
            raw = stat_path.read_text(encoding="ascii")
            fields = raw[raw.rfind(")") + 2 :].split()
            pid = int(stat_path.parent.name)
            processes[pid] = (
                int(fields[1]),
                sum(int(fields[index]) for index in (11, 12, 13, 14)) / ticks,
                max(0, int(fields[21])) * page_kb,
            )
        except (FileNotFoundError, ProcessLookupError):
            continue

    children: dict[int, list[int]] = {}
    for pid, (parent, _, _) in processes.items():
        children.setdefault(parent, []).append(pid)
    descendants = []
    pending = [root_pid]
    while pending:
        pid = pending.pop()
        if pid not in processes:
            continue
        descendants.append(pid)
        pending.extend(children.get(pid, ()))
    return (
        sum(processes[pid][2] for pid in descendants),
        sum(processes[pid][1] for pid in descendants),
    )


def capture_environment(
    root: Path,
    command: list[str],
    cpu: int,
    corpus: Path,
    targets: tuple[Path, Path],
) -> dict[str, Any]:
    """Capture the identities required to reproduce one measurement campaign."""
    root = root.resolve()
    cpu_root = Path(f"/sys/devices/system/cpu/cpu{cpu}")
    governor_path = cpu_root / "cpufreq" / "scaling_governor"
    return {
        "schema_version": 1,
        "source_baseline": "2ebbb17dcb9711dcf29c356e4ce044fc163ea6c0",
        "measurement_commit": _output(["git", "rev-parse", "HEAD"], root),
        "libafl_commit": _output(
            ["git", "-C", str(root / "LibAFL"), "rev-parse", "HEAD"], root
        ),
        "rustc": _output(["rustc", "--version", "--verbose"], root),
        "clang": _output(["clang", "--version"], root),
        "kernel": platform.release(),
        "machine": platform.machine(),
        "cpu_model": _cpu_model(),
        "cpu_topology": _output(
            ["lscpu", "-e=CPU,CORE,SOCKET,ONLINE", "--json"], root
        ),
        "governor": (
            governor_path.read_text(encoding="ascii").strip()
            if governor_path.is_file()
            else None
        ),
        "cpu": cpu,
        "command": command,
        "corpus_sha256": _sha256_tree(corpus),
        "target_sha256": {str(path): sha256_file(path) for path in targets},
    }


def _output(command: list[str], cwd: Path) -> str:
    return subprocess.run(
        command,
        cwd=cwd,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def _cpu_model() -> str:
    for line in Path("/proc/cpuinfo").read_text(encoding="ascii").splitlines():
        if line.startswith("model name"):
            return line.partition(":")[2].strip()
    return platform.processor()


def _sha256_tree(root: Path) -> str:
    digest = hashlib.sha256()
    root = root.resolve()
    for path in sorted(candidate for candidate in root.rglob("*") if candidate.is_file()):
        relative = path.relative_to(root).as_posix().encode()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        with path.open("rb") as source:
            for chunk in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(chunk)
    return digest.hexdigest()


def _fuzzer_command(manifest: CampaignManifest, paths: TrialPaths, mode: str) -> list[str]:
    return [
        str(manifest.repository_root / "target" / "debug" / "diffray"),
        "--mode",
        mode,
        "-i",
        str(manifest.seed_dir),
        "-o",
        str(paths.differences),
        "-l",
        str(paths.event_log),
        "--prog-a",
        str(manifest.target_a),
        "--prog-b",
        str(manifest.target_b),
        "--persistent-mode",
        "disabled",
        "--simple-monitor",
        "disabled",
        "--json-heartbeat-secs",
        "1",
    ]


def _write_json(path: Path, payload: dict[str, Any]) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(path)


def _execution_count(event_log: Path) -> int:
    stats = event_log / "fuzzer_stats.json"
    if not stats.is_file():
        return 0
    executions = 0
    for line in stats.read_text(encoding="utf-8").splitlines():
        if line.strip():
            executions = int(json.loads(line)["executions"])
    return executions


def _run_trial(
    manifest: CampaignManifest,
    paths: TrialPaths,
    configuration: str,
    trial_index: int,
    cpu: int,
    duration: float,
) -> TrialRecord:
    command = _fuzzer_command(
        manifest,
        paths,
        manifest.configuration_modes[configuration],
    )
    environment = capture_environment(
        manifest.repository_root,
        command,
        cpu,
        manifest.seed_dir,
        (manifest.target_a, manifest.target_b),
    )
    _write_json(paths.environment_json, environment)
    process = run_bounded(
        command,
        cwd=paths.root,
        env=clean_environment(manifest.repository_root),
        duration=duration,
        cpu=cpu,
        log_path=paths.fuzzer_log,
    )
    if process.completed_early:
        _write_json(
            paths.trial_json,
            {
                "schema_version": 1,
                "status": "early-exit",
                "configuration": configuration,
                "trial_index": trial_index,
                "process": process.as_dict(),
            },
        )
        raise RuntimeError(
            f"campaign process exited early ({process.exit_code}); evidence: {paths.root}"
        )

    replays = replay_raw_solutions(paths.differences)
    record = TrialRecord(
        configuration=configuration,
        trial_index=trial_index,
        paths=paths,
        wall_seconds=duration,
        client_count=1,
        executions=_execution_count(paths.event_log),
        cpu_seconds=process.cpu_seconds,
        peak_rss_kb=process.peak_rss_kb,
        stable_nonduplicate_differences=sum(
            replay.stable and replay.differential for replay in replays
        ),
        unstable_solutions=sum(not replay.stable for replay in replays),
        byte_duplicates=sum(len(replay.paths) - 1 for replay in replays),
        rng_seed_controlled=manifest.rng_seed_controlled,
    )
    _write_json(
        paths.trial_json,
        {
            "schema_version": 1,
            "status": "complete",
            "process": process.as_dict(),
            "record": record.as_dict(),
            "replays": [replay.as_dict() for replay in replays],
        },
    )
    return record


def run_campaign(
    manifest_path: Path,
    cpu: int,
    *,
    warmup_seconds: float | None = None,
    trial_seconds: float | None = None,
    trials: int | None = None,
) -> Path:
    manifest = CampaignManifest.load(manifest_path)
    warmup_seconds = manifest.warmup_seconds if warmup_seconds is None else _positive_finite(warmup_seconds, "warmup_seconds")
    trial_seconds = manifest.trial_seconds if trial_seconds is None else _positive_finite(trial_seconds, "trial_seconds")
    trial_count = manifest.trials if trials is None else trials
    if type(trial_count) is not int or trial_count <= 0:
        raise ManifestError("trials must be a positive integer")

    timestamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    campaign_root = (
        manifest.repository_root
        / "run"
        / "performance"
        / f"{timestamp}-{os.getpid()}-{time.time_ns() % 1_000_000_000:09d}"
    )
    campaign_root.mkdir(parents=True)
    _write_json(
        campaign_root / "campaign.json",
        {
            "schema_version": 1,
            "manifest": str(manifest_path.resolve()),
            "campaign_seed": manifest.campaign_seed,
            "rng_seed_controlled": manifest.rng_seed_controlled,
            "cpu": cpu,
            "warmup_seconds": warmup_seconds,
            "trial_seconds": trial_seconds,
            "trials": trial_count,
        },
    )

    for configuration in manifest.configurations:
        warmup = create_trial(campaign_root / "warmup", configuration, 0)
        _run_trial(
            manifest,
            warmup,
            configuration,
            -1,
            cpu,
            warmup_seconds,
        )

    records = []
    for trial_index in range(trial_count):
        for configuration in rotate_configurations(
            manifest.configurations, trial_index
        ):
            paths = create_trial(campaign_root, configuration, trial_index)
            records.append(
                _run_trial(
                    manifest,
                    paths,
                    configuration,
                    trial_index,
                    cpu,
                    trial_seconds,
                )
            )
            summary = aggregate_trials(records)
            summary.update(
                {
                    "campaign_root": str(campaign_root),
                    "campaign_seed": manifest.campaign_seed,
                    "rng_seed_controlled": manifest.rng_seed_controlled,
                }
            )
            _write_json(campaign_root / "summary.json", summary)
    return campaign_root


def aggregate_campaign(campaign_root: Path) -> dict[str, Any]:
    records = []
    for path in sorted(campaign_root.glob("*/trial-*/trial.json")):
        payload = json.loads(path.read_text(encoding="utf-8"))
        if payload.get("status") != "complete":
            continue
        item = payload["record"]
        records.append(
            TrialRecord(
                configuration=item["configuration"],
                trial_index=item["trial_index"],
                paths=TrialPaths(path.parent),
                wall_seconds=item["wall_seconds"],
                client_count=item["client_count"],
                executions=item["executions"],
                cpu_seconds=item["cpu_seconds"],
                peak_rss_kb=item["peak_rss_kb"],
                stable_nonduplicate_differences=item[
                    "stable_nonduplicate_differences"
                ],
                unstable_solutions=item["unstable_solutions"],
                byte_duplicates=item["byte_duplicates"],
                rng_seed_controlled=item["rng_seed_controlled"],
            )
        )
    if not records:
        raise RuntimeError(f"no complete measured trials in {campaign_root}")
    campaign = json.loads((campaign_root / "campaign.json").read_text(encoding="utf-8"))
    summary = aggregate_trials(records)
    summary.update(
        {
            "campaign_root": str(campaign_root.resolve()),
            "campaign_seed": campaign["campaign_seed"],
            "rng_seed_controlled": campaign["rng_seed_controlled"],
        }
    )
    _write_json(campaign_root / "summary.json", summary)
    return summary


def export_campaign(performance_root: Path, output: Path) -> dict[str, Any]:
    candidates = sorted(
        path for path in performance_root.iterdir() if (path / "summary.json").is_file()
    )
    if not candidates:
        raise RuntimeError(f"no complete campaign under {performance_root}")
    campaign_root = candidates[-1]
    summary = aggregate_campaign(campaign_root)
    first_trial = summary["configurations"][next(iter(summary["configurations"]))][
        "trials"
    ][0]
    environment = json.loads(
        Path(first_trial["environment_json"]).read_text(encoding="utf-8")
    )
    evidence = {
        "schema_version": 1,
        "environment": environment,
        "results": summary,
        "raw_campaign": str(campaign_root.resolve()),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    _write_json(output, evidence)
    return evidence


def _positive_seconds(value: str) -> float:
    try:
        return _positive_finite(float(value), "duration")
    except (ValueError, ManifestError) as error:
        raise argparse.ArgumentTypeError(str(error)) from error


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    run_parser = commands.add_parser("run", help="run an isolated pinned campaign")
    run_parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("examples/real-json/performance-campaign.json"),
    )
    run_parser.add_argument("--cpu", required=True, type=int)
    run_parser.add_argument("--warmup-seconds", type=_positive_seconds)
    run_parser.add_argument("--trial-seconds", type=_positive_seconds)
    run_parser.add_argument("--trials", type=int)
    aggregate_parser = commands.add_parser(
        "aggregate", help="rebuild a campaign summary from retained trials"
    )
    aggregate_parser.add_argument("campaign", type=Path)
    export_parser = commands.add_parser(
        "export", help="export compact evidence from the latest campaign"
    )
    export_parser.add_argument("--latest", action="store_true", required=True)
    export_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    if args.command == "run":
        campaign_root = run_campaign(
            args.manifest,
            args.cpu,
            warmup_seconds=args.warmup_seconds,
            trial_seconds=args.trial_seconds,
            trials=args.trials,
        )
        print(campaign_root)
    elif args.command == "aggregate":
        summary = aggregate_campaign(args.campaign.resolve())
        print(json.dumps(summary, indent=2, sort_keys=True))
    elif args.command == "export":
        root = Path(__file__).resolve().parents[1]
        export_campaign(root / "run" / "performance", args.output.resolve())
        print(args.output.resolve())


if __name__ == "__main__":
    try:
        main()
    except (ManifestError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        raise SystemExit(1) from error
