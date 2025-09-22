#!/usr/bin/env python3
"""
analysis.py

提供一个简单的 CLI，用于扫描目标目录下的 .tar.gz 文件并交互式选择要解压的文件。

用法示例:
  python analysis.py /path/to/experiments

参数:
  target: 目标目录（必选）
  --all: 直接解压所有未解压的 tar.gz
  --dest DIR: 将 tar 包解压到指定目录（默认为 tar 所在目录）
  --yes: 在交互式选择后自动确认（用于脚本化短路径）

检测未解压逻辑（启发式）：如果存在 foo.tar.gz 而同目录没有名为 foo 的目录或文件，则视为未解压。
"""

from __future__ import annotations

import argparse
import os
import sys
import tarfile
from typing import List, Tuple
import json
import numpy as np
from fuzzer_stats_parser import FuzzerStatsParser
import shutil
import glob
from plot_fuzzer_stats import FuzzerStatsPlotter


def collect_crashes_in_subtask(sub_path: str) -> int:
    """Collect (move) crash-related files for one subtask. Returns number of crash entries moved."""
    diffs_dir = os.path.join(sub_path, "diffs")
    if not os.path.isdir(diffs_dir):
        return 0
    crashes_dir = os.path.join(sub_path, "crashes")
    os.makedirs(crashes_dir, exist_ok=True)
    count = 0
    # prefer leading-dot metadata files first
    meta_patterns = [
        os.path.join(diffs_dir, ".*_1.metadata"),
        os.path.join(diffs_dir, "*_1.metadata"),
    ]
    meta_files = []
    for pat in meta_patterns:
        meta_files.extend(glob.glob(pat))
    if not meta_files:
        meta_files = glob.glob(os.path.join(diffs_dir, "*.metadata"))

    for meta_path in meta_files:
        try:
            with open(meta_path, "r", encoding="utf-8") as mf:
                m = json.load(mf)
        except Exception:
            continue
        mapobj = m.get("metadata", {}).get("map", {})
        meta_basename = os.path.basename(meta_path)
        if meta_basename.startswith("."):
            candidate_hash = meta_basename[1:]
        else:
            candidate_hash = meta_basename
        if candidate_hash.endswith("_1.metadata"):
            H = candidate_hash[: -len("_1.metadata")]
        else:
            H = candidate_hash

        is_crash = False
        for key, val in mapobj.items():
            if isinstance(val, list) and len(val) >= 2:
                info = val[1]
                if isinstance(info, dict) and info.get("note") == "crash_any":
                    is_crash = True
                    break

        if not is_crash:
            continue

        # move the three files if present: H, .H, .H_1.metadata
        candidates = [
            os.path.join(diffs_dir, H),
            os.path.join(diffs_dir, f".{H}"),
            os.path.join(diffs_dir, f".{H}_1.metadata"),
        ]
        moved_any = False
        for src in candidates:
            if os.path.exists(src):
                try:
                    shutil.move(src, crashes_dir)
                    moved_any = True
                except Exception:
                    pass
        if moved_any:
            count += 1

    return count


def collect_crashes_tasks(base_target: str) -> int:
    total = 0
    for task_name in sorted(os.listdir(base_target)):
        task_path = os.path.join(base_target, task_name)
        if not os.path.isdir(task_path):
            continue
        for sub in sorted(os.listdir(task_path)):
            sub_path = os.path.join(task_path, sub)
            if not os.path.isdir(sub_path):
                continue
            c = collect_crashes_in_subtask(sub_path)
            if c:
                print(f"Extracted {c} crash entries for {task_name}/{sub}")
            total += c
    print(f"Total extracted crashes: {total}")
    return total


def find_tarballs(target: str) -> List[str]:
    """递归扫描目标目录，返回候选的 .tar.gz 文件路径列表。"""
    tar_paths: List[str] = []
    for root, dirs, files in os.walk(target):
        for name in files:
            # 支持常见 tar 压缩扩展，以提高实用性
            if name.endswith((".tar.gz", ".tgz", ".tar")):
                tar_paths.append(os.path.join(root, name))
    return tar_paths


