#!/usr/bin/env python3
"""Build, fuzz and replay DiffRay's real JSON parser targets."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "third_party" / "json-libs.json"
DEMO = ROOT / "examples" / "real-json"
BIN = ROOT / "target" / "real-json"


class ManifestError(RuntimeError):
    """The vendored-source manifest is missing, malformed or stale."""


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

    seeds = sorted((DEMO / "seeds").iterdir())
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("verify-vendor", help="verify pinned source provenance")
    commands.add_parser("build", help="build DiffRay and the real parser targets")
    commands.add_parser("verify", help="check the shared target contract")
    args = parser.parse_args()
    if args.command == "verify-vendor":
        verify_vendor_manifest(ROOT)
        print("PASS: vendored cJSON and mjson sources match the pinned manifest")
    elif args.command == "build":
        build()
    elif args.command == "verify":
        verify()


if __name__ == "__main__":
    try:
        main()
    except (ManifestError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        sys.exit(1)
