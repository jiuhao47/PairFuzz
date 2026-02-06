#!/usr/bin/env python3
"""Batch runner for replay_program over LocateFix by_program data.

Walks LocateFix/data/collected/by_program/<program>/<group>/rep_*/diffs and runs
replay_program for each input file. Results are summarized into a per-rep JSON.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, Iterator, List, Optional


_SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(_SCRIPT_DIR))

import run_replay_wrapper as rr  # noqa: E402


DEFAULT_BASE_DIR = Path("LocateFix/data/collected/by_program")
DEFAULT_SUMMARY_NAME = "replay_summary.json"

PROGRAM_TO_REPLAY_KEY = {
    "png": "png-libpng",
    "spng": "png-libspng",
    "cjson": "json-cjson",
    "mjson": "json-mjson",
    "zlib": "zlib-zlib",
    "libdeflate": "zlib-libdeflate",
    "xml2": "xml-xml2",
    "expat": "xml-expat",
    "gm": "bmp-GM",
    "im": "bmp-IM",
    "xz": "xz-xz",
    "archive": "tar-archive",
}


@dataclass(frozen=True)
class RepTask:
    program: str
    group: str
    rep_dir: Path
    diffs_dir: Path
    replay_key: str
    summary_path: Path


def _split_csv(value: Optional[str]) -> Optional[set[str]]:
    if value is None:
        return None
    parts = [p.strip() for p in value.split(",")]
    parts = [p for p in parts if p]
    return set(parts) if parts else None


def _is_valid_summary_json(path: Path) -> bool:
    try:
        with path.open("r", encoding="utf-8") as f:
            data = json.load(f)
    except Exception:
        return False

    if not isinstance(data, dict):
        return False

    if "counts" not in data or "program" not in data:
        return False

    counts = data.get("counts")
    if not isinstance(counts, dict):
        return False

    for key in ("total", "yes", "no", "parse_failed", "timeout"):
        if key not in counts:
            return False

    return True


def _iter_input_files(
    diffs_dir: Path, recursive: bool, include_hidden: bool
) -> Iterator[Path]:
    if not recursive:
        for entry in sorted(diffs_dir.iterdir(), key=lambda p: p.name):
            if not include_hidden and entry.name.startswith("."):
                continue
            if entry.is_file():
                yield entry
        return

    for root, dirnames, filenames in os.walk(diffs_dir):
        if not include_hidden:
            dirnames[:] = [d for d in dirnames if not d.startswith(".")]
        for name in sorted(filenames):
            if not include_hidden and name.startswith("."):
                continue
            path = Path(root) / name
            if path.is_file():
                yield path


def iter_rep_tasks(
    base_dir: Path,
    summary_name: str,
    allowed_programs: Optional[set[str]],
    allowed_groups: Optional[set[str]],
) -> Iterable[RepTask]:
    if not base_dir.exists():
        raise FileNotFoundError(f"base dir not found: {base_dir}")

    for program_dir in sorted(
        [p for p in base_dir.iterdir() if p.is_dir()], key=lambda p: p.name
    ):
        program = program_dir.name
        if allowed_programs is not None and program not in allowed_programs:
            continue

        replay_key = PROGRAM_TO_REPLAY_KEY.get(program)

        for group_dir in sorted(
            [p for p in program_dir.iterdir() if p.is_dir()], key=lambda p: p.name
        ):
            group = group_dir.name
            if allowed_groups is not None and group not in allowed_groups:
                continue

            for rep_dir in sorted(
                [
                    p
                    for p in group_dir.iterdir()
                    if p.is_dir() and p.name.startswith("rep_")
                ],
                key=lambda p: p.name,
            ):
                diffs_dir = rep_dir / "diffs"
                summary_path = rep_dir / summary_name
                if replay_key is None:
                    yield RepTask(
                        program=program,
                        group=group,
                        rep_dir=rep_dir,
                        diffs_dir=diffs_dir,
                        replay_key="",
                        summary_path=summary_path,
                    )
                else:
                    yield RepTask(
                        program=program,
                        group=group,
                        rep_dir=rep_dir,
                        diffs_dir=diffs_dir,
                        replay_key=replay_key,
                        summary_path=summary_path,
                    )


def _positive_float(s: str) -> float:
    try:
        v = float(s)
    except ValueError as e:
        raise argparse.ArgumentTypeError("must be a number") from e
    if v <= 0:
        raise argparse.ArgumentTypeError("must be > 0")
    return v


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        prog="batch_replay_by_program",
        description="Run replay_program for inputs under LocateFix/data/collected/by_program",
    )
    parser.add_argument(
        "--base-dir",
        default=str(DEFAULT_BASE_DIR),
        help="Base directory containing by_program/*",
    )
    parser.add_argument(
        "--summary-name",
        default=DEFAULT_SUMMARY_NAME,
        help="Summary filename under each rep dir",
    )
    parser.add_argument(
        "--programs",
        default=None,
        help="Comma-separated program filters (e.g. png,mjson,xz)",
    )
    parser.add_argument(
        "--groups",
        default=None,
        help="Comma-separated group filters (e.g. single-cov,combo-pl)",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Force re-run even if summary exists",
    )
    parser.add_argument(
        "--recursive",
        action="store_true",
        help="Recursively traverse diffs dirs",
    )
    parser.add_argument(
        "--include-hidden",
        action="store_true",
        help="Include hidden files",
    )
    parser.add_argument(
        "--replay-bin",
        default=None,
        help="Path to replay_program binary",
    )
    parser.add_argument(
        "--timeout",
        type=_positive_float,
        default=None,
        help="Timeout (seconds) for replay_program",
    )
    parser.add_argument(
        "--env",
        action="append",
        default=[],
        help="Extra environment variables like KEY=VALUE (repeatable)",
    )
    parser.add_argument(
        "--show-output",
        action="store_true",
        help="Print replay output to stderr on every run",
    )

    args = parser.parse_args(argv)

    repo_root = rr._repo_root_from_here()
    base_dir = (repo_root / args.base_dir).resolve()
    summary_name = str(args.summary_name)
    allowed_programs = _split_csv(args.programs)
    allowed_groups = _split_csv(args.groups)
    force = bool(args.force)

    configs = rr._build_configs(repo_root)

    replay_bin = (
        Path(args.replay_bin).resolve()
        if args.replay_bin
        else (repo_root / "app" / "bins" / ".bin" / "replay" / "replay_program")
    )
    if not replay_bin.is_file():
        raise FileNotFoundError(f"replay_program not found: {replay_bin}")

    env_overrides: Dict[str, str] = {}
    for item in args.env:
        if "=" not in item:
            raise ValueError(f"invalid --env value (expected KEY=VALUE): {item}")
        k, v = item.split("=", 1)
        env_overrides[k] = v

    total_reps = 0
    ok_reps = 0
    skipped_done = 0
    skipped_empty = 0
    skipped_unknown = 0
    failed = 0

    failures: List[Dict[str, str]] = []

    for task in iter_rep_tasks(
        base_dir=base_dir,
        summary_name=summary_name,
        allowed_programs=allowed_programs,
        allowed_groups=allowed_groups,
    ):
        total_reps += 1

        if not task.replay_key:
            skipped_unknown += 1
            failures.append(
                {
                    "rep": str(task.rep_dir),
                    "reason": f"unknown program mapping: {task.program}",
                }
            )
            print(
                f"[SKIP] {task.program}/{task.group}/{task.rep_dir.name}: no replay mapping",
                file=sys.stderr,
            )
            continue

        cfg = configs.get(task.replay_key)
        if cfg is None:
            skipped_unknown += 1
            failures.append(
                {
                    "rep": str(task.rep_dir),
                    "reason": f"missing replay config: {task.replay_key}",
                }
            )
            print(
                f"[SKIP] {task.program}/{task.group}/{task.rep_dir.name}: missing config {task.replay_key}",
                file=sys.stderr,
            )
            continue

        if not task.diffs_dir.exists() or not task.diffs_dir.is_dir():
            failed += 1
            failures.append(
                {
                    "rep": str(task.rep_dir),
                    "reason": f"missing diffs dir: {task.diffs_dir}",
                }
            )
            print(
                f"[FAIL] {task.program}/{task.group}/{task.rep_dir.name}: diffs dir missing",
                file=sys.stderr,
            )
            continue

        if task.summary_path.exists() and not force:
            if _is_valid_summary_json(task.summary_path):
                skipped_done += 1
                print(
                    f"[SKIP] {task.program}/{task.group}/{task.rep_dir.name}: summary exists ({task.summary_path})",
                    file=sys.stderr,
                )
                continue
            failed += 1
            failures.append(
                {
                    "rep": str(task.rep_dir),
                    "reason": f"bad summary.json: {task.summary_path}",
                }
            )
            print(
                f"[FAIL-SKIP] {task.program}/{task.group}/{task.rep_dir.name}: bad summary.json",
                file=sys.stderr,
            )
            continue

        inputs = list(
            _iter_input_files(
                task.diffs_dir,
                recursive=bool(args.recursive),
                include_hidden=bool(args.include_hidden),
            )
        )
        if not inputs:
            skipped_empty += 1
            print(
                f"[SKIP] {task.program}/{task.group}/{task.rep_dir.name}: diffs empty",
                file=sys.stderr,
            )
            continue

        counts = {"total": 0, "yes": 0, "no": 0, "parse_failed": 0, "timeout": 0}
        start = time.time()

        print(
            f"[RUN] {task.program}/{task.group}/{task.rep_dir.name}: key={task.replay_key} inputs={len(inputs)}",
            file=sys.stderr,
        )

        for input_path in inputs:
            counts["total"] += 1
            try:
                cp = rr._run_replay(
                    replay_bin=replay_bin,
                    cfg=cfg,
                    input_path=input_path,
                    env_overrides=env_overrides,
                    timeout_s=args.timeout,
                )
            except Exception:
                counts["timeout"] += 1
                continue

            if args.show_output:
                print(cp.stdout, file=sys.stderr)

            diff = rr._parse_diff(cp.stdout)
            if diff is None:
                counts["parse_failed"] += 1
            elif diff == "Yes":
                counts["yes"] += 1
            else:
                counts["no"] += 1

        elapsed_s = time.time() - start
        summary = {
            "program": task.program,
            "group": task.group,
            "rep": task.rep_dir.name,
            "replay_key": task.replay_key,
            "counts": counts,
            "elapsed_s": elapsed_s,
        }
        with task.summary_path.open("w", encoding="utf-8") as f:
            json.dump(summary, f, ensure_ascii=False, indent=2)

        ok_reps += 1

    print("\n=== batch_replay_by_program summary ===", file=sys.stderr)
    print(f"total_reps: {total_reps}", file=sys.stderr)
    print(f"ok_reps: {ok_reps}", file=sys.stderr)
    print(f"skipped_done: {skipped_done}", file=sys.stderr)
    print(f"skipped_empty: {skipped_empty}", file=sys.stderr)
    print(f"skipped_unknown: {skipped_unknown}", file=sys.stderr)
    print(f"failed: {failed}", file=sys.stderr)

    if failures:
        print("\nFailures:", file=sys.stderr)
        for item in failures[:200]:
            print(
                f"- rep={item.get('rep')} reason={item.get('reason')}",
                file=sys.stderr,
            )
        if len(failures) > 200:
            print(f"  ... and {len(failures) - 200} more", file=sys.stderr)

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
