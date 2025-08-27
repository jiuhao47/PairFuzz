"""CPU assessor utility module.

Provides CPUAssessor which inspects /proc/stat and recommends CPUs for taskset.
"""

import os
import time
from typing import Dict, List, Any


class CPUAssessor:
    """Encapsulates CPU load assessment logic."""

    def __init__(
        self, threshold: float = 20.0, interval: float = 0.2, min_free: int = 1
    ):
        self.threshold = threshold
        self.interval = interval
        self.min_free = min_free

    @staticmethod
    def _read_proc_stat() -> List[Dict[str, int]]:
        cpus: List[Dict[str, int]] = []
        with open("/proc/stat", "r") as f:
            for line in f:
                if not line.startswith("cpu"):
                    continue
                parts = line.split()
                name = parts[0]
                if name == "cpu":
                    continue
                vals = [int(x) for x in parts[1:]]
                idle = vals[3] + (vals[4] if len(vals) > 4 else 0)
                total = sum(vals)
                cpus.append({"idle": idle, "total": total})
        return cpus

    def assess(self) -> Dict[str, Any]:
        """Run the assessment using instance settings and return result dict."""
        before = self._read_proc_stat()
        time.sleep(self.interval)
        after = self._read_proc_stat()

        per_cpu: Dict[int, float] = {}
        free_cpus: List[int] = []

        for idx, (b, a) in enumerate(zip(before, after)):
            d_idle = a["idle"] - b["idle"]
            d_total = a["total"] - b["total"]
            if d_total <= 0:
                usage = 0.0
            else:
                usage = max(0.0, 100.0 * (1.0 - (d_idle / d_total)))
            per_cpu[idx] = round(usage, 2)
            if usage < self.threshold:
                free_cpus.append(idx)

        try:
            allowed = sorted(os.sched_getaffinity(0))
        except AttributeError:
            allowed = list(range(len(per_cpu)))

        allowed_free = [c for c in free_cpus if c in allowed]
        suitable = len(allowed_free) >= self.min_free
        reason_parts: List[str] = []
        if suitable:
            reason_parts.append(
                f"found {len(allowed_free)} free CPUs (threshold={self.threshold}%)"
            )
        else:
            reason_parts.append(
                f"only {len(allowed_free)} free CPUs < required {self.min_free}"
            )

        try:
            with open("/proc/loadavg", "r") as f:
                la = f.read().split()
                load1 = float(la[0])
        except Exception:
            load1 = 0.0

        total_cpus = len(per_cpu)
        if total_cpus > 0 and load1 / total_cpus > 0.7:
            reason_parts.append(
                f"loadavg {load1:.2f} high relative to {total_cpus} cpus"
            )

        result: Dict[str, Any] = {
            "per_cpu": per_cpu,
            "free_cpus": free_cpus,
            "allowed_free": allowed_free,
            "suitable": suitable,
            "reason": "; ".join(reason_parts),
            "load1": load1,
            "total_cpus": total_cpus,
            "affinity_allowed": allowed,
        }
        return result

    def print_report(self, r: Dict[str, Any]) -> None:
        print("Per-CPU usage (%):")
        for idx in sorted(r["per_cpu"]):
            mark = ""
            if idx in r["allowed_free"]:
                mark = " <-- free & allowed"
            elif idx in r["free_cpus"]:
                mark = " <-- free (not allowed by affinity)"
            print(f'  cpu{idx}: {r["per_cpu"][idx]:6.2f}%{mark}')

        print()
        print(f"Load (1m): {r['load1']:.2f}  CPUs: {r['total_cpus']}")
        print(f"Affinity allowed CPUs for this process: {r['affinity_allowed']}")
        print(f"Free CPUs below threshold: {r['free_cpus']}")
        print(f"Allowed & free CPUs (recommended for taskset): {r['allowed_free']}")
        print(f"Suitable for taskset (min_free satisfied): {r['suitable']}")
        print("Reason:", r["reason"])


__all__ = ["CPUAssessor"]
