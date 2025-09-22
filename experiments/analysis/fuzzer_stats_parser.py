#!/usr/bin/env python3
"""Fuzzer stats parser extracted from plot_fuzzer_stats.py

Provides FuzzerStatsParser which can load JSON records from a file and
produce series suitable for plotting.
"""
from __future__ import annotations

import json
from collections import defaultdict
from typing import Any, Dict, List, Optional

import numpy as np


def extract_user_stats(user_stats_dict: Dict[str, Any]) -> Dict[str, Optional[float]]:
    out: Dict[str, Optional[float]] = {}
    if not isinstance(user_stats_dict, dict):
        return out
    for k, v in user_stats_dict.items():
        val = None
        try:
            value = v.get("value") if isinstance(v, dict) else None
            if isinstance(value, dict):
                for subk, subv in value.items():
                    if isinstance(subv, list) and len(subv) > 0:
                        val = subv[0]
                        break
                    elif isinstance(subv, (int, float)):
                        val = subv
                        break
            elif isinstance(v, (int, float)):
                val = v
        except Exception:
            val = None
        out[k] = val
    return out


def load_json_records(path: str) -> List[dict]:
    with open(path, "r") as f:
        s = f.read()
    s = s.strip()
    if not s:
        return []
    try:
        data = json.loads(s)
        if isinstance(data, list):
            return data
    except json.JSONDecodeError:
        pass

    decoder = json.JSONDecoder()
    idx = 0
    n = len(s)
    records: List[dict] = []
    while idx < n:
        while idx < n and s[idx].isspace():
            idx += 1
        if idx >= n:
            break
        try:
            obj, end = decoder.raw_decode(s, idx)
        except json.JSONDecodeError:
            break
        records.append(obj)
        idx = end
    return records


class FuzzerStatsParser:
    """Parser class to load fuzzer stats records and collect series."""

    @staticmethod
    def load_records(path: str) -> List[dict]:
        return load_json_records(path)

    @staticmethod
    def collect_series(records: List[dict]):
        series = defaultdict(list)
        user_keys = set()
        for rec in records:
            if not isinstance(rec, dict):
                continue
            run_time = None
            if isinstance(rec.get("run_time"), dict):
                run_time = rec["run_time"].get("secs")
            if run_time is None:
                run_time = (
                    rec.get("exec_sec") or rec.get("exec_secs") or rec.get("exec_sec")
                )
            if run_time is None and isinstance(rec.get("client_stats"), dict):
                try:
                    cs0 = rec["client_stats"].get("0")
                    if isinstance(cs0, dict) and isinstance(
                        cs0.get("last_window_time"), dict
                    ):
                        run_time = cs0["last_window_time"].get("secs")
                except Exception:
                    pass

            if run_time is None:
                continue

            corpus = rec.get("corpus") if rec.get("corpus") is not None else None
            objectives = (
                rec.get("objectives") if rec.get("objectives") is not None else None
            )
            exec_sec = (
                rec.get("exec_sec")
                if rec.get("exec_sec") is not None
                else rec.get("exec_sec")
            )

            cs = rec.get("client_stats")
            if isinstance(cs, dict):
                c0 = cs.get("0") or next(iter(cs.values()), None)
                if isinstance(c0, dict):
                    corpus = c0.get("corpus_size", corpus)
                    objectives = c0.get("objective_size", objectives)
                    exec_sec = rec.get("exec_sec", exec_sec)
                    if isinstance(c0.get("start_time"), dict) and run_time is None:
                        run_time = c0["start_time"].get("secs")
                    us = c0.get("user_stats")
                    if isinstance(us, dict):
                        flat = extract_user_stats(us)
                        for k, v in flat.items():
                            user_keys.add(k)
                            series[f"user:{k}"].append((run_time, v))

            series["run_time"].append(run_time)
            series["corpus"].append((run_time, corpus))
            series["objectives"].append((run_time, objectives))
            series["exec_sec"].append((run_time, exec_sec))

        out = {}
        for name, arr in series.items():
            if name == "run_time":
                continue
            arr = [x for x in arr if x[0] is not None]
            arr.sort(key=lambda x: x[0])
            ts = [x[0] for x in arr]
            vals = [x[1] for x in arr]
            # convert None to nan for numeric arrays
            vals_conv = [float(v) if v is not None else np.nan for v in vals]
            out[name] = (np.array(ts, dtype=float), np.array(vals_conv, dtype=float))

        return out
