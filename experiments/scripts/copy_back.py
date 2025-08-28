#!/usr/bin/env python3
"""
Interactive copy-back tool.

Features:
- List remote task directories under a remote base path.
- Detect which tasks are already copied locally (by presence of local tar.gz).
- Allow interactive selection of tasks to copy back.
- For each selected task:
  - remove stderr.log files on remote task dir
  - create a tar.gz mirror on the remote host
  - scp the tarball to local destination
  - optionally remove the remote tarball after successful copy

Usage (examples):
  python3 copy_back.py --host remote.example.com --user alice --remote-base /data/tasks --local-dest ~/backups
  python3 copy_back.py -H host -u user -r /path/to/tasks -l ./downloads --ssh-key ~/.ssh/id_rsa --remove-remote

Notes:
- This script uses the system 'ssh' and 'scp' commands (no extra Python deps).
- Ensure you have key-based access or ssh-agent configured for passwordless runs when needed.
"""

from __future__ import annotations

import argparse
import getpass
import os
import shlex
import subprocess
import sys
from typing import List, Tuple


def run_cmd(cmd: List[str], capture_output=False, check=False) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, stdout=subprocess.PIPE if capture_output else None,
                          stderr=subprocess.STDOUT if capture_output else None,
                          text=True, check=check)


def ssh_run(user: str, host: str, remote_cmd: str, port: int = None, ssh_key: str = None) -> Tuple[int, str]:
    ssh_parts = ["ssh"]
    if ssh_key:
        ssh_parts += ["-i", ssh_key]
    if port:
        ssh_parts += ["-p", str(port)]
    # If user is provided, use user@host, otherwise let ssh resolve the host and user via ~/.ssh/config
    if user:
        ssh_parts.append(f"{user}@{host}")
    else:
        ssh_parts.append(host)
    ssh_parts.append(remote_cmd)
    try:
        p = run_cmd(ssh_parts, capture_output=True)
        return p.returncode, p.stdout or ""
    except Exception as e:
        return 255, str(e)


def scp_get(user: str, host: str, remote_path: str, local_path: str, port: int = None, ssh_key: str = None) -> Tuple[int, str]:
    scp_parts = ["scp", "-q"]
    if ssh_key:
        scp_parts += ["-i", ssh_key]
    if port:
        scp_parts += ["-P", str(port)]
    if user:
        remote_spec = f"{user}@{host}:{remote_path}"
    else:
        remote_spec = f"{host}:{remote_path}"
    scp_parts += [remote_spec, local_path]
    try:
        p = run_cmd(scp_parts, capture_output=True)
        return p.returncode, p.stdout or ""
    except Exception as e:
        return 255, str(e)


def list_remote_tasks(user: str, host: str, remote_base: str, port: int = None, ssh_key: str = None) -> List[str]:
    # A portable way to list immediate subdirectories remotely.
    # It prints one directory name per line.
    remote_cmd = (
        "bash -lc " +
        shlex.quote(
            f"if [ -d {shlex.quote(remote_base)} ]; then "
            f"for d in {shlex.quote(remote_base)}/*; do [ -d \"$d\" ] && basename \"$d\"; done; "
            "else echo; fi"
        )
    )
    rc, out = ssh_run(user, host, remote_cmd, port=port, ssh_key=ssh_key)
    # If ssh printed directory names, accept them even if remote exit code was non-zero
    tasks = [line.strip() for line in (out or "").splitlines() if line.strip()]
    if not tasks and rc != 0:
        # no useful stdout and non-zero return -> error
        raise RuntimeError(f"ssh list failed: {out.strip()}")
    return tasks


def filter_uncopied(tasks: List[str], local_dest: str) -> List[str]:
    uncopied = []
    for t in tasks:
        candidates = [
            os.path.join(local_dest, f"{t}_mirror.tar.gz"),
            os.path.join(local_dest, f"{t}.tar.gz"),
        ]
        if not any(os.path.exists(c) for c in candidates):
            uncopied.append(t)
    return uncopied


def delete_stderr_on_remote(user: str, host: str, remote_base: str, task: str, port: int = None, ssh_key: str = None) -> Tuple[bool, str]:
    remote_dir = os.path.join(remote_base, task)
    # find and delete stderr.log files
    cmd = f"bash -lc {shlex.quote(f'find {remote_dir} -type f -name stderr.log -delete')}"
    rc, out = ssh_run(user, host, cmd, port=port, ssh_key=ssh_key)
    return rc == 0, out


def create_remote_tar(user: str, host: str, remote_base: str, task: str, port: int = None, ssh_key: str = None) -> Tuple[bool, str, str]:
    remote_parent = remote_base.rstrip("/")
    tar_name = f"{task}_mirror.tar.gz"
    remote_tar_path = os.path.join(remote_parent, tar_name)
    # create tar.gz in the parent directory: tar -czf /parent/task_mirror.tar.gz -C /parent task
    cmd = f"bash -lc {shlex.quote(f'tar -czf {remote_tar_path} -C {remote_parent} {shlex.quote(task)}')}"
    rc, out = ssh_run(user, host, cmd, port=port, ssh_key=ssh_key)
    return rc == 0, out, remote_tar_path


