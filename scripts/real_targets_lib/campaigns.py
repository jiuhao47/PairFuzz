import hashlib
import json
import math
import os
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

try:
    from scripts.performance_campaign import (
        clean_environment,
        generate_signature_index,
        run_bounded,
        sha256_file,
    )
except ModuleNotFoundError:
    from performance_campaign import (
        clean_environment,
        generate_signature_index,
        run_bounded,
        sha256_file,
    )

from .registry import pair_spec


class CampaignError(RuntimeError):
    pass


@dataclass(frozen=True)
class BuildManifest:
    path: Path
    root: Path
    payload: dict
    targets: dict[str, Path]


def _root_from_manifest(path: Path) -> Path:
    for candidate in (path.resolve(), *path.resolve().parents):
        if (candidate / "third_party").is_dir():
            return candidate
    raise CampaignError(f"cannot locate repository root from manifest {path}")


def load_build_manifest(path: Path, *, expected_pair=None, expected_profile=None) -> BuildManifest:
    path = path.resolve()
    try:
        payload = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise CampaignError(f"cannot read build manifest {path}: {error}") from error
    if payload.get("schema_version") != 1 or payload.get("status") != "complete":
        raise CampaignError("build manifest is incomplete or has an unsupported schema")
    if expected_pair is not None and payload.get("pair") != expected_pair:
        raise CampaignError(f"manifest pair {payload.get('pair')!r} does not match {expected_pair!r}")
    if expected_profile is not None and payload.get("profile") != expected_profile:
        raise CampaignError(f"manifest profile {payload.get('profile')!r} does not match {expected_profile!r}")
    root = _root_from_manifest(path)
    targets = {}
    for role in ("a", "b"):
        record = payload.get("targets", {}).get(role, {})
        relative = record.get("path")
        if not isinstance(relative, str):
            raise CampaignError(f"manifest target {role} has no path")
        target = (root / relative).resolve()
        if not target.is_file():
            raise CampaignError(f"manifest target {role} is missing: {target}")
        actual = sha256_file(target)
        if actual != record.get("sha256"):
            raise CampaignError(f"target {role} SHA-256 mismatch: expected {record.get('sha256')}, got {actual}")
        targets[role] = target
    return BuildManifest(path, root, payload, targets)


def campaign_command(root: Path, manifest: BuildManifest, output: Path, cpu: int, seed: int) -> list[str]:
    seed_dir = (root / pair_spec(manifest.payload["pair"]).seed_dir).resolve()
    return [
        str((root / "target/debug/diffray").resolve()),
        "--pipeline", "fast",
        "--mode", "single-cov",
        "--result-policy", "exact",
        "--signature-version", "2",
        "--persistent-mode", "disabled",
        "--simple-monitor", "disabled",
        "--mutator", "havoc",
        "--schedule", "explore",
        "--campaign-seed", str(seed),
        "--client-core-id", str(cpu),
        "--json-heartbeat-secs", "1",
        "--input", str(seed_dir),
        "--output", str((output / "diffs").resolve()),
        "--log", str((output / "log").resolve()),
        "--prog-a", str(manifest.targets["a"]),
        "--prog-b", str(manifest.targets["b"]),
    ]


def _write_json(path: Path, payload: dict) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


def _run_target(target: Path, input_path: Path, output_path: Path) -> dict:
    completed = subprocess.run(
        [str(target), "--replay", str(input_path), "--normalized-output", str(output_path)],
        cwd=target.parent,
        capture_output=True,
        text=True,
        check=False,
        timeout=30,
        env={name: value for name, value in os.environ.items() if not name.startswith(("__AFL_", "__DIFFRAY_"))},
    )
    if completed.returncode != 0:
        return {
            "exit_code": completed.returncode if completed.returncode >= 0 else None,
            "signal": -completed.returncode if completed.returncode < 0 else None,
            "class": "process-error",
            "semantic_code": None,
            "normalized_sha256": None,
        }
    try:
        report = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise CampaignError(f"standalone replay returned invalid JSON for {target}") from error
    report["exit_code"] = 0
    report["signal"] = None
    report["normalized_sha256"] = sha256_file(output_path) if output_path.is_file() else None
    return report