def is_extracted(tar_path: str, strict: bool = False) -> bool:
    """判断 tar 是否已被解压。

    默认（strict=False）使用快速启发式：基于文件名和常见变体在同目录是否存在目标目录/文件来判断（非常快）。
    若 strict=True，则打开 tar 检查其顶层成员并尝试匹配（可能慢，因为会打开并读取 tar）。
    在 I/O 错误时采用保守策略：不认为已解压（返回 False）。
    """
    dirpath = os.path.dirname(tar_path)
    filename = os.path.basename(tar_path)

    # 推断 base 名称（去掉常见后缀）
    base = filename
    for suf in (".tar.gz", ".tgz", ".tar"):
        if base.endswith(suf):
            base = base[: -len(suf)]
            break

    # 优先检查：如果文件名带有 _mirror 或 -mirror，尝试去掉该后缀再判断
    if base.endswith("_mirror") or base.endswith("-mirror"):
        stripped = base
        for suf in ("_mirror", "-mirror"):
            if stripped.endswith(suf):
                stripped = stripped[: -len(suf)]
                break
        stripped_path = os.path.join(dirpath, stripped)
        if os.path.exists(stripped_path):
            return True

    # 快速启发式：根据文件名和常见变体判断（不打开 tar）
    variants = [base, base + "_mirror", base + "-mirror", base + "_copy"]
    for cand in variants:
        candidate_path = os.path.join(dirpath, cand)
        if os.path.exists(candidate_path):
            return True

    if not strict:
        # 非严格模式下已检查常见变体，判定为未解压
        return False

    # 严格模式：打开 tar，读取顶层成员并匹配
    try:
        with tarfile.open(tar_path, "r:*") as tf:
            members = tf.getnames()
            if members:
                tops = {m.split("/", 1)[0] for m in members if m}
                for t in tops:
                    candidate = os.path.join(dirpath, t)
                    if os.path.exists(candidate):
                        return True
    except (tarfile.TarError, OSError):
        return False

    return False


def list_unextracted(tar_paths: List[str], strict: bool = False) -> List[str]:
    return [p for p in tar_paths if not is_extracted(p, strict=strict)]


def parse_selection(text: str, max_index: int) -> List[int]:
    text = text.strip().lower()
    if text in ("all", "*"):
        return list(range(1, max_index + 1))
    if text in ("none", "n", ""):
        return []
    parts = [p.strip() for p in text.split(",") if p.strip()]
    selected = set()
    for part in parts:
        if "-" in part:
            a, b = part.split("-", 1)
            try:
                ia = int(a)
                ib = int(b)
            except ValueError:
                continue
            if ia <= ib:
                for i in range(max(1, ia), min(max_index, ib) + 1):
                    selected.add(i)
        else:
            try:
                i = int(part)
            except ValueError:
                continue
            if 1 <= i <= max_index:
                selected.add(i)
    return sorted(selected)


def extract_tarball(tar_path: str, dest: str) -> Tuple[bool, str]:
    try:
        with tarfile.open(tar_path, "r:*") as tf:
            tf.extractall(path=dest)
        return True, ""
    except (tarfile.TarError, OSError) as e:
        return False, str(e)


