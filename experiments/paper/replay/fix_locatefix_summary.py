#!/usr/bin/env python3
"""Fix locatefix_summary.json based on replay_summary.json results."""

from __future__ import annotations

import argparse
import json
import shutil
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, List


def _load_json(path: Path) -> Dict[str, Any]:
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)


def _dump_json(path: Path, data: Dict[str, Any]) -> None:
    with path.open("w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
        f.write("\n")


def _collect_no_inputs(replay: Dict[str, Any]) -> set[str]:
    inputs = replay.get("inputs", [])
    no_inputs: set[str] = set()
    for item in inputs:
        if item.get("result") == "No":
            path = item.get("path")
            if isinstance(path, str) and path:
                no_inputs.add(path)
    return no_inputs


def _move_to_failures(summary: Dict[str, Any], no_inputs: set[str]) -> int:
    moved = 0

    failures: List[Dict[str, Any]] = summary.get("failures", [])
    if not isinstance(failures, list):
        failures = []
    failure_map = {
        item.get("input"): item for item in failures if isinstance(item, dict)
    }

    def move_from(section: str) -> List[Dict[str, Any]]:
        nonlocal moved
        items: List[Dict[str, Any]] = summary.get(section, [])
        if not isinstance(items, list):
            return []
        kept: List[Dict[str, Any]] = []
        for item in items:
            if not isinstance(item, dict):
                kept.append(item)
                continue
            input_path = item.get("input")
            if isinstance(input_path, str) and input_path in no_inputs:
                existing = failure_map.get(input_path)
                if existing is None:
                    new_item = dict(item)
                    new_item["status"] = "failure"
                    failure_map[input_path] = new_item
                else:
                    existing["status"] = "failure"
                moved += 1
            else:
                kept.append(item)
        return kept

    summary["successes"] = move_from("successes")
    summary["bugs"] = move_from("bugs")
    summary["filtered"] = move_from("filtered")

    summary["failures"] = list(failure_map.values())
    return moved


def _recompute_counts(summary: Dict[str, Any]) -> None:
    successes = summary.get("successes", [])
    filtered = summary.get("filtered", [])
    failures = summary.get("failures", [])

    def _len(value: Any) -> int:
        return len(value) if isinstance(value, list) else 0

    counts = summary.get("counts", {})
    if not isinstance(counts, dict):
        counts = {}

    counts["success_inputs"] = _len(successes)
    counts["unique_bugs"] = _count_unique_bug_ids(successes)
    counts["filtered_inputs"] = _len(filtered)
    counts["failure_inputs"] = _len(failures)
    counts["total_inputs"] = (
        counts["success_inputs"] + counts["filtered_inputs"] + counts["failure_inputs"]
    )

    summary["counts"] = counts


def _count_unique_bug_ids(successes: Any) -> int:
    if not isinstance(successes, list):
        return 0
    bug_ids = set()
    for item in successes:
        if not isinstance(item, dict):
            continue
        bug_id = item.get("bug_id")
        if bug_id is not None:
            bug_ids.add(bug_id)
    return len(bug_ids)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Fix locatefix_summary.json using replay_summary.json"
    )
    parser.add_argument("--replay", required=True, help="Path to replay_summary.json")
    parser.add_argument(
        "--locatefix", required=True, help="Path to locatefix_summary.json"
    )
    parser.add_argument(
        "--backup-suffix",
        default=datetime.now().strftime(".bak.%Y%m%d%H%M%S"),
        help="Suffix for backup file (default: timestamped)",
    )

    args = parser.parse_args()

    replay_path = Path(args.replay)
    locatefix_path = Path(args.locatefix)

    replay = _load_json(replay_path)
    summary = _load_json(locatefix_path)

    no_inputs = _collect_no_inputs(replay)

    backup_path = locatefix_path.with_name(locatefix_path.name + args.backup_suffix)
    shutil.copy2(locatefix_path, backup_path)

    moved = _move_to_failures(summary, no_inputs)
    _recompute_counts(summary)
    _dump_json(locatefix_path, summary)

    print(f"Backup written: {backup_path}")
    print(f"No-inputs in replay: {len(no_inputs)}")
    print(f"Moved to failures: {moved}")
    print(f"Updated: {locatefix_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
