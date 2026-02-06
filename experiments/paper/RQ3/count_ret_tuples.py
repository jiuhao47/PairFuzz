#!/usr/bin/env python3
"""Count distinct (ret_a, ret_b) tuples from two executables over a directory of inputs.

This script is designed to drive two external programs that print a line like:
  [RV_FLAG] ret = 18511
or:
  ret = 18511

You provide two command templates with placeholders:
  - {input}: input file path
  - {mode}: mode integer (e.g. 0/1/2)

Example:
  python3 count_ret_tuples.py \
    --cmd-a './IM/test_IM {input} {mode}' \
    --cmd-b './IM/test_GM {input} {mode}' \
    --inputs ./seeds

The script runs both commands for each input and mode, extracts the last "ret = <int>"
from their combined stdout+stderr, and counts unique return-value tuples per mode.

Notes:
  - By default, files starting with '.' are skipped.
  - Non-regular files are skipped.
  - Exit codes are not used to compute the tuple; output parsing is.
"""

from __future__ import annotations

import argparse
import collections
import concurrent.futures
import json
import os
import re
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass
from typing import Dict, Iterable, Iterator, List, Optional, Sequence, Tuple


_RET_RE = re.compile(r"\bret\s*=\s*(-?\d+)\b")


@dataclass(frozen=True)
class CmdResult:
    ret: Optional[int]
    exit_code: Optional[int]
    timed_out: bool


def _parse_last_ret(output: str) -> Optional[int]:
    last: Optional[int] = None
    for m in _RET_RE.finditer(output):
        last = int(m.group(1))
    return last


def _format_cmd(template_parts: Sequence[str], input_path: str, mode: int) -> List[str]:
    formatted: List[str] = []
    for p in template_parts:
        formatted.append(p.format(input=input_path, mode=mode))
    return formatted


def _run_cmd(
    template_parts: Sequence[str],
    input_path: str,
    mode: int,
    cwd: Optional[str],
    timeout_s: Optional[float],
) -> CmdResult:
    cmd = _format_cmd(template_parts, input_path, mode)
    try:
        cp = subprocess.run(
            cmd,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            errors="replace",
            timeout=timeout_s,
        )
        ret = _parse_last_ret(cp.stdout)
        return CmdResult(ret=ret, exit_code=cp.returncode, timed_out=False)
    except subprocess.TimeoutExpired as e:
        out = (e.stdout or "") + (e.stderr or "")
        ret = _parse_last_ret(out)
        return CmdResult(ret=ret, exit_code=None, timed_out=True)


def _iter_inputs(
    inputs_dir: str,
    recursive: bool,
    include_hidden: bool,
    exclude_names: frozenset[str],
) -> Iterator[str]:
    if not recursive:
        with os.scandir(inputs_dir) as it:
            for entry in it:
                name = entry.name
                if not include_hidden and name.startswith("."):
                    continue
                if name in exclude_names:
                    continue
                if not entry.is_file(follow_symlinks=False):
                    continue
                yield entry.path
        return

    for root, dirnames, filenames in os.walk(inputs_dir):
        if not include_hidden:
            dirnames[:] = [d for d in dirnames if not d.startswith(".")]
        for fn in filenames:
            if not include_hidden and fn.startswith("."):
                continue
            if fn in exclude_names:
                continue
            path = os.path.join(root, fn)
            try:
                st = os.stat(path, follow_symlinks=False)
            except OSError:
                continue
            if not os.path.isfile(path):
                continue
            _ = st  # keep for potential future filtering
            yield path


def _parse_modes(s: str) -> List[int]:
    parts = [p.strip() for p in s.split(",") if p.strip()]
    if not parts:
        raise argparse.ArgumentTypeError("--modes cannot be empty")
    modes: List[int] = []
    for p in parts:
        try:
            v = int(p)
        except ValueError as e:
            raise argparse.ArgumentTypeError(f"invalid mode: {p}") from e
        if v not in (0, 1, 2):
            raise argparse.ArgumentTypeError(f"mode must be 0/1/2, got {v}")
        modes.append(v)
    # Preserve user order but remove duplicates.
    seen = set()
    out: List[int] = []
    for v in modes:
        if v not in seen:
            seen.add(v)
            out.append(v)
    return out


def _positive_int(s: str) -> int:
    try:
        v = int(s)
    except ValueError as e:
        raise argparse.ArgumentTypeError("must be an integer") from e
    if v <= 0:
        raise argparse.ArgumentTypeError("must be > 0")
    return v


def _positive_float(s: str) -> float:
    try:
        v = float(s)
    except ValueError as e:
        raise argparse.ArgumentTypeError("must be a number") from e
    if v <= 0:
        raise argparse.ArgumentTypeError("must be > 0")
    return v


