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


def parse_duration(s: Optional[str]) -> Optional[float]:
    """Parse human-friendly duration strings like "24h", "90m", "1h30m10s" into seconds.
    Returns None when s is None.
    """
    if s is None:
        return None
    s = str(s).strip()
    total = 0.0
    num = ""
    unit = ""
    for ch in s:
        if ch.isdigit():
            if unit:
                # flush previous
                if unit in ("h", "hr", "hrs"):
                    total += float(num) * 3600
                elif unit in ("m", "min", "mins"):
                    total += float(num) * 60
                elif unit in ("s", "sec", "secs"):
                    total += float(num)
                else:
                    total += float(num)
                num = ch
                unit = ""
            else:
                num += ch
        else:
            unit += ch.lower()
    if num:
        if unit in ("h", "hr", "hrs"):
            total += float(num) * 3600
        elif unit in ("m", "min", "mins"):
            total += float(num) * 60
        elif unit in ("s", "sec", "secs", ""):
            total += float(num)
        else:
            total += float(num)
    return total


try:
    # when run from repo root
    from command_executor import CommandExecutor
except Exception:
    # package-style import when executed from scripts folder
    from experiments.scripts.command_executor import CommandExecutor  # type: ignore


try:
    # attempt local import when running from scripts folder
    from experiments.scripts.cpu_assessor import CPUAssessor  # type: ignore

    print("Successfully imported CPUAssessor from local module")
