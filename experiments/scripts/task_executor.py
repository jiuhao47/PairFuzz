#!/usr/bin/env python3
"""Task executor built on top of CommandExecutor.

Features:
- Load a JSON task config and copy it into a created task root folder.
- Create per-task folder named with timestamp + id/uuid to keep tasks unique.
- For each subtask: create required directories, copy files, create files with content,
  and run listed shell commands (with params filled in).
- Write human-readable comments and a README under the task root to guide users.

Config format (JSON) example:
{
  "id": "mytask",
  "comment": "High level task comment",
  "subtasks": [
    {
      "id": "step1",
      "comment": "This creates dirs and echoes a message",
      "params": {"name": "alice"},
      "prepare": ["input", "work/tmp"],
      "copy": [{"src": "path/on/host/file.txt", "dest": "input/file.txt"}],
      "create_files": [{"path": "input/info.txt", "content": "hello"}],
      "commands": ["echo Hello {name}", "ls -la input"]
    }
  ]
}

The executor will expand {param} style placeholders in commands using subtask.params.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import uuid
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional
import concurrent.futures
import threading
import signal
import time
import subprocess

try:
    # when run from repo root
    from command_executor import CommandExecutor
except Exception:
    # package-style import when executed from scripts folder
    from experiments.scripts.command_executor import CommandExecutor  # type: ignore

try:
    from experiments.scripts.cpu_assessor import CPUAssessor  # type: ignore
except Exception:
    CPUAssessor = None


@dataclass
class SubTaskConfig:
    id: str
    comment: str = ""
    params: Dict[str, Any] = field(default_factory=dict)
    prepare: List[str] = field(default_factory=list)  # dirs to create under subtask dir
    copy: List[Dict[str, str]] = field(default_factory=list)  # {src, dest}
    create_files: List[Dict[str, str]] = field(default_factory=list)  # {path, content}
    # optional CPU affinity spec: int (count) or str/list of cpu indices/ranges
    cpus: Optional[Any] = None
    # commands can be either strings (legacy) or dicts with keys:
    # {"cmd": "...", "mode": "foreground|bg_with_output|bg_no_output", "redirect_output": bool}
    commands: List[Any] = field(default_factory=list)


@dataclass
class TaskConfig:
    id: str
    comment: str = ""
    subtasks: List[SubTaskConfig] = field(default_factory=list)


class TaskExecutor:
    def __init__(self) -> None:
        self.ce = CommandExecutor()

    # bg jobs are persisted per-task in this file
    @staticmethod
    def _bg_jobs_file(task_root: Path) -> Path:
        return task_root / "bg_jobs.json"

    def _persist_bg_job(self, task_root: Path, entry: Dict[str, Any]) -> None:
        p = self._bg_jobs_file(task_root)
        jobs: List[Dict[str, Any]] = []
        if p.exists():
            try:
                jobs = json.loads(p.read_text())
            except Exception:
                jobs = []
        jobs.append(entry)
        p.write_text(json.dumps(jobs, indent=2))

    def list_bg_jobs(self, task_root: Path) -> List[Dict[str, Any]]:
        p = self._bg_jobs_file(task_root)
        if not p.exists():
            return []
        try:
            return json.loads(p.read_text())
        except Exception:
            return []

    def _terminate_bg_jobs(
        self, task_root: Path, job_ids: Optional[List[str]] = None
    ) -> Dict[str, Any]:
        """Attempt to terminate/kill background jobs. If job_ids is None, terminate all recorded jobs."""
        jobs = self.list_bg_jobs(task_root)
        targets = []
        if job_ids is None:
            targets = jobs
        else:
            targets = [j for j in jobs if j.get("job_id") in job_ids]

        results: Dict[str, Any] = {"terminated": [], "failed": []}
        for j in targets:
            jid = j.get("job_id")
            pid = j.get("pid")
            ok = False
            # try using in-memory CommandExecutor if available
            try:
                if jid and hasattr(self.ce, "is_running") and self.ce.is_running(jid):
                    self.ce.terminate(jid)
                    # small wait
                    time.sleep(0.1)
                    if self.ce.is_running(jid):
                        self.ce.kill(jid)
                    ok = True
            except Exception:
                ok = False

            # fallback to killing by pid
            if not ok and pid:
                try:
                    os.kill(int(pid), signal.SIGTERM)
                    time.sleep(0.1)
                    # if still exists, SIGKILL
                    try:
                        os.kill(int(pid), 0)
                        os.kill(int(pid), signal.SIGKILL)
                    except Exception:
                        pass
                    ok = True
                except Exception:
                    ok = False

            if ok:
                results["terminated"].append(jid or pid)
            else:
                results["failed"].append(jid or pid)

            # mark terminated in file (best-effort)
            try:
                remaining = jobs
                for r in remaining:
                    if r.get("job_id") in results["terminated"]:
                        r["terminated_at"] = datetime.now(timezone.utc).isoformat()
                p = self._bg_jobs_file(task_root)
                p.write_text(json.dumps(remaining, indent=2))
            except Exception:
                pass

        return results

    @staticmethod
    def load_config(path: Path) -> TaskConfig:
        data = json.loads(path.read_text())
        tid = data.get("id") or str(uuid.uuid4())
        comment = data.get("comment", "")
        subs: List[SubTaskConfig] = []
        for s in data.get("subtasks", []):
            subs.append(
                SubTaskConfig(
                    id=s.get("id", str(uuid.uuid4())),
                    comment=s.get("comment", ""),
                    params=s.get("params", {}),
                    prepare=s.get("prepare", []),
                    copy=s.get("copy", []),
                    create_files=s.get("create_files", []),
                    cpus=s.get("cpus", None),
                    commands=s.get("commands", []),
                )
            )
        return TaskConfig(id=tid, comment=comment, subtasks=subs)

    @staticmethod
    def make_task_root(base: Path, task_id: str) -> Path:
        ts = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        safe_id = task_id.replace("/", "_")
        folder = base / f"{ts}_{safe_id}"
        folder.mkdir(parents=True, exist_ok=False)
        return folder

    def create_task(self, config_path: Path, tasks_root: Path) -> Path:
        """Create task folder, copy config into it, and prepare subtask folders and files.

        Returns the created task root path.
        """
        cfg = self.load_config(config_path)
        task_root = self.make_task_root(tasks_root, cfg.id)

        # copy config file into task root
        shutil.copy2(config_path, task_root / Path(config_path).name)

        # write a README summarizing comments
        readme_lines = [
            f"Task ID: {cfg.id}",
            f"Comment: {cfg.comment}",
            "",
            "Subtasks:",
        ]
        for idx, s in enumerate(cfg.subtasks, start=1):
            readme_lines.append(f"  {idx}. {s.id} - {s.comment}")

        (task_root / "README.txt").write_text("\n".join(readme_lines))

        # create subtasks
        for idx, s in enumerate(cfg.subtasks, start=1):
            sub_dir = task_root / f"subtask_{idx}_{s.id}"
            sub_dir.mkdir(parents=True, exist_ok=True)

            # create prepare dirs
            for d in s.prepare:
                p = sub_dir / d
                p.mkdir(parents=True, exist_ok=True)

            # copy files (src is treated as absolute or relative to current CWD)
            for cp in s.copy:
                src_str = cp.get("src")
                dest_rel = cp.get("dest")
                if not src_str:
                    # dest will be used to record the missing source
                    dest = sub_dir / (dest_rel or "UNKNOWN")
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    (dest.parent / (dest.name + ".MISSING.txt")).write_text(
                        "Source not specified in config\n"
                    )
                    continue

                src = Path(src_str)
                dest = sub_dir / (dest_rel or src.name)
                dest.parent.mkdir(parents=True, exist_ok=True)
                if src.exists():
                    if src.is_dir():
                        # allow copying into an existing destination (idempotent prepare)
                        try:
                            shutil.copytree(src, dest, dirs_exist_ok=True)
                        except TypeError:
                            # older Python versions may not support dirs_exist_ok; fall back
                            # to skipping if dest exists, otherwise perform copytree
                            if dest.exists():
                                # nothing to do; assume existing tree is acceptable
                                pass
                            else:
                                shutil.copytree(src, dest)
                    else:
                        shutil.copy2(src, dest)
                else:
                    # record missing source as a warning file so user knows
                    (dest.parent / (dest.name + ".MISSING.txt")).write_text(
                        f"Source not found: {src}\n"
                    )

            # create files with provided content
            for cf in s.create_files:
                path_str = cf.get("path")
                if not path_str:
                    continue
                p = sub_dir / path_str
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_text(cf.get("content", ""))

            # store subtask comment
            (sub_dir / "COMMENT.txt").write_text(s.comment or "")

        return task_root

    def run_task(
        self,
        task_root: Path,
        capture_output: bool = True,
        stop_on_fail: bool = True,
        concurrency: int = 1,
    ) -> Dict[str, Any]:
        """Run all subtasks found under a task_root. Returns a result dict.

        If concurrency <= 1 the subtasks are executed sequentially (original
        behavior). If concurrency > 1 a ThreadPoolExecutor is used to run
        subtasks in parallel. stop_on_fail controls whether discovering a failed
        subtask should stop scheduling new subtasks (best-effort; running
        commands already started are not forcefully killed here).
        """
        results: Dict[str, Any] = {"subtasks": []}

        # attempt to find a config file in the root (first .json found)
        cfg_file = None
        for p in task_root.iterdir():
            if p.suffix == ".json":
                cfg_file = p
                break

        if cfg_file is None:
            raise FileNotFoundError("No JSON config file found in task root")

        cfg = self.load_config(cfg_file)

        # simple allocator for integer `cpus` requests when CPUAssessor is not present.
        # Ensures mutually exclusive allocation across subtasks in a single task run.
        total_cpus = os.cpu_count() or 1
        _allocated_cpus: set = set()
        _alloc_lock = threading.Lock()

        def _allocate_cpus(n: int):
            with _alloc_lock:
                free = [i for i in range(total_cpus) if i not in _allocated_cpus]
                if len(free) < n:
                    return None
                chosen = free[:n]
                _allocated_cpus.update(chosen)
                return chosen

        # helper to execute one subtask (used by both sequential and parallel flows)
        def _exec_subtask(
            idx: int, s: SubTaskConfig, stop_event: Optional[threading.Event] = None
        ) -> Dict[str, Any]:
            sub_dir = task_root / f"subtask_{idx}_{s.id}"
            sub_res: Dict[str, Any] = {"id": s.id, "commands": []}

            for cmd_item in s.commands:
                if stop_event is not None and stop_event.is_set():
                    sub_res["skipped"] = True
                    return sub_res

                # normalize command item: either string or dict
                if isinstance(cmd_item, str):
                    cmd_dict = {
                        "cmd": cmd_item,
                        "mode": "foreground",
                        "redirect_output": False,
                    }
                else:
                    cmd_dict = dict(cmd_item)

                raw_cmd = cmd_dict.get("cmd", "")
                mode = cmd_dict.get("mode", "foreground")
                redirect_output = bool(cmd_dict.get("redirect_output", False))

                # resolve params: if a param is a relative path that exists under
                # the prepared subtask dirs, convert it to the absolute path
                resolved_params: Dict[str, Any] = {}
                for pk, pv in (s.params or {}).items():
                    if isinstance(pv, str) and not pv.startswith("/"):
                        candidate = sub_dir / pv
                        if candidate.exists():
                            resolved_params[pk] = str(candidate.resolve())
                            continue
                        # also allow mapping when pv refers to a prepared directory
                        if any(
                            pv == d or pv.startswith(d + "/") for d in (s.prepare or [])
                        ):
                            resolved_params[pk] = str((sub_dir / pv).resolve())
                            continue
                    resolved_params[pk] = pv

                # fill params using resolved mapping
                try:
                    filled = raw_cmd.format(**resolved_params)
                except Exception:
                    filled = raw_cmd

                # prepare optional cpu affinity prefix
                cpu_prefix = ""
                affinity_cpus: Optional[List[int]] = None
                if getattr(s, "cpus", None) is not None:
                    spec = None
                    # if integer and CPUAssessor available, ask assessor to pick free CPUs
                    if isinstance(s.cpus, int) and CPUAssessor is not None:
                        n = int(s.cpus)
                        assessor = CPUAssessor()
                        report = assessor.assess()
                        allowed_free = report.get("allowed_free", [])
                        if len(allowed_free) < n:
                            # not enough free CPUs: mark subtask failed with readable message
                            sub_res["failed"] = True
                            sub_res["error"] = (
                                f"Requested {n} free CPU(s), but only {len(allowed_free)} available."
                                f" Assessor reason: {report.get('reason')}."
                            )
                            return sub_res
                        chosen = allowed_free[:n]
                        spec = ",".join(str(x) for x in chosen)
                        affinity_cpus = [int(x) for x in chosen]
                    elif isinstance(s.cpus, int):
                        # fallback: request `n` CPUs from local allocator to avoid overlap
                        n = int(s.cpus)
                        chosen = None
                        # try allocator first
                        try:
                            chosen = _allocate_cpus(n)
                        except Exception:
                            chosen = None

                        if chosen is None:
                            # fallback legacy behavior: map integer n -> 0-(n-1)
                            if n <= 1:
                                spec = "0"
                                affinity_cpus = [0]
                            else:
                                spec = f"0-{max(0, n-1)}"
                                try:
                                    affinity_cpus = list(range(0, max(1, n)))
                                except Exception:
                                    affinity_cpus = None
                        else:
                            spec = ",".join(str(x) for x in chosen)
                            affinity_cpus = [int(x) for x in chosen]
                    elif isinstance(s.cpus, (list, tuple)):
                        spec = ",".join(str(x) for x in s.cpus)
                    else:
                        spec = str(s.cpus)
                    # use taskset and run under bash -lc to ensure children inherit
                    cpu_prefix = f'taskset -c {spec} bash -lc "'

                out_log = sub_dir / "stdout.log"
                err_log = sub_dir / "stderr.log"

                # FOREGROUND
                if mode == "foreground":
                    if capture_output:
                        if affinity_cpus:
                            # run with strong affinity using preexec_fn
                            local_cpus = list(affinity_cpus)

                            def _pre():
                                try:
                                    os.sched_setaffinity(0, set(local_cpus))
                                except Exception:
                                    pass

                            completed = subprocess.run(
                                filled,
                                shell=True,
                                cwd=str(sub_dir),
                                stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE,
                                text=True,
                                preexec_fn=_pre,
                            )
                            r = {
                                "returncode": completed.returncode,
                                "stdout": completed.stdout,
                                "stderr": completed.stderr,
                            }
                        else:
                            cmd_to_run = filled
                            if cpu_prefix:
                                cmd_to_run = (
                                    cpu_prefix + filled.replace('"', '\\"') + '"'
                                )
                            r = self.ce.run_foreground(
                                cmd_to_run, cwd=str(sub_dir), capture_output=True
                            )
                        out_log.write_text(
                            (out_log.read_text() if out_log.exists() else "")
                            + (r.get("stdout") or "")
                        )
                        err_log.write_text(
                            (err_log.read_text() if err_log.exists() else "")
                            + (r.get("stderr") or "")
                        )
                        sub_res["commands"].append(
                            {"cmd": filled, "returncode": r.get("returncode")}
                        )
                        if r.get("returncode", 0) != 0:
                            sub_res["failed"] = True
                            return sub_res
                    else:
                        if affinity_cpus:
                            local_cpus = list(affinity_cpus)

                            def _pre():
                                try:
                                    os.sched_setaffinity(0, set(local_cpus))
                                except Exception:
                                    pass

                            completed = subprocess.run(
                                filled, shell=True, cwd=str(sub_dir), preexec_fn=_pre
                            )
                            r = {
                                "returncode": completed.returncode,
                                "stdout": None,
                                "stderr": None,
                            }
                        else:
                            cmd_to_run = filled
                            if cpu_prefix:
                                cmd_to_run = (
                                    cpu_prefix + filled.replace('"', '\\"') + '"'
                                )
                            r = self.ce.run_foreground(
                                cmd_to_run, cwd=str(sub_dir), capture_output=False
                            )
                        sub_res["commands"].append(
                            {"cmd": filled, "returncode": r.get("returncode")}
                        )
                        if r.get("returncode", 0) != 0:
                            sub_res["failed"] = True
                            return sub_res

                # BACKGROUND WITH STREAMED OUTPUT
                elif mode == "bg_with_output":
                    # if redirect_output requested, wrap with tee to also persist to file
                    if redirect_output:
                        inner = f"{filled} 2>&1 | tee -a {out_log}"
                        if affinity_cpus:
                            local_cpus = list(affinity_cpus)

                            def _pre():
                                try:
                                    os.sched_setaffinity(0, set(local_cpus))
                                except Exception:
                                    pass

                            proc = subprocess.Popen(
                                inner,
                                shell=True,
                                cwd=str(sub_dir),
                                stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT,
                                text=True,
                                bufsize=1,
                                preexec_fn=_pre,
                            )
                            job_id = str(uuid.uuid4())
                            try:
                                self.ce._procs[job_id] = proc
                                self.ce._locks[job_id] = threading.Lock()
                            except Exception:
                                pass

                            def _stream(p, jid, l):
                                prefix = f"[bg:{jid}] "
                                try:
                                    assert p.stdout is not None
                                    for line in p.stdout:
                                        with l:
                                            print(prefix + line.rstrip())
                                except Exception:
                                    pass
                                finally:
                                    if p.stdout:
                                        p.stdout.close()

                            t = threading.Thread(
                                target=_stream,
                                args=(
                                    proc,
                                    job_id,
                                    self.ce._locks.get(job_id, threading.Lock()),
                                ),
                                daemon=True,
                            )
                            t.start()
                            res = {"job_id": job_id, "pid": proc.pid}
                        else:
                            if cpu_prefix:
                                esc = inner.replace('"', '\\"')
                                wrapped = cpu_prefix + esc + '"'
                            else:
                                wrapped = 'bash -lc "' + inner + '"'
                            res = self.ce.run_background_with_output(
                                wrapped, cwd=str(sub_dir)
                            )
                    else:
                        if affinity_cpus:
                            local_cpus = list(affinity_cpus)

                            def _pre():
                                try:
                                    os.sched_setaffinity(0, set(local_cpus))
                                except Exception:
                                    pass

                            proc = subprocess.Popen(
                                filled,
                                shell=True,
                                cwd=str(sub_dir),
                                stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT,
                                text=True,
                                bufsize=1,
                                preexec_fn=_pre,
                            )
                            job_id = str(uuid.uuid4())
                            try:
                                self.ce._procs[job_id] = proc
                                self.ce._locks[job_id] = threading.Lock()
                            except Exception:
                                pass

                            def _stream2(p, jid, l):
                                prefix = f"[bg:{jid}] "
                                try:
                                    assert p.stdout is not None
                                    for line in p.stdout:
                                        with l:
                                            print(prefix + line.rstrip())
                                except Exception:
                                    pass
                                finally:
                                    if p.stdout:
                                        p.stdout.close()

                            t = threading.Thread(
                                target=_stream2,
                                args=(
                                    proc,
                                    job_id,
                                    self.ce._locks.get(job_id, threading.Lock()),
                                ),
                                daemon=True,
                            )
                            t.start()
                            res = {"job_id": job_id, "pid": proc.pid}
                        else:
                            if cpu_prefix:
                                esc = filled.replace('"', '\\"')
                                wrapped = cpu_prefix + esc + '"'
                                res = self.ce.run_background_with_output(
                                    wrapped, cwd=str(sub_dir)
                                )
                            else:
                                res = self.ce.run_background_with_output(
                                    filled, cwd=str(sub_dir)
                                )

                    jid = res.get("job_id") if isinstance(res, dict) else str(res)
                    pid = res.get("pid") if isinstance(res, dict) else None

                    sub_res["commands"].append(
                        {"cmd": filled, "background": True, "job_id": jid, "pid": pid}
                    )
                    # persist job info
                    self._persist_bg_job(
                        task_root,
                        {
                            "job_id": jid,
                            "pid": pid,
                            "cmd": filled,
                            "mode": "bg_with_output",
                            "started_at": datetime.now(timezone.utc).isoformat(),
                        },
                    )

                # BACKGROUND WITHOUT OUTPUT
                elif mode == "bg_no_output":
                    # if redirect_output requested, perform shell redirection into files
                    if redirect_output:
                        inner = f"{filled} > {out_log} 2> {err_log}"
                        if affinity_cpus:
                            local_cpus = list(affinity_cpus)

                            def _pre_bg():
                                try:
                                    os.setpgrp()
                                except Exception:
                                    pass
                                try:
                                    os.sched_setaffinity(0, set(local_cpus))
                                except Exception:
                                    pass

                            devnull = open(os.devnull, "wb")
                            proc = subprocess.Popen(
                                inner,
                                shell=True,
                                cwd=str(sub_dir),
                                stdout=devnull,
                                stderr=devnull,
                                preexec_fn=_pre_bg,
                            )
                            job_id = str(uuid.uuid4())
                            try:
                                self.ce._procs[job_id] = proc
                            except Exception:
                                pass
                            res = {"job_id": job_id, "pid": proc.pid}
                        else:
                            esc = inner.replace('"', '\\"')
                            if cpu_prefix:
                                wrapped = cpu_prefix + esc + '"'
                            else:
                                wrapped = 'bash -lc "' + inner + '"'
                            res = self.ce.run_background_no_output(
                                wrapped, cwd=str(sub_dir)
                            )
                    else:
                        if affinity_cpus:
                            local_cpus = list(affinity_cpus)

                            def _pre_bg2():
                                try:
                                    os.setpgrp()
                                except Exception:
                                    pass
                                try:
                                    os.sched_setaffinity(0, set(local_cpus))
                                except Exception:
                                    pass

                            devnull = open(os.devnull, "wb")
                            proc = subprocess.Popen(
                                filled,
                                shell=True,
                                cwd=str(sub_dir),
                                stdout=devnull,
                                stderr=devnull,
                                preexec_fn=_pre_bg2,
                            )
                            job_id = str(uuid.uuid4())
                            try:
                                self.ce._procs[job_id] = proc
                            except Exception:
                                pass
                            res = {"job_id": job_id, "pid": proc.pid}
                        else:
                            if cpu_prefix:
                                esc = filled.replace('"', '\\"')
                                wrapped = cpu_prefix + esc + '"'
                                res = self.ce.run_background_no_output(
                                    wrapped, cwd=str(sub_dir)
                                )
                            else:
                                res = self.ce.run_background_no_output(
                                    filled, cwd=str(sub_dir)
                                )

                    jid = res.get("job_id") if isinstance(res, dict) else str(res)
                    pid = res.get("pid") if isinstance(res, dict) else None

                    sub_res["commands"].append(
                        {"cmd": filled, "background": True, "job_id": jid, "pid": pid}
                    )
                    # persist job info
                    self._persist_bg_job(
                        task_root,
                        {
                            "job_id": jid,
                            "pid": pid,
                            "cmd": filled,
                            "mode": "bg_no_output",
                            "started_at": datetime.now(timezone.utc).isoformat(),
                        },
                    )

                else:
                    # unknown mode: treat as foreground
                    r = self.ce.run_foreground(
                        filled, cwd=str(sub_dir), capture_output=capture_output
                    )
                    sub_res["commands"].append(
                        {"cmd": filled, "returncode": r.get("returncode")}
                    )
                    if r.get("returncode", 0) != 0:
                        sub_res["failed"] = True
                        return sub_res

            return sub_res

        # sequential (default) path
        if concurrency <= 1:
            for idx, s in enumerate(cfg.subtasks, start=1):
                sub_res = _exec_subtask(idx, s)
                results["subtasks"].append(sub_res)
                if sub_res.get("failed") and stop_on_fail:
                    # attempt to terminate any background jobs for this task
                    try:
                        self._terminate_bg_jobs(task_root, None)
                    except Exception:
                        pass
                    results["status"] = "failed"
                    return results

            results["status"] = "ok"
            return results

        # parallel path
        stop_event = threading.Event()
        futures: List[tuple[int, str, concurrent.futures.Future]] = []
        with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as ex:
            for idx, s in enumerate(cfg.subtasks, start=1):
                if stop_on_fail and stop_event.is_set():
                    break
                fut = ex.submit(_exec_subtask, idx, s, stop_event)
                futures.append((idx, s.id, fut))

            # collect results
            for idx, sid, fut in futures:
                try:
                    sub_res = fut.result()
                except Exception as e:
                    sub_res = {"id": sid, "failed": True, "error": str(e)}

                results["subtasks"].append(sub_res)
                if sub_res.get("failed"):
                    # signal others to stop scheduling/starting new work
                    if stop_on_fail:
                        stop_event.set()
                        # best-effort terminate recorded background jobs
                        try:
                            self._terminate_bg_jobs(task_root, None)
                        except Exception:
                            pass

            # determine final status
            if any(s.get("failed") for s in results["subtasks"]):
                results["status"] = "failed"
            else:
                results["status"] = "ok"

        return results


def _create_demo_config(path: Path) -> Path:
    demo = {
        "id": "demo_task",
        "comment": "A small demo task with two subtasks",
        "subtasks": [
            {
                "id": "greet",
                "comment": "Echo a greeting and list the input dir",
                "params": {"name": "world"},
                "prepare": ["input"],
                "create_files": [{"path": "input/hello.txt", "content": "demo file"}],
                "commands": [
                    {"cmd": "echo Hello {name}", "mode": "foreground"},
                    {
                        "cmd": "ls -la input",
                        "mode": "bg_with_output",
                        "redirect_output": True,
                    },
                ],
            },
            {
                "id": "inspect",
                "comment": "Print working dir and sleep briefly",
                "commands": [
                    {"cmd": "pwd", "mode": "foreground"},
                    {"cmd": "sleep 0.05", "mode": "bg_no_output"},
                    {"cmd": "echo done", "mode": "foreground"},
                ],
            },
        ],
    }
    path.write_text(json.dumps(demo, indent=2))
    return path


def _parse_args():
    p = argparse.ArgumentParser(description="Create and run tasks from JSON configs")
    p.add_argument("--config", type=Path, help="Path to task JSON config")
    p.add_argument(
        "--tasks-root",
        type=Path,
        default=Path("./tasks_out"),
        help="Base dir to create task folders",
    )
    p.add_argument(
        "--dry-run",
        action="store_true",
        help="Only prepare directories/files, do not run commands",
    )
    p.add_argument(
        "--demo",
        action="store_true",
        help="Create and run a demo task in the tasks-root",
    )
    p.add_argument(
        "--no-capture",
        dest="capture",
        action="store_false",
        help="Do not capture command output (stream to terminal)",
    )
    p.add_argument(
        "--concurrency",
        type=int,
        default=1,
        help="Number of subtasks to run in parallel (default 1)",
    )
    p.add_argument(
        "--stop-on-fail",
        dest="stop_on_fail",
        action="store_true",
        help="Stop scheduling new subtasks when one fails (best-effort in parallel mode)",
    )
    p.add_argument(
        "--bg-action",
        type=str,
        choices=["status", "kill", "kill-all"],
        help="Query or control background jobs for a created task (status/kill/kill-all)",
    )
    p.add_argument(
        "--bg-job-ids",
        type=str,
        help="Comma-separated job ids to operate on when using --bg-action kill",
    )
    p.add_argument(
        "--task-dir",
        type=Path,
        help="Path to an existing task root (used with --bg-action)",
    )
    return p.parse_args()


def main():
    args = _parse_args()
    te = TaskExecutor()
    tasks_root = Path(args.tasks_root).absolute()
    tasks_root.mkdir(parents=True, exist_ok=True)

    # If user requested bg-action, handle it immediately (no need for --config/--demo)
    if getattr(args, "bg_action", None):
        if not getattr(args, "task_dir", None):
            print("--task-dir is required when using --bg-action")
            sys.exit(2)
        td = Path(args.task_dir)
        if not td.exists():
            print(f"Task dir not found: {td}")
            sys.exit(2)

        if args.bg_action == "status":
            jobs = te.list_bg_jobs(td)
            print(json.dumps(jobs, indent=2))
            return
        elif args.bg_action == "kill":
            if not args.bg_job_ids:
                print("Provide --bg-job-ids with comma-separated ids for kill")
                sys.exit(2)
            ids = [x.strip() for x in args.bg_job_ids.split(",") if x.strip()]
            res = te._terminate_bg_jobs(td, ids)
            print(json.dumps(res, indent=2))
            return
        elif args.bg_action == "kill-all":
            res = te._terminate_bg_jobs(td, None)
            print(json.dumps(res, indent=2))
            return

    if args.demo:
        cfg_path = tasks_root / "demo_config.json"
        _create_demo_config(cfg_path)
    elif args.config:
        cfg_path = Path(args.config).absolute()
        if not cfg_path.exists():
            print(f"Config not found: {cfg_path}")
            sys.exit(2)
    else:
        print("Provide --config or --demo")
        sys.exit(2)

    task_root = te.create_task(cfg_path, tasks_root)
    print(f"Created task at: {task_root}")

    # handle background job actions if requested
    if getattr(args, "bg_action", None):
        if not getattr(args, "task_dir", None):
            print("--task-dir is required when using --bg-action")
            sys.exit(2)
        td = Path(args.task_dir)
        if not td.exists():
            print(f"Task dir not found: {td}")
            sys.exit(2)

        if args.bg_action == "status":
            jobs = te.list_bg_jobs(td)
            print(json.dumps(jobs, indent=2))
            return
        elif args.bg_action == "kill":
            if not args.bg_job_ids:
                print("Provide --bg-job-ids with comma-separated ids for kill")
                sys.exit(2)
            ids = [x.strip() for x in args.bg_job_ids.split(",") if x.strip()]
            res = te._terminate_bg_jobs(td, ids)
            print(json.dumps(res, indent=2))
            return
        elif args.bg_action == "kill-all":
            res = te._terminate_bg_jobs(td, None)
            print(json.dumps(res, indent=2))
            return

    if args.dry_run:
        print("Dry-run: prepared files and folders, not executing commands")
        return

    res = te.run_task(
        task_root,
        capture_output=args.capture,
        concurrency=getattr(args, "concurrency", 1),
    )
    print("Result:", res)


if __name__ == "__main__":
    main()
