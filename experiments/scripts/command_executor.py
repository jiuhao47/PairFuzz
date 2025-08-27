import os
import shlex
import subprocess
import threading
import uuid
from typing import Any, Dict, Optional, Tuple, Union


class CommandExecutor:
    """Execute shell commands in different modes:

    - run_foreground: run command in foreground (blocking), optionally capturing output
    - run_background_with_output: start command in background and stream its output to this process's stdout
    - run_background_no_output: start command in background with stdout/stderr redirected to /dev/null

    Additional utilities: terminate/kill/is_running/get_exitcode for background tasks.
    """

    def __init__(self) -> None:
        self._procs: Dict[str, subprocess.Popen] = {}
        self._locks: Dict[str, threading.Lock] = {}

    def _normalize_cmd(self, command: Union[str, list]) -> Union[str, list]:
        return command

    def run_foreground(
        self,
        command: Union[str, list],
        cwd: Optional[str] = None,
        env: Optional[dict] = None,
        timeout: Optional[float] = None,
        capture_output: bool = False,
    ) -> Dict:
        """Run command in foreground. If capture_output=True, return captured stdout/stderr.

        Returns a dict: {returncode, stdout, stderr}
        """
        cmd = self._normalize_cmd(command)
        if capture_output:
            completed = subprocess.run(
                cmd,
                shell=isinstance(cmd, str),
                cwd=cwd,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=timeout,
                text=True,
            )
            return {
                "returncode": completed.returncode,
                "stdout": completed.stdout,
                "stderr": completed.stderr,
            }
        else:
            # Attach to terminal, let subprocess write directly to stdout/stderr
            completed = subprocess.run(
                cmd, shell=isinstance(cmd, str), cwd=cwd, env=env, timeout=timeout
            )
            return {"returncode": completed.returncode, "stdout": None, "stderr": None}

    def run_background_with_output(
        self,
        command: Union[str, list],
        cwd: Optional[str] = None,
        env: Optional[dict] = None,
    ) -> Dict[str, Any]:
        """Start command in background and stream its stdout/stderr to the parent process stdout.

        Returns a dict: {"job_id": <id>, "pid": <pid>} for the started background job.
        For backward compatibility, callers that expect a string may still use the returned
        dict and read the "job_id" key.
        """
        cmd = self._normalize_cmd(command)
        proc = subprocess.Popen(
            cmd,
            shell=isinstance(cmd, str),
            cwd=cwd,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )

        job_id = str(uuid.uuid4())
        lock = threading.Lock()
        self._locks[job_id] = lock
        self._procs[job_id] = proc

        def _stream_output(p: subprocess.Popen, jid: str, l: threading.Lock):
            prefix = f"[bg:{jid}] "
            try:
                assert p.stdout is not None
                for line in p.stdout:
                    with l:
                        print(prefix + line.rstrip())
            except Exception:
                pass
            finally:
                p.stdout and p.stdout.close()

        t = threading.Thread(
            target=_stream_output, args=(proc, job_id, lock), daemon=True
        )
        t.start()
        return {"job_id": job_id, "pid": proc.pid}

    def run_background_no_output(
        self,
        command: Union[str, list],
        cwd: Optional[str] = None,
        env: Optional[dict] = None,
    ) -> Dict[str, Any]:
        """Start command in background with no stdout/stderr (redirected to /dev/null).

        Returns a dict: {"job_id": <id>, "pid": <pid>} for the started background job.
        """
        cmd = self._normalize_cmd(command)
        devnull = open(os.devnull, "wb")
        # detach from terminal by creating a new process group
        proc = subprocess.Popen(
            cmd,
            shell=isinstance(cmd, str),
            cwd=cwd,
            env=env,
            stdout=devnull,
            stderr=devnull,
            preexec_fn=os.setpgrp,
        )
        job_id = str(uuid.uuid4())
        self._procs[job_id] = proc
        # no lock needed because no streaming
        return {"job_id": job_id, "pid": proc.pid}

    def is_running(self, job_id: str) -> bool:
        p = self._procs.get(job_id)
        if p is None:
            return False
        return p.poll() is None

    def terminate(self, job_id: str) -> bool:
        p = self._procs.get(job_id)
        if p is None:
            return False
        try:
            p.terminate()
            return True
        except Exception:
            return False

    def kill(self, job_id: str) -> bool:
        p = self._procs.get(job_id)
        if p is None:
            return False
        try:
            p.kill()
            return True
        except Exception:
            return False

    def get_exitcode(self, job_id: str) -> Optional[int]:
        p = self._procs.get(job_id)
        if p is None:
            return None
        return p.poll()


__all__ = ["CommandExecutor"]