def interactive_choose_and_extract(
    candidates: List[str], dest_override: str | None, auto_yes: bool
) -> None:
    if not candidates:
        print("未找到未解压的 .tar.gz 文件。")
        return

    print("找到以下未解压的 tar.gz 文件：")
    for idx, p in enumerate(candidates, start=1):
        print(f"  {idx}. {p}")

    if auto_yes:
        selection = "all"
    else:
        selection = input(
            "选择要解压的编号（逗号或范围，例如 1,3-5），输入 'all' 表示全部，直接回车表示 none: "
        )

    indices = parse_selection(selection, len(candidates))
    if not indices:
        print("未选择任何文件，退出。")
        return

    successes = []
    failures = []
    for i in indices:
        tar_path = candidates[i - 1]
        dest = dest_override if dest_override else os.path.dirname(tar_path)
        print(f"正在解压: {tar_path} -> {dest}")
        ok, err = extract_tarball(tar_path, dest)
        if ok:
            successes.append(tar_path)
            print(f"  ✓ 解压完成: {tar_path}")
        else:
            failures.append((tar_path, err))
            print(f"  ✗ 解压失败: {tar_path}  错误: {err}")

    print("\n总结：")
    print(f"  成功：{len(successes)}")
    for s in successes:
        print(f"    - {s}")
    print(f"  失败：{len(failures)}")
    for f, e in failures:
        print(f"    - {f}  错误: {e}")


