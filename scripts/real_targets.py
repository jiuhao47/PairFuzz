#!/usr/bin/env python3
import argparse
import json
from pathlib import Path

from real_targets_lib.builds import build_pair
from real_targets_lib.campaigns import replay_pair, run_pair, verify_manifests, verify_pair
from real_targets_lib.registry import pair_names, pair_spec
from real_targets_lib.sources import fetch_source, load_lock, source_entries


ROOT = Path(__file__).resolve().parents[1]
LOCK_PATH = ROOT / "third_party" / "real-targets.lock.json"
SOURCE_CACHE = ROOT / "run" / "real-targets" / "sources"


def _selected_sources(pair: str):
    if pair == "all":
        lock = load_lock(LOCK_PATH)
        return lock, [entry["name"] for entry in lock["sources"]]
    lock = load_lock(LOCK_PATH)
    return lock, list(pair_spec(pair).dependencies)


def command_fetch(args):
    lock, names = _selected_sources(args.pair)
    entries = source_entries(lock)
    missing = sorted(set(names) - set(entries))
    if missing:
        raise RuntimeError(f"source lock is missing dependencies: {', '.join(missing)}")
    results = {}
    for name in names:
        results[name] = str(fetch_source(entries[name], SOURCE_CACHE).relative_to(ROOT))
    print(json.dumps(results, indent=2, sort_keys=True))


def command_build(args):
    if args.pair != "all":
        print(build_pair(ROOT, args.pair, args.profile))
        return
    pairs = pair_names() if args.pair == "all" else (args.pair,)
    results = {}
    for pair in pairs:
        try:
            results[pair] = str(build_pair(ROOT, pair, args.profile))
        except RuntimeError as error:
            raise RuntimeError(f"cannot build {pair}: {error}") from error
    print(json.dumps(results, indent=2, sort_keys=True))


def command_verify(args):
    if args.pair == "all":
        if args.manifest_map is None:
            raise RuntimeError("verify --pair all requires --manifest-map and never builds implicitly")
        try:
            payload = json.loads(args.manifest_map.read_text())
        except (OSError, json.JSONDecodeError) as error:
            raise RuntimeError(f"cannot read manifest map {args.manifest_map}: {error}") from error
        if not isinstance(payload, dict):
            raise RuntimeError("manifest map must be a JSON object")
        manifests = {
            pair: Path(payload[pair])
            for pair in pair_names()
            if pair in payload
        }
        print(verify_manifests(ROOT, manifests, args.profile))
        return
    pairs = pair_names() if args.pair == "all" else (args.pair,)
    results = {}
    for pair in pairs:
        try:
            results[pair] = str(verify_pair(ROOT, pair, args.profile))
        except RuntimeError as error:
            raise RuntimeError(f"cannot verify {pair}: {error}") from error
    print(json.dumps(results, indent=2, sort_keys=True))


def command_run(args):
    manifest = build_pair(ROOT, args.pair, args.profile)
    print(run_pair(ROOT, args.pair, manifest, args.seconds, args.cpu, args.seed))


def command_replay(args):
    from real_targets_lib.campaigns import load_build_manifest

    load_build_manifest(args.build_manifest, expected_pair=args.pair)
    output = ROOT / "run" / "real-targets" / "replay" / args.pair / str(__import__("time").time_ns())
    result = replay_pair(args.build_manifest, args.input, output)
    print(json.dumps(result, indent=2, sort_keys=True))


def parser():
    result = argparse.ArgumentParser(description="DiffRay reproducible native-target tooling")
    commands = result.add_subparsers(dest="command", required=True)
    fetch = commands.add_parser("fetch", help="download, verify, and extract locked sources")
    fetch.add_argument("--pair", choices=(*pair_names(), "all"), required=True)
    fetch.set_defaults(handler=command_fetch)
    build = commands.add_parser("build", help="build one target pair from locked sources")
    build.add_argument("--pair", choices=(*pair_names(), "all"), required=True)
    build.add_argument("--profile", choices=("asan", "release"), required=True)
    build.set_defaults(handler=command_build)
    verify = commands.add_parser("verify", help="replay the locked contract fixture suite")
    verify.add_argument("--pair", choices=(*pair_names(), "all"), required=True)
    verify.add_argument("--profile", choices=("asan", "release"), required=True)
    verify.add_argument("--manifest-map", type=Path)
    verify.set_defaults(handler=command_verify)
    run = commands.add_parser("run", help="run one bounded audited campaign")
    run.add_argument("--pair", choices=pair_names(), required=True)
    run.add_argument("--profile", choices=("asan", "release"), required=True)
    run.add_argument("--seconds", type=float, required=True)
    run.add_argument("--cpu", type=int, required=True)
    run.add_argument("--seed", type=int, required=True)
    run.set_defaults(handler=command_run)
    replay = commands.add_parser("replay", help="audit one input with manifest-selected targets")
    replay.add_argument("--pair", choices=pair_names(), required=True)
    replay.add_argument("--build-manifest", type=Path, required=True)
    replay.add_argument("--input", type=Path, required=True)
    replay.set_defaults(handler=command_replay)
    return result


def main():
    arguments = parser().parse_args()
    arguments.handler(arguments)


if __name__ == "__main__":
    main()
