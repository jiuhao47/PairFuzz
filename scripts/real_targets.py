#!/usr/bin/env python3
import argparse
import json
from pathlib import Path

from real_targets_lib.builds import build_pair
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
    manifest = build_pair(ROOT, args.pair, args.profile)
    print(manifest)


def parser():
    result = argparse.ArgumentParser(description="DiffRay reproducible native-target tooling")
    commands = result.add_subparsers(dest="command", required=True)
    fetch = commands.add_parser("fetch", help="download, verify, and extract locked sources")
    fetch.add_argument("--pair", choices=(*pair_names(), "all"), required=True)
    fetch.set_defaults(handler=command_fetch)
    build = commands.add_parser("build", help="build one target pair from locked sources")
    build.add_argument("--pair", choices=pair_names(), required=True)
    build.add_argument("--profile", choices=("asan", "release"), required=True)
    build.set_defaults(handler=command_build)
    return result


def main():
    arguments = parser().parse_args()
    arguments.handler(arguments)


if __name__ == "__main__":
    main()
