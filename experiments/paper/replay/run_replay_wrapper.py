#!/usr/bin/env python3
"""Run replay_program with a selected target config and parse the DIFF result.

Example:
  python3 run_replay_wrapper.py --program png-libpng --input /path/to/sample
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional


_DIFF_RE = re.compile(r"\[DIFF\]:\s*(Yes|No)\b", re.IGNORECASE)


@dataclass(frozen=True)
class ReplayConfig:
    prog_a: Path
    func_a: str
    prog_b: Path
    func_b: str


def _repo_root_from_here() -> Path:
    return Path(__file__).resolve().parents[3]


def _build_configs(repo_root: Path) -> Dict[str, ReplayConfig]:
    bins = repo_root / "app" / "bins" / ".bin"
    return {
        "png-libpng": ReplayConfig(
            prog_a=bins / "libpng" / "libpng_local.so",
            func_a="libpng_parse",
            prog_b=bins / "libpng" / "libspng_local.so",
            func_b="libspng_parse",
        ),
        "png-libspng": ReplayConfig(
            prog_a=bins / "libspng" / "libpng_local.so",
            func_a="libpng_parse",
            prog_b=bins / "libspng" / "libspng_local.so",
            func_b="libspng_parse",
        ),
        "json-cjson": ReplayConfig(
            prog_a=bins / "cjson" / "cjson_local.so",
            func_a="cjson_parse",
            prog_b=bins / "cjson" / "mjson_local.so",
            func_b="mjson_parse",
        ),
        "json-mjson": ReplayConfig(
            prog_a=bins / "mjson" / "cjson_local.so",
            func_a="cjson_parse",
            prog_b=bins / "mjson" / "mjson_local.so",
            func_b="mjson_parse",
        ),
        "zlib-zlib": ReplayConfig(
            prog_a=bins / "zlib" / "zlib_local.so",
            func_a="zlib_parse",
            prog_b=bins / "zlib" / "libdeflate_local.so",
            func_b="libdeflate_parse",
        ),
        "zlib-libdeflate": ReplayConfig(
            prog_a=bins / "libdeflate" / "zlib_local.so",
            func_a="zlib_parse",
            prog_b=bins / "libdeflate" / "libdeflate_local.so",
            func_b="libdeflate_parse",
        ),
        "tar-archive": ReplayConfig(
            prog_a=bins / "archive_old" / "archive_local.so",
            func_a="archive_parse",
            prog_b=bins / "archive_old" / "libtar_local.so",
            func_b="tar_parse",
        ),
        "xz-xz": ReplayConfig(
            prog_a=bins / "xzold" / "xz_local.so",
            func_a="xz_parse",
            prog_b=bins / "xzold" / "xz7z_local.so",
            func_b="cli_scanxz",
        ),
        "xml-xml2": ReplayConfig(
            prog_a=bins / "xml2" / "xml2_local.so",
            func_a="xml2_xmlparse",
            prog_b=bins / "xml2" / "expat_local.so",
            func_b="expat_xmlparse",
        ),
        "xml-expat": ReplayConfig(
            prog_a=bins / "expat" / "xml2_local.so",
            func_a="xml2_xmlparse",
            prog_b=bins / "expat" / "expat_local.so",
            func_b="expat_xmlparse",
        ),
        "bmp-IM": ReplayConfig(
            prog_a=bins / "IM" / "IM_local.so",
            func_a="imagemagick_parse",
            prog_b=bins / "IM" / "GM_local.so",
            func_b="GM_parse",
        ),
        "bmp-GM": ReplayConfig(
            prog_a=bins / "GM" / "GM_local.so",
            func_a="GM_parse",
            prog_b=bins / "GM" / "GM_local.so",
            func_b="GM_parse",
        ),
    }


def _parse_diff(output: str) -> Optional[str]:
    last: Optional[str] = None
    for m in _DIFF_RE.finditer(output):
        last = m.group(1).capitalize()
    return last


def _run_replay(
    replay_bin: Path,
    cfg: ReplayConfig,
    input_path: Path,
    env_overrides: Dict[str, str],
    timeout_s: Optional[float],
) -> subprocess.CompletedProcess[str]:
    cmd = [
        str(replay_bin),
        f"--prog-a={cfg.prog_a}",
        f"--func-a={cfg.func_a}",
        f"--prog-b={cfg.prog_b}",
        f"--func-b={cfg.func_b}",
        f"--input={input_path}",
    ]
    env = os.environ.copy()
    env.update(env_overrides)
    return subprocess.run(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
        timeout=timeout_s,
        env=env,
    )


def _positive_float(s: str) -> float:
    try:
        v = float(s)
    except ValueError as e:
        raise argparse.ArgumentTypeError("must be a number") from e
    if v <= 0:
        raise argparse.ArgumentTypeError("must be > 0")
    return v


def main(argv: List[str]) -> int:
    ap = argparse.ArgumentParser(
        description="Run replay_program with a named target config and print DIFF result.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    ap.add_argument("--program", help="Program key, use --list to see options.")
    ap.add_argument("--input", help="Input file path.")
    ap.add_argument(
        "--replay-bin",
        default=None,
        help="Path to replay_program binary (defaults to app/bins/.bin/replay/replay_program).",
    )
    ap.add_argument(
        "--repo-root",
        default=None,
        help="Repository root (defaults to auto-detect from script location).",
    )
    ap.add_argument(
        "--env",
        action="append",
        default=[],
        help="Extra environment variables like KEY=VALUE (repeatable).",
    )
    ap.add_argument(
        "--timeout",
        type=_positive_float,
        default=None,
        help="Timeout (seconds) for replay_program.",
    )
    ap.add_argument(
        "--list",
        action="store_true",
        help="List available program keys and exit.",
    )
    ap.add_argument(
        "--show-output",
        action="store_true",
        help="Print full replay output to stderr.",
    )

    ns = ap.parse_args(argv)

    repo_root = Path(ns.repo_root).resolve() if ns.repo_root else _repo_root_from_here()
    configs = _build_configs(repo_root)

    if ns.list:
        for key in sorted(configs.keys()):
            print(key)
        return 0

    if not ns.program or not ns.input:
        ap.error("--program and --input are required unless --list is used")

    cfg = configs.get(ns.program)
    if cfg is None:
        print(f"error: unknown program key: {ns.program}", file=sys.stderr)
        print("hint: use --list to see available keys", file=sys.stderr)
        return 2

    input_path = Path(ns.input).resolve()
    if not input_path.is_file():
        print(f"error: input file not found: {input_path}", file=sys.stderr)
        return 2

    replay_bin = (
        Path(ns.replay_bin).resolve()
        if ns.replay_bin
        else (repo_root / "app" / "bins" / ".bin" / "replay" / "replay_program")
    )
    if not replay_bin.is_file():
        print(f"error: replay_program not found: {replay_bin}", file=sys.stderr)
        return 2

    env_overrides: Dict[str, str] = {}
    for item in ns.env:
        if "=" not in item:
            print(
                f"error: invalid --env value (expected KEY=VALUE): {item}",
                file=sys.stderr,
            )
            return 2
        k, v = item.split("=", 1)
        env_overrides[k] = v

    try:
        cp = _run_replay(
            replay_bin=replay_bin,
            cfg=cfg,
            input_path=input_path,
            env_overrides=env_overrides,
            timeout_s=ns.timeout,
        )
    except subprocess.TimeoutExpired:
        print("error: replay_program timed out", file=sys.stderr)
        return 124

    if ns.show_output:
        print(cp.stdout, file=sys.stderr)

    diff = _parse_diff(cp.stdout)
    if diff is None:
        print("error: failed to parse [DIFF]: Yes/No from output", file=sys.stderr)
        return 3

    print(diff)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
