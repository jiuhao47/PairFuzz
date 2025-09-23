#!/usr/bin/env python3
"""
validate_bugs.py

Usage:
  python3 validate_bugs.py -i <file_or_dir> [<file_or_dir> ...] [-m <map.json>] [-o <out.json>] [-t <timeout>]

This script reads `map.json` in a benchmark project directory (or a provided map file),
then for each bug it locates the two corresponding build/version directories and tries to
find an executable program under each. It runs each program against each input file with
the invocation: <program> <input_file> and treats output length > 0 as `reject`, length == 0 as `accept`.
If for a given input and bug one version accepts and the other rejects, the bug is considered triggered.

The script writes a JSON report with per-bug and per-input results.

This is a renamed and generalized version of the previous validate_libpng_bugs.py
so it supports validating bugs for multiple benchmark projects (libpng, libspng, ...)
by using the `-p/--project` argument.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from typing import List, Dict, Optional, Tuple


def load_map(map_path: str) -> Dict:
    with open(map_path, "r", encoding="utf-8") as f:
        return json.load(f)


def collect_input_files(paths: List[str]) -> List[str]:
    files = []
    for p in paths:
        if os.path.isfile(p):
            if not os.path.basename(p).startswith("."):
                files.append(os.path.abspath(p))
        elif os.path.isdir(p):
            for root, dirnames, filenames in os.walk(p):
                # skip hidden directories
                dirnames[:] = [d for d in dirnames if not d.startswith(".")]
                for fn in filenames:
                    if fn.startswith("."):
                        continue
                    files.append(os.path.abspath(os.path.join(root, fn)))
        else:
            print(f"warning: input path not found: {p}", file=sys.stderr)
    seen = set()
    out = []
    for f in files:
        if f not in seen:
            seen.add(f)
            out.append(f)
    return out


def find_all_build_target(map_json: Dict, name: str) -> Optional[str]:
    for b in map_json.get("all_builds", []):
        if b.get("name") == name:
            return b.get("target")
    return None


def find_executables_under(path: str) -> List[str]:
    exes = []
    if not os.path.exists(path):
        return exes
    for root, _, files in os.walk(path):
        for fn in files:
            p = os.path.join(root, fn)
            if p.endswith((".so", ".a", ".la", ".h", ".hpp", ".txt", ".md")):
                continue
            if os.path.isfile(p) and os.access(p, os.X_OK):
                exes.append(p)
    return exes


def choose_executable(candidates: List[str]) -> Optional[str]:
    if not candidates:
        return None
    app_candidates = [c for c in candidates if os.path.sep + "app" + os.path.sep in c]
    use = app_candidates or candidates
    use = sorted(use, key=lambda x: (len(x), x))
    return use[0]


def run_program(
    exe: str, input_file: str, timeout: int, ld_paths: Optional[List[str]] = None
) -> Tuple[int, int, str]:
    try:
        env = os.environ.copy()
        if ld_paths:
            cur = env.get("LD_LIBRARY_PATH", "")
            prepend = ":".join(ld_paths)
            env["LD_LIBRARY_PATH"] = prepend + ((":" + cur) if cur else "")
        p = subprocess.run(
            [exe, input_file], capture_output=True, timeout=timeout, env=env
        )
        out = (p.stdout or b"") + (p.stderr or b"")
        try:
            out_text = out.decode("utf-8", errors="replace")
        except Exception:
            out_text = str(out)
        return len(out), p.returncode, out_text
    except subprocess.TimeoutExpired:
        return -1, -1, f"<timeout after {timeout}s>"
    except FileNotFoundError:
        return -1, -1, "<executable not found>"
    except PermissionError:
        return -1, -1, "<permission error>"
    except Exception as e:
        return -1, -1, f"<error: {e}>"


def collect_ld_paths_for_exe(exe: str) -> List[str]:
    paths: List[str] = []
    if not exe:
        return paths
    start = os.path.dirname(os.path.abspath(exe))
    p = start
    for _ in range(5):
        for dname in ("lib", "lib64"):
            cand = os.path.join(p, dname)
            if os.path.isdir(cand) and cand not in paths:
                paths.append(cand)
        parent = os.path.dirname(p)
        if parent and parent != p:
            for dname in ("lib", "lib64"):
                cand2 = os.path.join(parent, dname)
                if os.path.isdir(cand2) and cand2 not in paths:
                    paths.append(cand2)
        if p == parent:
            break
        p = parent
    return paths


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Validate benchmark bugs using map.json"
    )
    parser.add_argument(
        "-i",
        "--inputs",
        nargs="+",
        required=True,
        help="One or more input files or directories",
    )
    parser.add_argument(
        "-p",
        "--project",
        default="libpng",
        help="Project under benchmark (libpng or libspng)",
    )
    parser.add_argument(
        "-m",
        "--map",
        dest="map_file",
        default=None,
        help="Path to map.json (default: benchmark/<project>/map.json)",
    )
    parser.add_argument(
        "-o", "--out", default="validate_bugs_results.json", help="Output JSON file"
    )
    parser.add_argument(
        "-t", "--timeout", type=int, default=10, help="Timeout (seconds) per run"
    )
    parser.add_argument(
        "-k",
        "--keep",
        choices=("all", "triggered"),
        default="triggered",
        help=(
            "What per-input results to keep in the output JSON: 'all' keeps everything; 'triggered' "
            "keeps only inputs that triggered the bug or had missing/error status."
        ),
    )
    args = parser.parse_args()

    if args.map_file:
        map_path = args.map_file
    else:
        map_path = os.path.join(os.path.dirname(__file__), args.project, "map.json")
    if not os.path.isfile(map_path):
        print(f"map.json not found at: {map_path}", file=sys.stderr)
        sys.exit(2)

    map_data = load_map(map_path)
    inputs = collect_input_files(args.inputs)
    if not inputs:
        print("no input files collected", file=sys.stderr)
        sys.exit(2)

    results = {
        "map_path": os.path.abspath(map_path),
        "inputs": inputs,
        "bugs": {},
        "created_at": time.strftime("%Y-%m-%d %H:%M:%S"),
    }

    mapping = map_data.get("mapping", {})
    total_bugs = len(mapping)
    print(f"Processing {total_bugs} bugs, {len(inputs)} inputs each")

    for idx, (bug_dir, version_names) in enumerate(mapping.items(), start=1):
        bug_entry = {
            "version_names": version_names,
            "exe_paths": [None, None],
            "results": {},
            "note": "",
        }

        # find executables for both versions
        exe_paths: List[Optional[str]] = [None, None]
        for i, vname in enumerate(version_names):
            suffix = vname.split("_")[-1]
            candidate_dir = os.path.join(bug_dir, suffix)
            found: List[str] = []
            if os.path.isdir(candidate_dir):
                found = find_executables_under(candidate_dir)
            if not found:
                target = find_all_build_target(map_data, vname)
                if target:
                    found = find_executables_under(target)
            if not found:
                alt = os.path.join(bug_dir, vname)
                if os.path.isdir(alt):
                    found = find_executables_under(alt)
            exe = choose_executable(found)
            exe_paths[i] = exe

        bug_entry["exe_paths"] = exe_paths

        # collect full results, then optionally filter what to keep in the output JSON
        full_results: Dict[str, Dict] = {}
        triggered_any_full = False
        error_or_missing_count = 0
        triggered_count = 0

        print(f"[{idx}/{total_bugs}] Processing bug: {bug_dir}")

        for in_file in inputs:
            r: Dict[str, object] = {}
            statuses: List[str] = []
            for i, exe in enumerate(exe_paths):
                label = f"v{i+1}"
                if exe is None:
                    r[label] = {
                        "status": "missing_exe",
                        "len": -1,
                        "exit_code": None,
                        "out_sample": "",
                    }
                    statuses.append("missing_exe")
                else:
                    ld_paths = collect_ld_paths_for_exe(exe)
                    out_len, exit_code, out_text = run_program(
                        exe, in_file, args.timeout, ld_paths=ld_paths
                    )
                    if out_len == -1 and exit_code == -1:
                        status = "error"
                    else:
                        status = "reject" if out_len > 0 else "accept"
                    r[label] = {
                        "status": status,
                        "len": out_len,
                        "exit_code": exit_code,
                        "out_sample": (
                            out_text[:2000]
                            if isinstance(out_text, str)
                            else str(out_text)
                        ),
                    }
                    statuses.append(status)

            triggered = False
            if any(s == "accept" for s in statuses) and any(
                s == "reject" for s in statuses
            ):
                triggered = True

            r["triggered"] = triggered
            full_results[in_file] = r

            if triggered:
                triggered_any_full = True
                triggered_count += 1
            if any(s in ("missing_exe", "error") for s in statuses):
                error_or_missing_count += 1

        # depending on args.keep, filter the per-input results included in output
        if args.keep == "all":
            bug_entry["results"] = full_results
        else:
            filtered = {
                inf: rr
                for inf, rr in full_results.items()
                if rr.get("triggered")
                or any(
                    rr.get(k, {}).get("status") in ("missing_exe", "error")
                    for k in ("v1", "v2")
                )
            }
            bug_entry["results"] = filtered

        # keep triggered_any based on the full scan (not the filtered view)
        bug_entry["triggered_any"] = triggered_any_full
        results["bugs"][bug_dir] = bug_entry

        print(
            f"[{idx}/{total_bugs}] Done {bug_dir}: triggered_inputs={triggered_count}, error_or_missing={error_or_missing_count} (kept={len(bug_entry['results'])})"
        )

    triggered_bugs = [
        b for b, info in results["bugs"].items() if info.get("triggered_any")
    ]
    results["summary"] = {
        "triggered_bugs": triggered_bugs,
        "triggered_count": len(triggered_bugs),
        "total_bugs": len(results["bugs"]),
    }

    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2, ensure_ascii=False)

    print(f"done. results written to {args.out}")


if __name__ == "__main__":
    main()
