#!/usr/bin/env python3
"""Run and summarize reproducible DiffRay performance campaigns."""

from __future__ import annotations

import hashlib
import json
import math
import os
from pathlib import Path
import platform
import signal
import subprocess
import time
from typing import Any


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
        "seed_dir",
        "target_a",
        "target_b",
        "configurations",
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
        seed_dir: Path,
        target_a: Path,
        target_b: Path,
        configurations: tuple[str, ...],
    ) -> None:
        self.schema_version = schema_version
        self.workload = workload
        self.warmup_seconds = warmup_seconds
        self.trial_seconds = trial_seconds
        self.trials = trials
        self.campaign_seed = campaign_seed
        self.rng_seed_controlled = rng_seed_controlled
        self.seed_dir = seed_dir
        self.target_a = target_a
        self.target_b = target_b
        self.configurations = configurations

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

        return cls(
            schema_version=schema_version,
            workload=workload,
            warmup_seconds=warmup_seconds,
            trial_seconds=trial_seconds,
            trials=trials,
            campaign_seed=campaign_seed,
            rng_seed_controlled=rng_seed_controlled,
            seed_dir=seed_dir,
            target_a=target_a,
            target_b=target_b,
            configurations=tuple(sorted(raw_configurations)),
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
    process_cpu: dict[int, float] = {}
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
            peak_rss_kb = max(
                peak_rss_kb,
                _sample_process_group(process.pid, process_cpu),
            )
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
                peak_rss_kb = max(
                    peak_rss_kb,
                    _sample_process_group(process.pid, process_cpu),
                )
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
        cpu_seconds=sum(process_cpu.values()),
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


def _sample_process_group(process_group: int, process_cpu: dict[int, float]) -> int:
    ticks = os.sysconf("SC_CLK_TCK")
    page_kb = os.sysconf("SC_PAGE_SIZE") // 1024
    rss_kb = 0
    for stat_path in Path("/proc").glob("[0-9]*/stat"):
        try:
            raw = stat_path.read_text(encoding="ascii")
            fields = raw[raw.rfind(")") + 2 :].split()
            if int(fields[2]) != process_group:
                continue
            pid = int(stat_path.parent.name)
            cpu_seconds = (int(fields[11]) + int(fields[12])) / ticks
            process_cpu[pid] = max(process_cpu.get(pid, 0.0), cpu_seconds)
            rss_kb += max(0, int(fields[21])) * page_kb
        except (FileNotFoundError, ProcessLookupError):
            continue
    return rss_kb


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