def main(argv: Sequence[str]) -> int:
    ap = argparse.ArgumentParser(
        description="Run two commands over a directory of inputs and count unique (ret_a, ret_b) tuples per mode.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    ap.add_argument(
        "--cmd-a",
        required=True,
        help="Command template for executable A (supports {input} and {mode}).",
    )
    ap.add_argument(
        "--cmd-b",
        required=True,
        help="Command template for executable B (supports {input} and {mode}).",
    )
    ap.add_argument(
        "--inputs", required=True, help="Input directory containing test cases."
    )
    ap.add_argument(
        "--modes",
        type=_parse_modes,
        default="0,1,2",
        help="Comma-separated list of modes to run.",
    )
    ap.add_argument(
        "--recursive",
        action="store_true",
        help="Recursively traverse the input directory.",
    )
    ap.add_argument(
        "--include-hidden",
        action="store_true",
        help="Include files/dirs starting with '.' (hidden).",
    )
    ap.add_argument(
        "--exclude-name",
        action="append",
        default=["README.txt"],
        help="Exclude files whose base name equals this value. Can be specified multiple times.",
    )
    ap.add_argument(
        "--cwd", default=None, help="Working directory used to run commands."
    )
    ap.add_argument(
        "--timeout",
        type=_positive_float,
        default=None,
        help="Timeout (seconds) per command.",
    )
    ap.add_argument(
        "--jobs", type=_positive_int, default=1, help="Number of concurrent workers."
    )
    ap.add_argument(
        "--ignore-missing",
        action="store_true",
        help="Ignore samples where either command did not print a parsable ret.",
    )
    ap.add_argument(
        "--top",
        type=int,
        default=0,
        help="Print top-N most frequent tuples per mode (0 disables).",
    )
    ap.add_argument(
        "--json-out",
        default=None,
        help="Write full tuple frequency data to a JSON file.",
    )
    ap.add_argument(
        "--progress-every",
        type=_positive_int,
        default=0,
        help="Print progress every N processed (file,mode) jobs (0 disables).",
    )

    ns = ap.parse_args(list(argv))

    inputs_dir = ns.inputs
    if not os.path.isdir(inputs_dir):
        print(f"error: --inputs is not a directory: {inputs_dir}", file=sys.stderr)
        return 2

    cmd_a_parts = shlex.split(ns.cmd_a)
    cmd_b_parts = shlex.split(ns.cmd_b)

    modes: List[int] = (
        ns.modes if isinstance(ns.modes, list) else _parse_modes(ns.modes)
    )

    exclude_names = frozenset(ns.exclude_name)
    files = list(
        _iter_inputs(
            inputs_dir,
            recursive=ns.recursive,
            include_hidden=ns.include_hidden,
            exclude_names=exclude_names,
        )
    )
    files.sort()

    # Stats per mode
    tuple_counts: Dict[
        int, collections.Counter[Tuple[Optional[int], Optional[int]]]
    ] = {m: collections.Counter() for m in modes}
    missing_counts: Dict[int, int] = {m: 0 for m in modes}
    timeout_counts: Dict[int, int] = {m: 0 for m in modes}

    start = time.time()

    def work(
        item: Tuple[str, int],
    ) -> Tuple[int, Tuple[Optional[int], Optional[int]], bool, bool]:
        path, mode = item
        ra = _run_cmd(cmd_a_parts, path, mode, cwd=ns.cwd, timeout_s=ns.timeout)
        rb = _run_cmd(cmd_b_parts, path, mode, cwd=ns.cwd, timeout_s=ns.timeout)
        tup = (ra.ret, rb.ret)
        missing = (ra.ret is None) or (rb.ret is None)
        timed_out = ra.timed_out or rb.timed_out
        return mode, tup, missing, timed_out

    jobs: List[Tuple[str, int]] = [(p, m) for p in files for m in modes]

    processed = 0
    if ns.jobs == 1:
        for item in jobs:
            mode, tup, missing, timed_out = work(item)
            processed += 1
            if timed_out:
                timeout_counts[mode] += 1
            if missing:
                missing_counts[mode] += 1
                if ns.ignore_missing:
                    continue
            tuple_counts[mode][tup] += 1
            if ns.progress_every and (processed % ns.progress_every == 0):
                print(f"progress: {processed}/{len(jobs)}")
    else:
        with concurrent.futures.ThreadPoolExecutor(max_workers=ns.jobs) as ex:
            for mode, tup, missing, timed_out in ex.map(work, jobs, chunksize=1):
                processed += 1
                if timed_out:
                    timeout_counts[mode] += 1
                if missing:
                    missing_counts[mode] += 1
                    if ns.ignore_missing:
                        continue
                tuple_counts[mode][tup] += 1
                if ns.progress_every and (processed % ns.progress_every == 0):
                    print(f"progress: {processed}/{len(jobs)}")

    elapsed = time.time() - start

    # Summary
    print(f"files: {len(files)}")
    print(f"modes: {','.join(str(m) for m in modes)}")
    print(f"jobs: {len(jobs)}")
    print(f"elapsed_s: {elapsed:.3f}")

    for mode in modes:
        uniq = len(tuple_counts[mode])
        total = sum(tuple_counts[mode].values())
        print(
            f"mode={mode}: unique_tuples={uniq} total_counted={total} missing={missing_counts[mode]} timeouts={timeout_counts[mode]}"
        )
        if ns.top and ns.top > 0:
            print(f"mode={mode} top{ns.top}:")
            for (ra, rb), c in tuple_counts[mode].most_common(ns.top):
                print(f"  ({ra},{rb}) -> {c}")

    if ns.json_out:
        out = {
            "files": len(files),
            "modes": modes,
            "jobs": len(jobs),
            "elapsed_s": elapsed,
            "by_mode": {
                str(mode): {
                    "unique_tuples": len(tuple_counts[mode]),
                    "missing": missing_counts[mode],
                    "timeouts": timeout_counts[mode],
                    "tuples": [
                        {"a": ra, "b": rb, "count": c}
                        for (ra, rb), c in tuple_counts[mode].most_common()
                    ],
                }
                for mode in modes
            },
        }
        with open(ns.json_out, "w", encoding="utf-8") as f:
            json.dump(out, f, ensure_ascii=False, indent=2)

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
