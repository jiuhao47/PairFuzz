import hashlib
import json
from pathlib import Path

from .registry import pair_names
from .registry import pair_spec


class EvidenceError(RuntimeError):
    pass


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _load_evidence(path: Path) -> dict:
    try:
        payload = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise EvidenceError(f"cannot read smoke evidence {path}: {error}") from error
    if payload.get("schema_version") != 1:
        raise EvidenceError(f"unsupported smoke evidence schema in {path}")
    return payload


def _validate_profiles(pair: str, payload: dict) -> None:
    profiles = payload.get("profiles")
    if not isinstance(profiles, dict) or set(profiles) != {"asan", "release"}:
        raise EvidenceError(f"{pair} evidence profiles must be exactly asan and release")
    for name in ("asan", "release"):
        record = profiles[name]
        if record.get("profile", name) != name:
            raise EvidenceError(f"{pair} evidence mixes profiles")
        saved = record.get("solutions_saved")
        replay = record.get("replay_completeness")
        if (
            type(saved) is not int
            or saved < 0
            or not isinstance(replay, dict)
            or replay.get("saved") != saved
            or replay.get("audited") != saved
            or replay.get("three_run_complete") != saved
        ):
            raise EvidenceError(f"{pair} {name} replay evidence is incomplete")
        for key in (
            "wall_seconds",
            "executions",
            "throughput_executions_per_wall_second",
            "build_manifest_sha256",
            "target_sha256",
        ):
            if key not in record:
                raise EvidenceError(f"{pair} {name} evidence is missing {key}")


def write_native_summary(evidence_paths, output: Path) -> Path:
    paths = [Path(path).resolve() for path in evidence_paths]
    payloads = [_load_evidence(path) for path in paths]
    expected = pair_names()
    actual = tuple(payload.get("pair") for payload in payloads)
    if actual != expected:
        raise EvidenceError(
            f"evidence pair order must be {', '.join(expected)}, got {actual}"
        )
    seeds = {payload.get("campaign_seed") for payload in payloads}
    if len(seeds) != 1 or None in seeds:
        raise EvidenceError("all native smoke evidence must use the same seed")
    pairs = []
    for path, payload in zip(paths, payloads):
        pair = payload["pair"]
        _validate_profiles(pair, payload)
        profiles = {}
        for name in ("asan", "release"):
            record = payload["profiles"][name]
            profiles[name] = {
                key: record[key]
                for key in (
                    "wall_seconds",
                    "executions",
                    "throughput_executions_per_wall_second",
                    "solutions_saved",
                    "stable_signatures",
                    "unstable_or_unreproducible",
                    "replay_completeness",
                    "build_manifest_sha256",
                    "target_sha256",
                )
            }
            profiles[name]["logical_cpu"] = record.get("logical_cpu")
        pairs.append(
            {
                "pair": pair,
                "operation": payload.get("operation"),
                "source_versions": payload.get("source_versions", {}),
                "evidence": path.name,
                "evidence_sha256": _sha256(path),
                "profiles": profiles,
            }
        )
    summary = {
        "schema_version": 1,
        "label": "smoke-not-performance-ranking",
        "campaign_seed": seeds.pop(),
        "pairs": pairs,
        "interpretation": (
            "Bounded workflow evidence only; executions per second are not "
            "comparable across operations or library pairs."
        ),
    }
    output = Path(output)
    temporary = output.with_suffix(output.suffix + ".tmp")
    temporary.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    temporary.replace(output)
    return output