def _standalone_outcome(report: dict) -> tuple:
    return (
        report.get("exit_code"),
        report.get("signal"),
        report.get("semantic_code"),
        report.get("normalized_sha256"),
    )


def replay_pair(manifest: Path, input_path: Path, output_dir: Path) -> dict:
    build = load_build_manifest(manifest)
    if not input_path.is_file():
        raise CampaignError(f"replay input is missing: {input_path}")
    output_dir.mkdir(parents=True, exist_ok=False)
    runs = []
    for repetition in range(3):
        pair = {}
        for role in ("a", "b"):
            pair[role] = _run_target(build.targets[role], input_path.resolve(), output_dir / f"{role}-{repetition}.canonical")
        if (
            pair["a"]["semantic_code"] is not None
            and pair["a"]["semantic_code"] == pair["b"]["semantic_code"]
            and pair["a"]["normalized_sha256"] != pair["b"]["normalized_sha256"]
        ):
            raise CampaignError("semantic fingerprint collision/audit discrepancy")
        runs.append(pair)
    stable = all(runs[index] == runs[0] for index in range(1, 3))
    differential = stable and _standalone_outcome(runs[0]["a"]) != _standalone_outcome(runs[0]["b"])
    result = {
        "schema_version": 1,
        "input": str(input_path.resolve()),
        "input_sha256": sha256_file(input_path),
        "manifest": str(build.path),
        "stable": stable,
        "differential": differential,
        "runs": runs,
    }
    _write_json(output_dir / "replay.json", result)
    return result


def _executions(log_dir: Path) -> int:
    stats = log_dir / "fuzzer_stats.json"
    if not stats.is_file():
        return 0
    value = 0
    for line in stats.read_text().splitlines():
        if line.strip():
            value = int(json.loads(line).get("executions", value))
    return value


def _verified_seed_paths(root: Path, build: BuildManifest) -> list[Path]:
    seed_dir = (root / pair_spec(build.payload["pair"]).seed_dir).resolve()
    recorded = build.payload.get("seed_sha256")
    if not isinstance(recorded, dict) or not recorded:
        raise CampaignError("build manifest has no seed hashes")
    paths = []
    for relative, expected in sorted(recorded.items()):
        path = (root / relative).resolve()
        if not path.is_relative_to(seed_dir) or not path.is_file():
            raise CampaignError(f"manifest seed escapes or is missing: {relative}")
        actual = sha256_file(path)
        if actual != expected:
            raise CampaignError(
                f"seed SHA-256 mismatch for {relative}: expected {expected}, got {actual}"
            )
        paths.append(path)
    actual_paths = sorted(path.resolve() for path in seed_dir.iterdir() if path.is_file())
    if paths != actual_paths:
        raise CampaignError("manifest seed set does not match the selected pair seed directory")
    return paths


def _preflight_seeds(build: BuildManifest, seeds: list[Path], output: Path) -> None:
    output.mkdir()
    records = []
    failed = None
    for index, seed in enumerate(seeds):
        pair = {}
        for role in ("a", "b"):
            normalized = output / f"{index:03d}-{role}.canonical"
            pair[role] = _run_target(build.targets[role], seed, normalized)
            normalized.unlink(missing_ok=True)
            if pair[role].get("class") in ("adapter-error", "process-error"):
                failed = (seed, role, pair[role]["class"])
        records.append({"seed": str(seed), "targets": pair})
    _write_json(output / "summary.json", {"schema_version": 1, "seeds": records})
    if failed is not None:
        seed, role, class_name = failed
        raise CampaignError(f"seed preflight returned {class_name} for target {role}: {seed}")


