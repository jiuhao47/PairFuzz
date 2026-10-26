#!/usr/bin/env python3
"""Build, fuzz and replay DiffRay's real JSON parser targets."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "third_party" / "json-libs.json"
DEMO = ROOT / "examples" / "real-json"
BIN = ROOT / "target" / "real-json"


class ManifestError(RuntimeError):
    """The vendored-source manifest is missing, malformed or stale."""


class MetadataError(RuntimeError):
    """A saved differential input has missing or inconsistent metadata."""


def environment() -> dict[str, str]:
    env = os.environ.copy()
    tools = ROOT / ".tools" / "usr" / "lib" / "llvm-18" / "bin"
    if (tools / "clang").exists():
        env["PATH"] = str(tools) + os.pathsep + env["PATH"]
        env.setdefault("LLVM_CONFIG", str(tools / "llvm-config"))
    env["ASAN_OPTIONS"] = "abort_on_error=1:detect_leaks=false"
    env["CARGO_TARGET_DIR"] = str(ROOT / "target")
    for name in list(env):
        if name.startswith(("__AFL_", "__DIFFRAY_")):
            del env[name]
    return env


def verify_vendor_manifest(root: Path) -> None:
    root = root.resolve()
    manifest_path = root / "third_party" / "json-libs.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ManifestError(f"cannot read {manifest_path}: {error}") from error

    if not isinstance(manifest, dict) or manifest.get("schema_version") != 1:
        raise ManifestError("unsupported vendor manifest schema")
    libraries = manifest.get("libraries")
    if not isinstance(libraries, list) or not libraries:
        raise ManifestError("vendor manifest must contain libraries")

    third_party = (root / "third_party").resolve()
    required = {"name", "version", "tag", "commit", "source_url", "files"}
    for library in libraries:
        if not isinstance(library, dict) or not required.issubset(library):
            raise ManifestError("vendor manifest library entry is incomplete")
        files = library["files"]
        if not isinstance(files, dict) or not files:
            raise ManifestError(f"{library['name']} has no vendored files")
        for relative, wanted_hash in files.items():
            if not isinstance(relative, str) or not isinstance(wanted_hash, str):
                raise ManifestError("vendor file paths and hashes must be strings")
            path = (root / relative).resolve()
            try:
                path.relative_to(third_party)
            except ValueError as error:
                raise ManifestError(f"vendor path escapes third_party: {relative}") from error
            if not path.is_file():
                raise ManifestError(f"vendored file is missing: {relative}")
            actual_hash = hashlib.sha256(path.read_bytes()).hexdigest()
            if actual_hash != wanted_hash:
                raise ManifestError(
                    f"checksum mismatch for {relative}: {actual_hash} != {wanted_hash}"
                )


def validate_metadata(path: Path, expected: tuple[int, int]) -> dict:
    direct = Path(f"{path}.metadata")
    hidden = path.parent / f".{path.name}.metadata"
    collision_prefix = f".{path.name}_"
    collision_sidecars = sorted(
        candidate
        for candidate in path.parent.iterdir()
        if candidate.is_file()
        and candidate.name.startswith(collision_prefix)
        and candidate.name.endswith(".metadata")
    )
    candidates = [candidate for candidate in (direct, hidden) if candidate.is_file()]
    candidates.extend(collision_sidecars)
    if not candidates:
        raise MetadataError(f"no metadata sidecar found for {path}")

    errors = []
    for sidecar in candidates:
        try:
            payload = json.loads(sidecar.read_text(encoding="utf-8"))
            records = payload["metadata"]["map"].values()
        except (OSError, json.JSONDecodeError, KeyError, AttributeError) as error:
            errors.append(f"{sidecar.name}: {error}")
            continue
        for entry in records:
            if not isinstance(entry, list) or len(entry) < 2 or not isinstance(entry[1], dict):
                continue
            record = entry[1]
            pair = (record.get("rv_a"), record.get("rv_b"))
            if isinstance(record.get("note"), str) and record["note"] and pair == expected:
                return record
    detail = "; ".join(errors) if errors else "no matching return-value record"
    raise MetadataError(f"metadata for {path} does not match {expected}: {detail}")


def results(data: bytes) -> list[int]:
    pair = []
    for name in ("cjson_target", "mjson_target"):
        binary = BIN / name
        if not binary.is_file():
            raise RuntimeError("real JSON targets are missing; run the build command first")
        completed = subprocess.run(
            [str(binary)],
            input=data,
            capture_output=True,
            timeout=5,
            env=environment(),
            check=False,
        )
        if completed.returncode not in (0, 1, 100):
            raise RuntimeError(
                f"{name} failed: exit={completed.returncode}, stderr={completed.stderr!r}"
            )
        pair.append(completed.returncode)
    return pair


def verify() -> None:
    verify_vendor_manifest(ROOT)
    cases = [
        (b'{"x":1}', [0, 0]),
        (b'  {"x":[true,null]}\n', [0, 0]),
        (b'[]', [1, 1]),
        (b'null', [1, 1]),
        (b'{"x":1}junk', [1, 1]),
        (b'{"x":1}\x00', [1, 1]),
        (b'', [100, 100]),
    ]
    for data, expected in cases:
        actual = results(data)
        if actual != expected:
            raise RuntimeError(f"input {data[:80]!r}: expected {expected}, got {actual}")

    seeds = sorted((DEMO / "seeds").iterdir()) + sorted(
        (DEMO / "campaign-seeds").iterdir()
    )
    for seed in seeds:
        left, right = results(seed.read_bytes())
        if left != right:
            raise RuntimeError(f"initial seed is already differential: {seed}")

    known = DEMO / "known-difference.json"
    actual = results(known.read_bytes())
    if actual != [1, 0]:
        raise RuntimeError(f"known parser difference: expected [1, 0], got {actual}")
    print(
        f"PASS: {len(cases)} contract cases, {len(seeds)} matching seeds, "
        "known real parser difference [1, 0]"
    )


def build() -> None:
    env = environment()
    if not (ROOT / "LibAFL" / "crates" / "libafl" / "Cargo.toml").is_file():
        raise RuntimeError("initialize the pinned dependency: git submodule update --init LibAFL")
    commands = (
        ["cargo", "build", "--locked", "--bin", "diffray", "-j", "4"],
        [
            "cargo",
            "build",
            "--locked",
            "--manifest-path",
            "app/compiler/Cargo.toml",
            "--target-dir",
            str(ROOT / "target"),
            "--lib",
            "--bin",
            "libafl_forkserver_cc",
            "-j",
            "4",
        ],
        ["make", "-C", str(DEMO)],
    )
    for command in commands:
        subprocess.run(command, cwd=ROOT, env=env, check=True)
    verify()


def positive_seconds(value: str) -> float:
    seconds = float(value)
    if not math.isfinite(seconds) or seconds <= 0:
        raise argparse.ArgumentTypeError("duration must be a finite positive number")
    return seconds


def stop_group(process) -> None:
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def wait_for_campaign(process, seconds: float) -> None:
    try:
        status = process.wait(timeout=seconds)
    except subprocess.TimeoutExpired:
        return
    raise RuntimeError(f"fuzzer exited early ({status})")


def replay(path: Path) -> None:
    data = path.read_bytes()
    pair = results(data)
    print(f"Input: {path}\nBytes: {data[:120]!r}\ncJSON={pair[0]}, mjson={pair[1]}")
    if pair[0] == pair[1]:
        raise RuntimeError("input does not reproduce a parser difference")
    print("PASS: real parser difference reproduced")


def _version_summary() -> list[dict[str, str]]:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    return [
        {
            "name": library["name"],
            "version": library["version"],
            "commit": library["commit"],
        }
        for library in manifest["libraries"]
    ]


def run(seconds: float, mode: str) -> Path:
    verify()
    fuzzer = ROOT / "target" / "debug" / "diffray"
    if not fuzzer.is_file():
        raise RuntimeError("DiffRay is missing; run the build command first")

    parent = ROOT / "run" / "real-json"
    parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="campaign-", dir=parent))
    log = directory / "fuzzer.log"
    command = [
        str(fuzzer),
        "-v",
        "--mode",
        mode,
        "-i",
        str(DEMO / "campaign-seeds"),
        "-o",
        str(directory / "diffs"),
        "-l",
        str(directory / "log"),
        "--prog-a",
        str(BIN / "cjson_target"),
        "--prog-b",
        str(BIN / "mjson_target"),
        "--persistent-mode",
        "disabled",
        "--simple-monitor",
        "enabled",
        "--json-heartbeat-secs",
        "1",
    ]
    print(f"Fuzzing real parsers for {seconds:g}s; output: {directory}", flush=True)
    with log.open("wb") as output:
        process = subprocess.Popen(
            command,
            cwd=directory,
            env=environment(),
            stdout=output,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        try:
            wait_for_campaign(process, seconds)
        finally:
            stop_group(process)

    saved = []
    differences = directory / "diffs"
    candidates = sorted(differences.iterdir()) if differences.exists() else []
    for path in candidates:
        if not path.is_file() or path.name.startswith(".") or path.suffix == ".metadata":
            continue
        pair = results(path.read_bytes())
        if pair[0] == pair[1]:
            continue
        validate_metadata(path, (pair[0], pair[1]))
        saved.append(path)

    report = {
        "libraries": _version_summary(),
        "mode": mode,
        "duration_seconds": seconds,
        "reproduced_differences": len(saved),
        "sample": str(saved[0].relative_to(ROOT)) if saved else None,
        "command": command,
    }
    summary = directory / "summary.json"
    summary.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    if not saved:
        raise RuntimeError(f"no reproduced difference saved; inspect {log}")
    replay(saved[0])
    print(f"Reproduced saved inputs: {len(saved)}\nReport: {summary}")
    return directory


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("verify-vendor", help="verify pinned source provenance")
    commands.add_parser("build", help="build DiffRay and the real parser targets")
    commands.add_parser("verify", help="check the shared target contract")
    replay_parser = commands.add_parser("replay", help="replay one differential input")
    replay_parser.add_argument("input", type=Path)
    run_parser = commands.add_parser("run", help="run a bounded real fuzzing campaign")
    run_parser.add_argument("--seconds", type=positive_seconds, default=30.0)
    run_parser.add_argument(
        "--mode",
        choices=(
            "single-cov",
            "single-rv",
            "single-pl",
            "single-rv-cov",
            "combo-cov",
            "combo-rv",
            "combo-pl",
        ),
        default="single-cov",
    )
    args = parser.parse_args()
    if args.command == "verify-vendor":
        verify_vendor_manifest(ROOT)
        print("PASS: vendored cJSON and mjson sources match the pinned manifest")
    elif args.command == "build":
        build()
    elif args.command == "verify":
        verify()
    elif args.command == "replay":
        replay(args.input)
    elif args.command == "run":
        run(args.seconds, args.mode)


if __name__ == "__main__":
    try:
        main()
    except (ManifestError, MetadataError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        sys.exit(1)
