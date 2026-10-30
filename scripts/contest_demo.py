#!/usr/bin/env python3
"""Build, exercise and replay the classic PairFuzz competition demonstration."""

import argparse
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
DEMO = ROOT / "examples" / "contest"
BIN = ROOT / "target" / "contest"


def environment():
    env = os.environ.copy()
    tools = ROOT / ".tools" / "usr" / "lib" / "llvm-18" / "bin"
    if (tools / "clang").exists():
        env["PATH"] = str(tools) + os.pathsep + env["PATH"]
        env.setdefault("LLVM_CONFIG", str(tools / "llvm-config"))
    env["ASAN_OPTIONS"] = "abort_on_error=1:detect_leaks=false"
    env["CARGO_TARGET_DIR"] = str(ROOT / "target")
    # Replays must never attach to a caller's active fuzzing session.
    for name in list(env):
        if name.startswith(("__AFL_", "__PAIRFUZZ_")):
            del env[name]
    return env


def results(data):
    pair = []
    for name in ("demo_ref", "demo_alt"):
        binary = BIN / name
        if not binary.is_file():
            raise RuntimeError("Demo targets missing; run the build command first")
        result = subprocess.run(
            [str(binary)], input=data, capture_output=True,
            timeout=5, env=environment(), check=False,
        )
        if result.returncode not in (0, 1):
            raise RuntimeError(f"{name} failed: exit={result.returncode}, stderr={result.stderr!r}")
        pair.append(result.returncode)
    return pair


def verify():
    cases = [
        (b"10\n", [0, 0]),
        (b"08\n", [0, 0]),
        (b"0x10\n", [0, 1]),
        (b"0Xff\n", [0, 1]),
        (b"0w10\n", [1, 1]),
        (b"", [1, 1]),
        (b"0x\n", [1, 1]),
        (b"12junk\n", [1, 1]),
        (b"1\x002", [1, 1]),
        (b"9" * 10000, [1, 1]),
    ]
    for data, expected in cases:
        actual = results(data)
        if actual != expected:
            raise RuntimeError(f"Input {data[:40]!r}: expected {expected}, got {actual}")
    for seed in sorted((DEMO / "seeds").iterdir()):
        actual = results(seed.read_bytes())
        if actual[0] != actual[1]:
            raise RuntimeError(f"Initial seed already differs: {seed}")
    print(f"PASS: {len(cases)} target cases; all starting seeds agree")


def build():
    env = environment()
    if not (ROOT / "LibAFL" / "crates" / "libafl" / "Cargo.toml").exists():
        raise RuntimeError("Initialize the pinned dependency: git submodule update --init LibAFL")
    for command in (
        ["cargo", "build", "--locked", "--bin", "pairfuzz", "-j", "4"],
        ["cargo", "build", "--locked", "--manifest-path", "app/compiler/Cargo.toml",
         "--target-dir", str(ROOT / "target"), "--lib", "--bin", "libafl_forkserver_cc", "-j", "4"],
        ["make", "-C", str(DEMO)],
    ):
        subprocess.run(command, cwd=ROOT, env=env, check=True)
    verify()


def replay(path):
    data = path.read_bytes()
    actual = results(data)
    print(f"Input: {path}\nBytes: {data[:80]!r}\nref={actual[0]}, alt={actual[1]}")
    if actual != [0, 1]:
        raise RuntimeError("Input does not reproduce the demo's acceptance difference")
    print("PASS: acceptance difference reproduced")


def stop_group(process):
    # Stop the restarter group; forkservers exit when their control pipes close.
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        pass
    finally:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def run(seconds, mode):
    verify()
    fuzzer = ROOT / "target" / "debug" / "pairfuzz"
    if not fuzzer.is_file():
        raise RuntimeError("Fuzzer missing; run the build command first")
    parent = ROOT / "run" / "contest"
    parent.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="demo-", dir=parent))
    log = directory / "fuzzer.log"
    command = [
        str(fuzzer), "-v", "--mode", mode,
        "-i", str(DEMO / "seeds"), "-o", str(directory / "diffs"),
        "-l", str(directory / "log"), "--prog-a", str(BIN / "demo_ref"),
        "--prog-b", str(BIN / "demo_alt"), "--persistent-mode", "disabled",
        "--simple-monitor", "enabled", "--json-heartbeat-secs", "1",
    ]
    print(f"Fuzzing for {seconds:g}s; output: {directory}", flush=True)
    with log.open("wb") as output:
        process = subprocess.Popen(command, cwd=directory, env=environment(),
                                   stdout=output, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        try:
            try:
                status = process.wait(timeout=seconds)
            except subprocess.TimeoutExpired:
                status = None
        finally:
            stop_group(process)
    if status is not None:
        raise RuntimeError(f"Fuzzer exited early ({status}); inspect {log}")

    saved = []
    differences = directory / "diffs"
    for path in sorted(differences.iterdir()) if differences.exists() else []:
        if path.is_file() and not path.name.startswith(".") and path.suffix != ".metadata":
            if results(path.read_bytes()) == [0, 1]:
                saved.append(path)
    report = {"mode": mode, "duration_seconds": seconds,
              "reproduced_differences": len(saved),
              "sample": str(saved[0].relative_to(ROOT)) if saved else None,
              "command": command}
    (directory / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    if not saved:
        raise RuntimeError(f"No reproduced difference saved in this random run; inspect {log} or retry longer")
    replay(saved[0])
    print(f"Reproduced saved inputs: {len(saved)}\nReport: {directory / 'summary.json'}")


def positive_seconds(value):
    value = float(value)
    if not math.isfinite(value) or value <= 0:
        raise argparse.ArgumentTypeError("duration must be a finite positive number")
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("build", help="Build the fuzzer, existing runtime and demo pair")
    commands.add_parser("verify", help="Check target behavior and matching seeds")
    replay_parser = commands.add_parser("replay", help="Reproduce a saved differential input")
    replay_parser.add_argument("input", type=Path)
    run_parser = commands.add_parser("run", help="Run a bounded real fuzzing session")
    run_parser.add_argument("--seconds", type=positive_seconds, default=30)
    run_parser.add_argument("--mode", choices=("single-cov", "single-rv", "single-pl",
                                             "single-rv-cov", "combo-cov", "combo-rv", "combo-pl"),
                            default="single-cov")
    args = parser.parse_args()
    if args.command == "build":
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
    except (RuntimeError, OSError, subprocess.SubprocessError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        sys.exit(130)