def main(argv: List[str] | None = None) -> int:
    argv = argv if argv is not None else sys.argv[1:]
    parser = argparse.ArgumentParser(
        description="扫描并解压目标目录下的未解压 .tar.gz 文件"
    )
    parser.add_argument("target", help="目标目录")
    parser.add_argument(
        "--all", action="store_true", help="直接解压所有未解压的 tar.gz（非交互）"
    )
    parser.add_argument("--dest", help="覆盖解压目录（默认解压到 tar 所在目录）")
    parser.add_argument(
        "--yes",
        "-y",
        action="store_true",
        help="在交互式选择后自动确认（等同于选择后自动继续）",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="启用严格检测（会打开 tar 并检查顶层成员，较慢）",
    )
    parser.add_argument(
        "--summarize",
        action="store_true",
        help="扫描目标目录下的 fuzzer_stats.json 并为每个任务生成 info.json 汇总",
    )
    parser.add_argument(
        "--plot",
        action="store_true",
        help="为每个 subtask 生成 fuzzer_stats 的图并保存到 subtask/plot/",
    )
    parser.add_argument(
        "--extract-crashes",
        action="store_true",
        help="从每个 subtask 的 diffs 中提取 note == 'crash_any' 的文件到 crashes/ 文件夹",
    )
    parser.add_argument(
        "--aggregate",
        action="store_true",
        help="对同一目录下多个同构任务的 info.json 做逐字段平均并输出 aggregated_info.json",
    )
    args = parser.parse_args(argv)

    target = args.target
    if not os.path.isdir(target):
        print(f"错误：目标路径不是目录或不存在: {target}", file=sys.stderr)
        return 2

    if args.summarize:

        def get_last_value(series_tuple):
            """Return last non-NaN value from (ts_array, vals_array) as int or float or None."""
            if series_tuple is None:
                return None
            ts, vals = series_tuple
            if len(vals) == 0:
                return None
            try:
                mask = ~np.isnan(vals)
                idxs = np.where(mask)[0]
                if idxs.size == 0:
                    return None
                last = idxs[-1]
                v = vals[last]
                if np.isnan(v):
                    return None
                if abs(v - round(v)) < 1e-9:
                    return int(round(v))
                return float(v)
            except Exception:
                try:
                    v = vals[-1]
                    if np.isnan(v):
                        return None
                    if abs(v - round(v)) < 1e-9:
                        return int(round(v))
                    return float(v)
                except Exception:
                    return None

        def summarize_subtask(sub_path: str):
            """Summarize a single subtask directory; returns info dict.

            Ensures crash artifacts are extracted (if missing), then counts non-dot
            files in diffs/ and crashes/ and includes those counts in the
            objective_size field alongside the original objective value.
            """
            fpath = os.path.join(sub_path, "log", "fuzzer_stats.json")

            # default info structure
            info = {
                "corpus_size": None,
                "objective_size": None,
                "exec_sec": None,
                "user_stats": {},
            }

            # ensure crashes extracted so we can count them
            crashes_dir = os.path.join(sub_path, "crashes")
            diffs_dir = os.path.join(sub_path, "diffs")
            if not os.path.isdir(crashes_dir):
                # perform extraction for this subtask
                try:
                    _moved = collect_crashes_in_subtask(sub_path)
                except Exception:
                    _moved = 0

            # count non-dot files in diffs and crashes
            def count_nondot_files(dpath: str) -> int:
                if not os.path.isdir(dpath):
                    return 0
                try:
                    return len([n for n in os.listdir(dpath) if not n.startswith(".")])
                except Exception:
                    return 0

            diffs_count = count_nondot_files(diffs_dir)
            crashes_count = count_nondot_files(crashes_dir)

            # read fuzzer_stats.json if present
            objective_val = None
            if os.path.exists(fpath):
                records = FuzzerStatsParser.load_records(fpath)
                data = FuzzerStatsParser.collect_series(records)
                info["corpus_size"] = get_last_value(data.get("corpus"))
                objective_val = get_last_value(data.get("objectives"))
                info["exec_sec"] = get_last_value(data.get("exec_sec"))
                for k, v in data.items():
                    if k.startswith("user:"):
                        uname = k[len("user:") :]
                        info["user_stats"][uname] = get_last_value(v)
            else:
                info["error"] = "missing fuzzer_stats.json"

            # put objective_size as a dict with original value and counts
            info["objective_size"] = {
                "value": objective_val,
                "diffs_count": diffs_count,
                "crashes_count": crashes_count,
            }

            # attach COMMENT.txt if present
            comment_path = os.path.join(sub_path, "COMMENT.txt")
            if os.path.exists(comment_path):
                try:
                    with open(comment_path, "r", encoding="utf-8") as cf:
                        info["comment"] = cf.read().strip()
                except Exception:
                    pass
            return info

        def summarize_tasks(base_target: str):
            """Iterate task dirs under base_target and write info.json for each task."""
            for task_name in sorted(os.listdir(base_target)):
                task_path = os.path.join(base_target, task_name)
                if not os.path.isdir(task_path):
                    continue
                summary = {"task": task_name, "subtasks": {}}
                for sub in sorted(os.listdir(task_path)):
                    sub_path = os.path.join(task_path, sub)
                    if not os.path.isdir(sub_path):
                        continue
                    try:
                        info = summarize_subtask(sub_path)
                    except Exception as e:
                        info = {"error": str(e)}
                    summary["subtasks"][sub] = info

                    # optionally generate plots for each subtask
                    if args.plot:
                        fpath = os.path.join(sub_path, "log", "fuzzer_stats.json")
                        if os.path.exists(fpath):
                            try:
                                records = FuzzerStatsParser.load_records(fpath)
                                data = FuzzerStatsParser.collect_series(records)
                                plot_dir = os.path.join(sub_path, "plot")
                                plotter = FuzzerStatsPlotter(
                                    markersize=2.0,
                                    dpi=150,
                                    annotate=True,
                                    annotate_top_n=5,
                                )
                                plots = plotter.plot_series(data, plot_dir)
                                print(
                                    f"Wrote {len(plots)} plots for {task_name}/{sub} -> {plot_dir}"
                                )
                            except Exception as e:
                                print(
                                    f"Failed to plot for {task_name}/{sub}: {e}",
                                    file=sys.stderr,
                                )

                outpath = os.path.join(task_path, "info.json")
                try:
                    with open(outpath, "w") as outf:
                        json.dump(summary, outf, indent=2, ensure_ascii=False)
                    print(f"Wrote summary for task {task_name} -> {outpath}")
                except Exception as e:
                    print(
                        f"Failed to write summary for {task_name}: {e}", file=sys.stderr
                    )

        summarize_tasks(target)
        return 0

    if args.aggregate:

        def average_numeric(vals):
            nums = [v for v in vals if isinstance(v, (int, float))]
            if not nums:
                return None
            return float(sum(nums)) / len(nums)

        def aggregate_tasks(base_target: str):
            # collect info.json from each task folder
            task_infos = []
            task_names = []
            for task_name in sorted(os.listdir(base_target)):
                task_path = os.path.join(base_target, task_name)
                if not os.path.isdir(task_path):
                    continue
                info_path = os.path.join(task_path, "info.json")
                if not os.path.exists(info_path):
                    continue
                try:
                    with open(info_path, "r", encoding="utf-8") as inf:
                        data = json.load(inf)
                    task_infos.append(data)
                    task_names.append(task_name)
                except Exception:
                    continue

            if not task_infos:
                print("No info.json files found to aggregate.")
                return 1

            # map subtask_name -> list of info dicts (one per task)
            subtasks_map = {}
            for info in task_infos:
                subs = info.get("subtasks", {})
                for subname, sinfo in subs.items():
                    subtasks_map.setdefault(subname, []).append(sinfo)

            aggregated = {"tasks_aggregated": len(task_infos), "subtasks": {}}

            for subname, sinfos in sorted(subtasks_map.items()):
                # aggregate numeric fields
                corpus_vals = [s.get("corpus_size") for s in sinfos]
                exec_vals = [s.get("exec_sec") for s in sinfos]

                # objective_size may be a dict with value/diffs_count/crashes_count
                obj_vals = []
                diffs_vals = []
                crashes_vals = []
                for s in sinfos:
                    o = s.get("objective_size")
                    if isinstance(o, dict):
                        obj_vals.append(o.get("value"))
                        diffs_vals.append(o.get("diffs_count"))
                        crashes_vals.append(o.get("crashes_count"))
                    else:
                        obj_vals.append(o)

                # user stats: collect per-username
                user_map = {}
                for s in sinfos:
                    us = s.get("user_stats", {}) or {}
                    for uname, uval in us.items():
                        user_map.setdefault(uname, []).append(uval)

                aggregated_sub = {}
                aggregated_sub["corpus_size"] = average_numeric(corpus_vals)
                aggregated_sub["exec_sec"] = average_numeric(exec_vals)
                aggregated_sub["objective_size"] = {
                    "value": average_numeric(obj_vals),
                    "diffs_count": average_numeric(diffs_vals),
                    "crashes_count": average_numeric(crashes_vals),
                }

                # aggregate user_stats
                agg_users = {}
                for uname, uvals in user_map.items():
                    avg = average_numeric(uvals)
                    if avg is not None:
                        agg_users[uname] = avg
                aggregated_sub["user_stats"] = agg_users

                # collect comments and errors
                comments = []
                error_count = 0
                for s in sinfos:
                    c = s.get("comment")
                    if c:
                        if c not in comments:
                            comments.append(c)
                    if s.get("error"):
                        error_count += 1

                aggregated_sub["comments"] = comments
                aggregated_sub["errors_count"] = error_count
                aggregated_sub["samples"] = len(sinfos)

                aggregated["subtasks"][subname] = aggregated_sub

            outpath = os.path.join(base_target, "aggregated_info.json")
            try:
                with open(outpath, "w", encoding="utf-8") as outf:
                    json.dump(aggregated, outf, indent=2, ensure_ascii=False)
                print(f"Wrote aggregated summary -> {outpath}")
            except Exception as e:
                print(f"Failed to write aggregated summary: {e}", file=sys.stderr)
                return 2
            return 0

        return aggregate_tasks(target)

    if args.extract_crashes:
        # use module-level functions to collect crashes
        collect_crashes_tasks(target)
        return 0

    tar_paths = find_tarballs(target)
    unextracted = list_unextracted(tar_paths, strict=args.strict)

    if not unextracted:
        print("未发现需要解压的 tar.gz 文件。")
        return 0

    if args.all:
        # 直接全部解压
        interactive_choose_and_extract(unextracted, args.dest, auto_yes=True)
        return 0

    interactive_choose_and_extract(unextracted, args.dest, auto_yes=args.yes)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