def write_smoke_evidence(
    root: Path,
    pair: str,
    campaign_summaries: dict[str, Path],
    output: Path,
    *,
    host: dict,
) -> Path:
    root = Path(root).resolve()
    if set(campaign_summaries) != {"asan", "release"}:
        raise EvidenceError("smoke evidence requires exactly asan and release campaigns")
    lock = json.loads((root / "third_party/real-targets.lock.json").read_text())
    locked = {entry["name"]: entry for entry in lock["sources"]}
    profiles = {}
    seeds = set()
    operations = set()
    source_identities = []
    seed_identities = []
    for profile in ("asan", "release"):
        summary_path = Path(campaign_summaries[profile]).resolve()
        summary = json.loads(summary_path.read_text())
        if (
            summary.get("schema_version") != 1
            or summary.get("status") != "complete"
            or summary.get("pair") != pair
            or summary.get("profile") != profile
        ):
            raise EvidenceError(f"{profile} campaign does not match {pair}")
        campaign_root = summary_path.parent
        campaign = json.loads((campaign_root / "campaign.json").read_text())
        manifest_path = Path(campaign["manifest"]).resolve()
        manifest = json.loads(manifest_path.read_text())
        if (
            _sha256(manifest_path) != summary.get("build_manifest_sha256")
            or manifest.get("pair") != pair
            or manifest.get("profile") != profile
            or summary.get("target_sha256")
            != {role: manifest["targets"][role]["sha256"] for role in ("a", "b")}
        ):
            raise EvidenceError(f"{pair} {profile} mixes build manifests")
        replays = summary.get("replays")
        saved = summary.get("solutions_saved")
        if type(saved) is not int or not isinstance(replays, list) or len(replays) != saved:
            raise EvidenceError(f"{pair} {profile} replay count is incomplete")
        three_run_complete = sum(
            isinstance(record.get("runs"), list) and len(record["runs"]) == 3
            for record in replays
        )
        for record in replays:
            for run in record.get("runs", []):
                for role in ("a", "b"):
                    if run[role].get("class") == "adapter-error":
                        raise EvidenceError(f"{pair} {profile} replay contains adapter-error")
        wall_seconds = summary["wall_seconds"]
        executions = summary["executions"]
        profile_record = {
            "profile": profile,
            "requested_seconds": campaign["seconds"],
            "logical_cpu": campaign["cpu"],
            "wall_seconds": wall_seconds,
            "cpu_seconds": summary["cpu_seconds"],
            "peak_rss_kb": summary["peak_rss_kb"],
            "executions": executions,
            "throughput_executions_per_wall_second": (
                executions / wall_seconds if wall_seconds > 0 else 0.0
            ),
            "solutions_saved": saved,
            "stable_signatures": summary["stable_signatures"],
            "unstable_or_unreproducible": summary["unstable_or_unreproducible"],
            "zero_solutions": summary["zero_solutions"],
            "replay_completeness": {
                "saved": saved,
                "audited": len(replays),
                "three_run_complete": three_run_complete,
            },
            "build_fingerprint": manifest["fingerprint"],
            "build_manifest_sha256": summary["build_manifest_sha256"],
            "target_sha256": summary["target_sha256"],
            "raw_campaign": str(campaign_root.relative_to(root)),
            "raw_build_manifest": str(manifest_path.relative_to(root)),
        }
        profiles[profile] = profile_record
        seeds.add(summary["seed"])
        operations.add(summary["operation"])
        source_identities.append(manifest["source_sha256"])
        seed_identities.append(summary["seed_sha256"])
    if len(seeds) != 1 or len(operations) != 1:
        raise EvidenceError(f"{pair} campaigns mix seed or operation")
    if source_identities[0] != source_identities[1] or seed_identities[0] != seed_identities[1]:
        raise EvidenceError(f"{pair} campaigns mix source or seed manifests")
    dependencies = pair_spec(pair).dependencies
    payload = {
        "schema_version": 1,
        "label": "smoke-not-performance-ranking",
        "recorded_at": "2026-10-28",
        "pair": pair,
        "operation": operations.pop(),
        "semantic_abi_version": 1,
        "result_policy": "exact",
        "signature_version": 2,
        "campaign_seed": seeds.pop(),
        "host": host,
        "source_versions": {
            name: locked[name]["version"] for name in dependencies if name in locked
        },
        "source_sha256": source_identities[0],
        "seed_sha256": seed_identities[0],
        "profiles": profiles,
        "interpretation": (
            "Both bounded runs completed their process lifecycle. This is a "
            "workflow smoke result, not a relative performance, security, or "
            "defect-count claim."
        ),
    }
    output = Path(output)
    temporary = output.with_suffix(output.suffix + ".tmp")
    temporary.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    temporary.replace(output)
    return output