def run_pair(root: Path, pair: str, manifest: Path, seconds: float, cpu: int, seed: int) -> Path:
    try:
        spec = pair_spec(pair)
    except ValueError as error:
        raise CampaignError(str(error)) from error
    if not spec.build_supported:
        raise CampaignError(f"pair {pair} is not implemented")
    if not isinstance(seconds, (int, float)) or not math.isfinite(seconds) or seconds <= 0:
        raise CampaignError("seconds must be positive and finite")
    if type(cpu) is not int or cpu < 0 or cpu not in os.sched_getaffinity(0):
        raise CampaignError("cpu must be a non-negative allowed logical CPU")
    if type(seed) is not int or not 0 <= seed <= (1 << 64) - 1:
        raise CampaignError("seed must be an unsigned 64-bit integer")
    root = root.resolve()
    build = load_build_manifest(manifest, expected_pair=pair)
    if build.root != root:
        raise CampaignError("manifest belongs to a different repository")
    campaign_root = root / "run/real-targets/campaigns" / pair / f"{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}-{os.getpid()}-{time.time_ns()}"
    campaign_root.mkdir(parents=True)
    _preflight_seeds(build, _verified_seed_paths(root, build), campaign_root / "seed-preflight")
    command = campaign_command(root, build, campaign_root, cpu, seed)
    _write_json(campaign_root / "campaign.json", {"schema_version": 1, "manifest": str(build.path), "manifest_sha256": sha256_file(build.path), "command": command, "cpu": cpu, "seed": seed, "seconds": seconds})
    process = run_bounded(command, root, clean_environment(root), seconds, cpu, campaign_root / "fuzzer.log")
    if process.completed_early:
        _write_json(campaign_root / "summary.json", {"schema_version": 1, "status": "early-exit", "process": process.as_dict()})
        raise CampaignError(f"campaign exited early ({process.exit_code}); evidence: {campaign_root}")
    solution_root = campaign_root / "diffs/client-000/solutions"
    solutions = [] if not solution_root.is_dir() else [path for path in sorted(solution_root.iterdir()) if path.is_file() and not path.name.startswith(".") and not path.name.endswith(".metadata")]
    signature_index = (
        generate_signature_index(
            root,
            solution_root,
            build.targets["a"],
            build.targets["b"],
            signature_version=2,
            result_policy="exact",
        )
        if solutions
        else {}
    )
    replay_records = []
    audit_root = campaign_root / "audit"
    audit_root.mkdir()
    for index, solution in enumerate(solutions):
        record = replay_pair(build.path, solution, audit_root / f"solution-{index:06d}")
        coverage = signature_index.get(str(solution.resolve()))
        if coverage is None:
            coverage_report = {
                "stable": False,
                "differential": False,
                "signatures": [],
                "signature": None,
                "return_pairs": [],
                "standalone_codes_match": False,
            }
        else:
            signatures = coverage.get("signatures", [])
            standalone_pairs = [
                [
                    int(run["a"]["semantic_code"], 16),
                    int(run["b"]["semantic_code"], 16),
                ]
                for run in record["runs"]
                if run["a"]["semantic_code"] is not None
                and run["b"]["semantic_code"] is not None
            ]
            coverage_pairs = coverage.get("return_pairs", [])
            coverage_report = {
                "stable": bool(coverage.get("stable")),
                "differential": bool(coverage.get("differential")),
                "signatures": signatures,
                "signature": signatures[0]
                if len(signatures) == 3 and len(set(signatures)) == 1
                else None,
                "return_pairs": coverage_pairs,
                "standalone_codes_match": (
                    not standalone_pairs or standalone_pairs == coverage_pairs
                ),
                "discovery_elapsed_nanos": coverage.get("discovery_elapsed_nanos"),
            }
        record["coverage_replay"] = coverage_report
        record["stable"] = (
            record["stable"]
            and record["differential"]
            and coverage_report["stable"]
            and coverage_report["differential"]
            and coverage_report["signature"] is not None
            and coverage_report["standalone_codes_match"]
        )
        replay_records.append(record)
    stable_signatures = {
        record["coverage_replay"]["signature"]
        for record in replay_records
        if record["stable"]
    }
    summary = {
        "schema_version": 1,
        "status": "complete",
        "pair": pair,
        "operation": build.payload["operation"],
        "profile": build.payload["profile"],
        "seed": seed,
        "semantic_abi_version": 1,
        "result_policy": "exact",
        "signature_version": 2,
        "build_manifest_sha256": sha256_file(build.path),
        "target_sha256": {role: build.payload["targets"][role]["sha256"] for role in ("a", "b")},
        "seed_sha256": build.payload.get("seed_sha256", {}),
        "wall_seconds": process.wall_seconds,
        "cpu_seconds": process.cpu_seconds,
        "peak_rss_kb": process.peak_rss_kb,
        "executions": _executions(campaign_root / "log"),
        "solutions_saved": len(solutions),
        "stable_signatures": len(stable_signatures),
        "unstable_or_unreproducible": sum(not record["stable"] for record in replay_records),
        "zero_solutions": len(solutions) == 0,
        "process": process.as_dict(),
        "replays": replay_records,
    }
    _write_json(campaign_root / "summary.json", summary)
    return campaign_root