def remove_remote_file(user: str, host: str, remote_path: str, port: int = None, ssh_key: str = None) -> Tuple[bool, str]:
    cmd = f"bash -lc {shlex.quote(f'rm -f {remote_path}')}"
    rc, out = ssh_run(user, host, cmd, port=port, ssh_key=ssh_key)
    return rc == 0, out


def prompt_selection(items: List[str]) -> List[str]:
    print("选择要回传的任务（输入序号，逗号分隔；输入 'a' 选择全部；直接回车取消）：")
    for i, it in enumerate(items, start=1):
        print(f"  {i}) {it}")
    s = input("选择: ").strip()
    if not s:
        return []
    if s.lower() == "a":
        return items
    chosen = []
    parts = s.split(",")
    for p in parts:
        p = p.strip()
        if not p:
            continue
        if "-" in p:
            try:
                a, b = p.split("-", 1)
                ia, ib = int(a), int(b)
                for idx in range(ia, ib + 1):
                    if 1 <= idx <= len(items):
                        chosen.append(items[idx - 1])
            except Exception:
                continue
        else:
            try:
                idx = int(p)
                if 1 <= idx <= len(items):
                    chosen.append(items[idx - 1])
            except Exception:
                continue
    # dedupe while preserving order
    seen = set()
    out = []
    for x in chosen:
        if x not in seen:
            seen.add(x)
            out.append(x)
    return out


def ensure_local_dir(path: str) -> None:
    os.makedirs(path, exist_ok=True)


def main():
    parser = argparse.ArgumentParser(description="Copy back remote task directories as tar.gz mirror archives.")
    parser.add_argument("--host", "-H", required=True, help="Remote SSH host (can be an entry from ~/.ssh/config)")
    parser.add_argument("--user", "-u", default=None, help="SSH user (optional). If omitted, ssh will use ~/.ssh/config or the default user)")
    parser.add_argument("--remote-base", "-r", required=True, help="Remote base directory containing task subdirs")
    parser.add_argument("--local-dest", "-l", required=True, help="Local directory to save downloaded archives")
    parser.add_argument("--ssh-key", help="Path to SSH private key (optional)")
    parser.add_argument("--port", "-p", type=int, help="SSH port (optional)")
    parser.add_argument("--remove-remote", action="store_true", help="Remove remote tar after successful download")
    args = parser.parse_args()

    try:
        tasks = list_remote_tasks(args.user, args.host, args.remote_base, port=args.port, ssh_key=args.ssh_key)
    except Exception as e:
        print(f"无法列出远端任务: {e}", file=sys.stderr)
        sys.exit(2)

    if not tasks:
        print("远端没有检测到子任务目录。")
        sys.exit(0)

    ensure_local_dir(args.local_dest)

    uncopied = filter_uncopied(tasks, args.local_dest)
    if not uncopied:
        print("没有未回传的数据（本地已有对应 tar.gz）。")
        sys.exit(0)

    print("以下是未回传的数据：")
    for t in uncopied:
        print("  -", t)

    chosen = prompt_selection(uncopied)
    if not chosen:
        print("未选择任何任务，退出。")
        return

    for task in chosen:
        print(f"\n处理任务: {task}")
        ok, out = delete_stderr_on_remote(args.user, args.host, args.remote_base, task, port=args.port, ssh_key=args.ssh_key)
        if ok:
            print("  已删除远端 stderr.log（如果存在）")
        else:
            print(f"  删除 stderr.log 失败: {out.strip()}", file=sys.stderr)

        ok, out, remote_tar = create_remote_tar(args.user, args.host, args.remote_base, task, port=args.port, ssh_key=args.ssh_key)
        if not ok:
            print(f"  远端打包失败: {out.strip()}", file=sys.stderr)
            continue
        print(f"  远端已创建镜像归档: {remote_tar}")

        # Download via scp
        rc, scp_out = scp_get(args.user, args.host, remote_tar, args.local_dest, port=args.port, ssh_key=args.ssh_key)
        if rc != 0:
            print(f"  scp 下载失败: {scp_out.strip()}", file=sys.stderr)
            continue
        print(f"  已下载到本地: {os.path.join(args.local_dest, os.path.basename(remote_tar))}")

        if args.remove_remote:
            ok, rem_out = remove_remote_file(args.user, args.host, remote_tar, port=args.port, ssh_key=args.ssh_key)
            if ok:
                print("  已删除远端打包文件")
            else:
                print(f"  无法删除远端打包文件: {rem_out.strip()}", file=sys.stderr)

    print("\n处理完成。")


if __name__ == "__main__":
    main()