except Exception:
    try:
        # try importing as a sibling module when executed from repo root
        from cpu_assessor import CPUAssessor  # type: ignore

        print("Successfully imported CPUAssessor from sibling module")
    except Exception:
        print("Warning: cpu_assessor not found; CPU affinity features disabled")
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
        # attempt to enrich entry with pgid when possible so external watchers
        # can terminate entire process groups (more reliable cleanup)
        try:
            pid = entry.get("pid")
            if pid is not None:
                try:
                    pgid = os.getpgid(int(pid))
                    entry.setdefault("pgid", pgid)
                except Exception:
                    # ignore if pgid cannot be determined
                    pass
        except Exception:
            pass
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

        results: Dict[str, Any] = {"terminated": [], "failed": [], "details": []}
        term_log = task_root / "terminate.log"

        def _log_line(line: str) -> None:
            try:
                term_log.write_text(
                    (term_log.read_text() if term_log.exists() else "") + line + "\n"
                )
            except Exception:
                pass

        for j in targets:
            jid = j.get("job_id")
            pid = j.get("pid")
            attempt_info: Dict[str, Any] = {"job_id": jid, "pid": pid, "attempts": []}
            ok = False

            # try using in-memory CommandExecutor if available
            try:
                if jid and hasattr(self.ce, "is_running") and self.ce.is_running(jid):
                    attempt_info["attempts"].append("ce.terminate")
                    try:
                        self.ce.terminate(jid)
                        time.sleep(0.1)
                        if self.ce.is_running(jid):
                            attempt_info["attempts"].append("ce.kill")
                            self.ce.kill(jid)
                        ok = True
                    except Exception as e:
                        attempt_info["attempts"].append(f"ce.error:{e}")
                        ok = False
            except Exception as e:
                attempt_info["attempts"].append(f"ce.lookup_error:{e}")
                ok = False

            # fallback to killing by pid / process group
            if not ok and pid:
                try:
                    p_int = int(pid)
                    # attempt to get process group id and kill group (best-effort)
                    try:
                        pgid = os.getpgid(p_int)
                        attempt_info["attempts"].append(f"killpg:{pgid}")
                        try:
                            os.killpg(pgid, signal.SIGTERM)
                        except Exception as e:
                            attempt_info["attempts"].append(f"killpg_term_err:{e}")
                        time.sleep(0.1)
                        # check if process still exists
                        try:
                            os.kill(p_int, 0)
                            # still exists, escalate
                            try:
                                os.killpg(pgid, signal.SIGKILL)
                                attempt_info["attempts"].append(f"killpg_kill:{pgid}")
                            except Exception as e:
                                attempt_info["attempts"].append(f"killpg_kill_err:{e}")
                        except Exception:
                            # process not found
                            pass
                        ok = True
                    except Exception as e:
                        # couldn't get pgid, try kill pid directly
                        attempt_info["attempts"].append(f"killpid:{p_int}")
                        try:
                            os.kill(p_int, signal.SIGTERM)
                            time.sleep(0.1)
                            try:
                                os.kill(p_int, 0)
                                os.kill(p_int, signal.SIGKILL)
                                attempt_info["attempts"].append("killpid_kill")
                            except Exception:
                                pass
                            ok = True
                        except Exception as e2:
                            attempt_info["attempts"].append(f"killpid_err:{e2}")
                            ok = False
                except Exception as e:
                    attempt_info["attempts"].append(f"pid_parse_err:{e}")
                    ok = False

                # If kill attempts didn't work, try to kill descendant processes found via /proc (Linux)
                if not ok and pid:
                    try:
                        p_int = int(pid)
                        descendants = []
                        proc_base = Path("/proc")
                        if proc_base.exists():
                            # gather all processes where ancestor chain includes p_int
                            for entry in proc_base.iterdir():
                                if not entry.name.isdigit():
                                    continue
                                try:
                                    status = entry / "status"
                                    if not status.exists():
                                        continue
                                    txt = status.read_text()
                                    # quick parse PPid line
                                    for line in txt.splitlines():
                                        if line.startswith("PPid:"):
                                            parts = line.split()
                                            if len(parts) >= 2 and parts[1].isdigit():
                                                ppid = int(parts[1])
                                            else:
                                                ppid = 0
                                            break
                                    else:
                                        ppid = 0
                                    # walk up to see if ancestor chain reaches p_int
                                    cur = ppid
                                    seen = set()
                                    chain_ok = False
                                    while cur and cur not in seen:
                                        seen.add(cur)
                                        if cur == p_int:
                                            chain_ok = True
                                            break
                                        # read parent's status
                                        par_status = Path(f"/proc/{cur}/status")
                                        if not par_status.exists():
                                            break
                                        par_txt = par_status.read_text()
                                        new_pp = 0
                                        for l in par_txt.splitlines():
                                            if l.startswith("PPid:"):
                                                parts = l.split()
                                                if (
                                                    len(parts) >= 2
                                                    and parts[1].isdigit()
                                                ):
                                                    new_pp = int(parts[1])
                                                break
                                        cur = new_pp
                                    if chain_ok:
                                        descendants.append(int(entry.name))
                                except Exception:
                                    continue
                        if descendants:
                            attempt_info["attempts"].append(
                                f"descendants:{descendants}"
                            )
                            for dpid in descendants:
                                try:
                                    os.kill(dpid, signal.SIGTERM)
                                except Exception:
                                    pass
                            time.sleep(0.1)
                            for dpid in descendants:
                                try:
                                    os.kill(dpid, 0)
                                    try:
                                        os.kill(dpid, signal.SIGKILL)
                                    except Exception:
                                        pass
                                except Exception:
                                    # no longer exists
                                    pass
                            ok = True
                    except Exception as e:
                        attempt_info["attempts"].append(f"descendants_err:{e}")
                        # leave ok as-is

            if ok:
                results["terminated"].append(jid or pid)
            else:
                results["failed"].append(jid or pid)

            attempt_info["ok"] = ok
            results["details"].append(attempt_info)

            # append detail to terminate.log for post-mortem inspection
            try:
                _log_line(
                    f"[{datetime.now(timezone.utc).isoformat()}] terminate attempt: {json.dumps(attempt_info)}"
                )
            except Exception:
                pass

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
        total_duration: Optional[float] = None,  # seconds, None means no limit
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

        # Prepare CPU allocation sources. If CPUAssessor is available, take a
        # snapshot once per run and allocate from its `allowed_free` pool while
        # respecting `affinity_allowed` and avoiding CPUs already allocated in
        # this run (in _allocated_cpus). If not available, fall back to the
        # legacy in-process allocator.
        total_cpus = os.cpu_count() or 1
        _allocated_cpus: set = set()
        _alloc_lock = threading.Lock()

        assessor_report = None
        allowed_free_pool: List[int] = []
        affinity_allowed_set = set()
        affinity_reserved_set = set()
        if CPUAssessor is not None:
            try:
                assessor = CPUAssessor()
                assessor_report = assessor.assess()
                allowed_free_pool = list(assessor_report.get("allowed_free", []))
                affinity_allowed_set = set(assessor_report.get("affinity_allowed", []))
                affinity_reserved_set = set(
                    assessor_report.get("affinity_reserved", [])
                )
            except Exception:
                assessor_report = None

        # Require CPUAssessor to be available and to produce a report. If we
        # cannot get an assessor snapshot, we must abort — this script no
        # longer supports a local fallback allocator.
        if assessor_report is None:
            print(
                "Error: CPUAssessor unavailable or failed to produce a report; CPU allocation requires CPUAssessor."
            )
            sys.exit(2)

        def _allocate_cpus(n: int):
            with _alloc_lock:
                if assessor_report is not None:
                    # pick from allowed_free_pool excluding already allocated
                    free = [i for i in allowed_free_pool if i not in _allocated_cpus]
                else:
                    free = [i for i in range(total_cpus) if i not in _allocated_cpus]
                if len(free) < n:
                    return None
                chosen = free[:n]
                _allocated_cpus.update(chosen)
                return chosen

        def _parse_cpu_spec(spec: str) -> List[int]:
            """Parse a taskset-style spec like "0,2-4" into a list of ints.
            Returns empty list on parse failure.
            """
            out: List[int] = []
            try:
                for part in str(spec).split(","):
                    part = part.strip()
                    if not part:
                        continue
                    if "-" in part:
                        a, b = part.split("-", 1)
                        a_i = int(a)
                        b_i = int(b)
                        if b_i < a_i:
                            continue
                        out.extend(list(range(a_i, b_i + 1)))
                    else:
                        out.append(int(part))
            except Exception:
                return []
            return out

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
                    # integer request: prefer assessor snapshot allocation
                    if isinstance(s.cpus, int):
                        n = int(s.cpus)
                        if assessor_report is not None:
                            # quick check: are there enough allowed_free in snapshot?
                            if len(allowed_free_pool) < n:
                                sub_res["failed"] = True
                                sub_res["error"] = (
                                    f"Requested {n} free CPU(s), but only {len(allowed_free_pool)} available."
                                    f" Assessor reason: {assessor_report.get('reason')}."
                                )
                                return sub_res
                            # attempt to allocate from the shared pool for this run
                            chosen = _allocate_cpus(n)
                            if chosen is None:
                                sub_res["failed"] = True
                                sub_res["error"] = (
                                    f"Requested {n} CPU(s) but allocation conflict occurred; try with lower concurrency."
                                )
                                return sub_res
                            affinity_cpus = [int(x) for x in chosen]
                            spec = ",".join(str(x) for x in chosen)
                        else:
                            # This branch must not be used: CPUAssessor is
                            # required by policy. Fail the subtask if reached.
                            sub_res["failed"] = True
                            sub_res["error"] = (
                                "CPUAssessor required for CPU allocation but unavailable during allocation step."
                            )
                            return sub_res
                    elif isinstance(s.cpus, (list, tuple)):
                        # explicit list: validate against affinity_allowed and existing allocations
                        requested = [int(x) for x in s.cpus]
                        # if assessor snapshot exists, ensure all requested CPUs are allowed
                        if assessor_report is not None:
                            bad = [
                                c for c in requested if c not in affinity_allowed_set
                            ]
                            if bad:
                                sub_res["failed"] = True
                                sub_res["error"] = (
                                    f"Requested CPUs {requested} include CPUs not allowed by system affinity: {bad}."
                                )
                                return sub_res
                            # ensure not already allocated in this run
                            busy = [c for c in requested if c in _allocated_cpus]
                            if busy:
                                sub_res["failed"] = True
                                sub_res["error"] = (
                                    f"Requested CPUs {requested} include already allocated CPUs in this run: {busy}."
                                )
                                return sub_res
                        # reserve these for this run
                        with _alloc_lock:
                            for c in requested:
                                _allocated_cpus.add(c)
                        affinity_cpus = requested
                        spec = ",".join(str(x) for x in requested)
                    else:
                        # string spec: try to parse, otherwise leave for taskset to handle
                        parsed = _parse_cpu_spec(str(s.cpus))
                        if parsed:
                            # validate parsed list similar to above
                            if assessor_report is not None:
                                bad = [
                                    c for c in parsed if c not in affinity_allowed_set
                                ]
                                if bad:
                                    sub_res["failed"] = True
                                    sub_res["error"] = (
                                        f"Requested CPUs {parsed} include CPUs not allowed by system affinity: {bad}."
                                    )
                                    return sub_res
                                busy = [c for c in parsed if c in _allocated_cpus]
                                if busy:
                                    sub_res["failed"] = True
                                    sub_res["error"] = (
                                        f"Requested CPUs {parsed} include already allocated CPUs in this run: {busy}."
                                    )
                                    return sub_res
                            with _alloc_lock:
                                for c in parsed:
                                    _allocated_cpus.add(c)
                            affinity_cpus = parsed
                            spec = ",".join(str(x) for x in parsed)
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

        # create a stop event and a watcher thread if total_duration is provided
        stop_event = threading.Event()
        timer_thread: Optional[threading.Thread] = None
        timed_out = False
        # If caller didn't pass total_duration explicitly, attempt to read
        # a "duration" field from the task config JSON in the task root.
        if total_duration is None:
            try:
                raw_cfg = json.loads(cfg_file.read_text())
                cfg_dur = raw_cfg.get("duration") or raw_cfg.get("total_duration")
                parsed = parse_duration(cfg_dur)
                if parsed is not None:
                    total_duration = parsed
                    print(
                        f"[TASK_MONITOR] duration read from config: {cfg_dur} -> {total_duration}s"
                    )
            except Exception:
                # ignore parse errors and proceed without a limit
                pass

        if total_duration is not None:

            def _watcher():
                nonlocal timed_out
                # setup end time and logging cadence
                end = time.time() + float(total_duration)
                start_ts = datetime.now(timezone.utc).isoformat()
                print(
                    f"[TASK_MONITOR] watcher started at {start_ts}, total_duration={total_duration}s, will end at {datetime.fromtimestamp(end, timezone.utc).isoformat()}"
                )
                # choose a log interval: every 60s for long runs, otherwise every 5s
                log_interval = 60 if total_duration > 120 else 5
                next_log = time.time() + min(log_interval, max(1, total_duration / 10))
                try:
                    while time.time() < end and not stop_event.is_set():
                        now = time.time()
                        if now >= next_log:
                            remaining = max(0, int(end - now))
                            print(f"[TASK_MONITOR] time remaining: {remaining}s")
                            next_log = now + log_interval
                        time.sleep(0.5)
                except Exception as e:
                    print(f"[TASK_MONITOR] watcher encountered error: {e}")
                if not stop_event.is_set():
                    # time exceeded
                    timed_out = True
                    print(
                        f"[TASK_MONITOR] total duration exceeded ({total_duration}s). Initiating shutdown and terminating bg jobs..."
                    )
                    stop_event.set()
                    try:
                        term_res = self._terminate_bg_jobs(task_root, None)
                        print(f"[TASK_MONITOR] terminate_bg_jobs result: {term_res}")
                    except Exception as e:
                        print(f"[TASK_MONITOR] terminate_bg_jobs failed: {e}")

            # make watcher non-daemon so it can finish termination work even if
            # the main thread would otherwise exit; we'll join it before
            # returning from run_task to ensure bg processes are cleaned up.
            timer_thread = threading.Thread(target=_watcher)
            timer_thread.start()

        # sequential (default) path
        if concurrency <= 1:
            for idx, s in enumerate(cfg.subtasks, start=1):
                if stop_event.is_set():
                    results["status"] = "timeout" if timed_out else "stopped"
                    # signal watcher to exit early and wait for it to finish cleanup
                    try:
                        stop_event.set()
                        if timer_thread is not None:
                            print(
                                "[TASK_MONITOR] main waiting for watcher to finish cleanup..."
                            )
                            timer_thread.join(timeout=5)
                    except Exception:
                        pass
                    return results
                sub_res = _exec_subtask(idx, s, stop_event)
                results["subtasks"].append(sub_res)
                if sub_res.get("failed") and stop_on_fail:
                    # attempt to terminate any background jobs for this task
                    try:
                        self._terminate_bg_jobs(task_root, None)
                    except Exception:
                        pass
                    results["status"] = "failed"
                    # ensure watcher exits
                    try:
                        stop_event.set()
                        if timer_thread is not None:
                            timer_thread.join(timeout=5)
                    except Exception:
                        pass
                    return results

            results["status"] = "timeout" if timed_out else "ok"
            # normal completion: signal watcher to exit early and wait briefly
            try:
                stop_event.set()
                if timer_thread is not None:
                    print(
                        "[TASK_MONITOR] main waiting for watcher to exit after normal completion..."
                    )
                    timer_thread.join(timeout=5)
            except Exception:
                pass
            return results

        # parallel path (reuse the stop_event created above so the watcher
        # thread can signal timeout for both sequential and parallel flows)
        futures: List[tuple[int, str, concurrent.futures.Future]] = []
        with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as ex:
            for idx, s in enumerate(cfg.subtasks, start=1):
                # stop scheduling new work if a global stop_event was triggered
                # (e.g. due to total_duration timeout) or if we've been asked to
                # stop on failure and a failure already signalled the stop_event.
                if stop_event.is_set():
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

            # determine final status (prefer timeout over other states)
            if timed_out:
                results["status"] = "timeout"
            elif any(s.get("failed") for s in results["subtasks"]):
                results["status"] = "failed"
            else:
                results["status"] = "ok"

            # signal watcher to exit and wait for its cleanup (bg jobs termination)
            try:
                stop_event.set()
                if timer_thread is not None:
                    print(
                        "[TASK_MONITOR] main waiting for watcher to finish cleanup (parallel)..."
                    )
                    timer_thread.join(timeout=5)
            except Exception:
                pass

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
    p.add_argument(
        "--duration",
        type=str,
        help="Total duration to run the task (e.g. 24h, 3600s, 1h30m). When reached, stops executing new work and attempts graceful shutdown",
    )
    p.add_argument(
        "--detach-monitor",
        action="store_true",
        help="Detach a separate monitor process that will enforce the task duration and terminate background jobs after timeout; the main program will exit after creating the task.",
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

    # If user requested a detached monitor, write a small monitor.py into the
    # task root and spawn it as a separate process group so the main program
    # can exit while the monitor enforces duration and cleans up bg jobs.
    if getattr(args, "detach_monitor", False):
        monitor_path = task_root / "monitor.py"
        monitor_code = """#!/usr/bin/env python3
import json
import os
import time
import signal
from pathlib import Path
from datetime import datetime, timezone

def parse_duration(s):
    if s is None:
        return None
    s = str(s).strip()
    total = 0.0
    num = ""
    unit = ""
    for ch in s:
        if ch.isdigit():
            if unit:
                if unit in ("h","hr","hrs"):
                    total += float(num) * 3600
                elif unit in ("m","min","mins"):
                    total += float(num) * 60
                elif unit in ("s","sec","secs"):
                    total += float(num)
                else:
                    total += float(num)
                num = ch
                unit = ""
            else:
                num += ch
        else:
            unit += ch.lower()
    if num:
        if unit in ("h","hr","hrs"):
            total += float(num) * 3600
        elif unit in ("m","min","mins"):
            total += float(num) * 60
        else:
            total += float(num)
    return total

def terminate_jobs(task_root: Path):
    p = task_root / 'bg_jobs.json'
    out = []
    if not p.exists():
        print(f"[DETACHED_MONITOR] no bg_jobs.json at {p}")
        return
    try:
        jobs = json.loads(p.read_text())
    except Exception as e:
        print(f"[DETACHED_MONITOR] failed to parse bg_jobs.json: {e}")
        return

    for j in jobs:
        pid = j.get('pid')
        pgid = j.get('pgid')
        jid = j.get('job_id')
        try:
            if pgid:
                print(f"[DETACHED_MONITOR] killing pgid {pgid} for job {jid}")
                try:
                    os.killpg(int(pgid), signal.SIGTERM)
                except Exception:
                    os.killpg(int(pgid), signal.SIGKILL)
            elif pid:
                print(f"[DETACHED_MONITOR] killing pid {pid} for job {jid}")
                try:
                    os.kill(int(pid), signal.SIGTERM)
                except Exception:
                    try:
                        os.kill(int(pid), signal.SIGKILL)
                    except Exception:
                        pass
        except Exception as e:
            print(f"[DETACHED_MONITOR] failed to kill job {jid}: {e}")

def main():
    task_root = Path(__file__).parent
    # find config
    cfg_file = None
    for p in task_root.iterdir():
        if p.suffix == '.json':
            cfg_file = p
            break
    dur = None
    if cfg_file is not None:
        try:
            raw = json.loads(cfg_file.read_text())
            cfg_dur = raw.get('duration') or raw.get('total_duration')
            parsed = parse_duration(cfg_dur)
            if parsed is not None:
                dur = parsed
                print(f"[DETACHED_MONITOR] duration read from config: {cfg_dur} -> {dur}s")
        except Exception as e:
            print(f"[DETACHED_MONITOR] failed to read config duration: {e}")

    if dur is None:
        print("[DETACHED_MONITOR] no duration found; exiting")
        return

    end = time.time() + float(dur)
    print(f"[DETACHED_MONITOR] started at {datetime.now(timezone.utc).isoformat()}, will end at {datetime.fromtimestamp(end, timezone.utc).isoformat()}")
    try:
        while time.time() < end:
            time.sleep(1)
    except KeyboardInterrupt:
        pass

    print(f"[DETACHED_MONITOR] duration reached; terminating bg jobs in {task_root}")
    terminate_jobs(task_root)

if __name__ == '__main__':
    main()
"""
        try:
            monitor_path.write_text(monitor_code)
            monitor_path.chmod(0o755)
        except Exception as e:
            print(f"Failed to write monitor script: {e}")
        # spawn detached monitor in its own process group; redirect output to monitor.log
        try:
            logf = open(task_root / "monitor.log", "a")
            subprocess.Popen(
                [sys.executable, str(monitor_path)],
                cwd=str(task_root),
                stdout=logf,
                stderr=logf,
                preexec_fn=os.setpgrp,
            )
            print(
                f"Detached monitor started at {monitor_path}; main process will exit now."
            )
        except Exception as e:
            print(f"Failed to start detached monitor: {e}")
            # fall through and continue running normally

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

    def _parse_duration(s: Optional[str]) -> Optional[float]:
        if s is None:
            return None
        s = s.strip()
        # support forms like "24h", "90m", "3600s", "1h30m", "1h30m10s"
        total = 0.0
        num = ""
        unit = ""
        for ch in s:
            if ch.isdigit():
                if unit:
                    # flush previous
                    if unit in ("h", "hr", "hrs"):
                        total += float(num) * 3600
                    elif unit in ("m", "min", "mins"):
                        total += float(num) * 60
                    elif unit in ("s", "sec", "secs"):
                        total += float(num)
                    else:
                        # unknown unit, try seconds
                        total += float(num)
                    num = ch
                    unit = ""
                else:
                    num += ch
            else:
                unit += ch.lower()
        if num:
            # flush trailing number
            if unit in ("h", "hr", "hrs"):
                total += float(num) * 3600
            elif unit in ("m", "min", "mins"):
                total += float(num) * 60
            elif unit in ("s", "sec", "secs", ""):
                total += float(num)
            else:
                total += float(num)
        return total

    dur = _parse_duration(getattr(args, "duration", None))

    res = te.run_task(
        task_root,
        capture_output=args.capture,
        concurrency=getattr(args, "concurrency", 1),
        total_duration=dur,
    )
    print("Result:", res)


if __name__ == "__main__":
    main()