def verify_pair(root: Path, pair: str, profile: str, manifest_path: Path | None = None) -> Path:
    if manifest_path is None:
        from .builds import build_pair

        manifest_path = build_pair(root, pair, profile)
    build = load_build_manifest(manifest_path, expected_pair=pair, expected_profile=profile)
    fixtures_path = root / f"examples/real-targets/{pair}/fixtures.json"
    if not fixtures_path.is_file():
        raise CampaignError(f"verification fixtures are missing for {pair}")
    fixtures = json.loads(fixtures_path.read_text())["fixtures"]
    verify_root = root / "run/real-targets/verify" / pair / profile / f"{time.time_ns()}"
    verify_root.mkdir(parents=True)
    records = []
    for index, fixture in enumerate(fixtures):
        input_path = root / f"examples/real-targets/{pair}" / fixture["path"]
        replay = replay_pair(build.path, input_path, verify_root / f"fixture-{index:03d}")
        if not replay.get("stable"):
            raise CampaignError(
                f"fixture {fixture['name']} produced an unstable three-run replay"
            )
        first = replay["runs"][0]
        if fixture.get("matching"):
            for role in ("a", "b"):
                report = first[role]
                dimensions_match = "dimensions" not in fixture or [
                    report["width"],
                    report["height"],
                ] == fixture["dimensions"]
                if (
                    report["class"] != fixture["expected_class"]
                    or not dimensions_match
                    or report["normalized_sha256"] != fixture["canonical_sha256"]
                ):
                    raise CampaignError(f"fixture {fixture['name']} does not match its canonical expectation for {role}")
        else:
            for role in ("a", "b"):
                if first[role]["class"] != fixture["expected_by_target"][role]:
                    raise CampaignError(f"fixture {fixture['name']} has unexpected class for {role}")
        records.append({"fixture": fixture["name"], "stable": replay["stable"], "differential": replay["differential"]})
    summary = {"schema_version": 1, "status": "complete", "pair": pair, "profile": profile, "manifest": str(build.path), "fixtures": records}
    _write_json(verify_root / "summary.json", summary)
    return verify_root / "summary.json"


def verify_manifests(root: Path, manifests: dict[str, Path], profile: str) -> Path:
    from .registry import pair_names

    expected = pair_names()
    if set(manifests) != set(expected):
        raise CampaignError(
            f"manifest map must contain exactly {', '.join(expected)}"
        )
    verify_root = (
        root.resolve()
        / "run/real-targets/verify/all"
        / profile
        / str(time.time_ns())
    )
    verify_root.mkdir(parents=True)
    results = {}
    failures = []
    for pair in expected:
        try:
            summary = verify_pair(root, pair, profile, Path(manifests[pair]))
            results[pair] = {"status": "complete", "summary": str(summary)}
        except (
            CampaignError,
            RuntimeError,
            OSError,
            ValueError,
            KeyError,
            json.JSONDecodeError,
            subprocess.SubprocessError,
        ) as error:
            failures.append(pair)
            results[pair] = {"status": "failed", "error": str(error)}
    aggregate = verify_root / "summary.json"
    _write_json(
        aggregate,
        {
            "schema_version": 1,
            "status": "failed" if failures else "complete",
            "profile": profile,
            "attempted": list(expected),
            "results": results,
        },
    )
    if failures:
        raise CampaignError(
            f"cannot verify {', '.join(failures)}; evidence: {aggregate}"
        )
    return aggregate
