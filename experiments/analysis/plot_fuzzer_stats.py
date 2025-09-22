#!/usr/bin/env python3
"""Parse a fuzzer_stats.json containing multiple homogeneous dicts and plot
corpus_size, objective_size, exec_sec, and user_stats over run_time.secs.

Usage: python scripts/plot_fuzzer_stats.py /path/to/fuzzer_stats.json --outdir plots
"""
import argparse
import os

# parser functionality moved to fuzzer_stats_parser.py; no need for json/defaultdict here

import matplotlib.pyplot as plt
from matplotlib.ticker import ScalarFormatter, MaxNLocator
import numpy as np
from fuzzer_stats_parser import FuzzerStatsParser


class FuzzerStatsPlotter:
    """Reusable plotter for fuzzer_stats series.

    Example:
        plotter = FuzzerStatsPlotter(markersize=2, dpi=150, annotate=False)
        plotter.plot_series(data, outdir)
    """

    def __init__(
        self,
        markersize: float = 2.0,
        dpi: int = 150,
        annotate: bool = True,
        annotate_top_n: int = 5,
    ):
        self.markersize = float(markersize)
        self.dpi = int(dpi)
        self.annotate = bool(annotate)
        self.annotate_top_n = int(annotate_top_n)

    @staticmethod
    def _format_no_sci(v):
        try:
            if v is None:
                return "nan"
            if isinstance(v, (int,)):
                return str(v)
            f = float(v)
            if np.isnan(f):
                return "nan"
            if abs(f - round(f)) < 1e-9:
                return str(int(round(f)))
            s = f"{f:.6f}".rstrip("0").rstrip(".")
            return s
        except Exception:
            return str(v)

    def plot_series(self, data, outdir):
        os.makedirs(outdir, exist_ok=True)
        plots = []
        mapping = {
            "corpus": "corpus_size",
            "objectives": "objective_size",
            "exec_sec": "exec_sec",
        }
        for key in ["corpus", "objectives", "exec_sec"]:
            if key in data:
                t, v = data[key]
                fig, ax = plt.subplots(figsize=(8, 4), dpi=self.dpi)
                ax.plot(t, v, marker="o", markersize=self.markersize)
                ax.set_xlabel("run_time (secs)")
                ax.set_ylabel(mapping[key])
                ax.set_title(f"{mapping[key]} over run_time")
                ax.grid(True)

                ax.yaxis.set_major_formatter(ScalarFormatter(useMathText=False))
                ax.ticklabel_format(style="plain", axis="y")

                is_exec = key == "exec_sec"
                if is_exec:
                    ax.yaxis.set_major_locator(MaxNLocator(integer=True))

                if self.annotate and len(t) > 0:
                    last_x = t[-1]
                    last_y = v[-1]
                    ax.scatter(
                        [last_x], [last_y], color="red", s=(self.markersize * 15)
                    )
                    ann_text = (
                        self._format_no_sci(int(round(last_y)))
                        if (is_exec and last_y is not None)
                        else self._format_no_sci(last_y)
                    )
                    ax.annotate(
                        ann_text,
                        xy=(last_x, last_y),
                        xytext=(5, 5),
                        textcoords="offset points",
                        color="red",
                    )

                    try:
                        mask = ~np.isnan(v)
                        valid_idx = np.where(mask)[0]
                        if valid_idx.size > 0:
                            try:
                                vals = v[valid_idx]
                                peak_count = min(self.annotate_top_n, vals.size)
                                peak_order = np.argsort(vals)[::-1]
                                for pidx in peak_order[:peak_count]:
                                    real_idx = valid_idx[pidx]
                                    x = t[real_idx]
                                    y = v[real_idx]
                                    ax.scatter(
                                        [x],
                                        [y],
                                        color="green",
                                        s=(self.markersize * 14),
                                    )
                                    ann_text = (
                                        self._format_no_sci(int(round(y)))
                                        if (is_exec and y is not None)
                                        else self._format_no_sci(y)
                                    )
                                    ax.annotate(
                                        ann_text,
                                        xy=(x, y),
                                        xytext=(5, -18),
                                        textcoords="offset points",
                                        color="green",
                                    )
                            except Exception:
                                pass

                            changed = []
                            last_val = None
                            for i in valid_idx:
                                val = v[i]
                                if last_val is None or val != last_val:
                                    changed.append(i)
                                last_val = val

                            if len(changed) == 0:
                                chosen = []
                            elif len(changed) <= self.annotate_top_n:
                                chosen = changed
                            else:
                                step = len(changed) / float(self.annotate_top_n)
                                chosen = [
                                    changed[int(i * step)]
                                    for i in range(self.annotate_top_n)
                                ]

                            for real_idx in chosen:
                                x = t[real_idx]
                                y = v[real_idx]
                                ax.scatter(
                                    [x], [y], color="orange", s=(self.markersize * 12)
                                )
                                ann_text = (
                                    self._format_no_sci(int(round(y)))
                                    if (is_exec and y is not None)
                                    else self._format_no_sci(y)
                                )
                                ax.annotate(
                                    ann_text,
                                    xy=(x, y),
                                    xytext=(5, -10),
                                    textcoords="offset points",
                                    color="orange",
                                )
                    except Exception:
                        pass

                outp = os.path.join(outdir, f"{mapping[key]}.png")
                fig.tight_layout()
                fig.savefig(outp, dpi=self.dpi)
                plt.close(fig)
                plots.append(outp)

        user_keys = [k for k in data.keys() if k.startswith("user:")]
        if user_keys:
            fig, ax = plt.subplots(figsize=(10, 6), dpi=self.dpi)
            for uk in sorted(user_keys):
                t, v = data[uk]
                ax.plot(
                    t,
                    v,
                    marker="o",
                    markersize=self.markersize,
                    label=uk.replace("user:", ""),
                )
            ax.yaxis.set_major_formatter(ScalarFormatter(useMathText=False))
            ax.ticklabel_format(style="plain", axis="y")

            if self.annotate:
                for uk in sorted(user_keys):
                    t, v = data[uk]
                    if len(t) == 0:
                        continue
                    try:
                        last_x = t[-1]
                        last_y = v[-1]
                        ax.scatter(
                            [last_x], [last_y], color="red", s=(self.markersize * 15)
                        )
                        ax.annotate(
                            self._format_no_sci(last_y),
                            xy=(last_x, last_y),
                            xytext=(5, 5),
                            textcoords="offset points",
                            color="red",
                        )
                    except Exception:
                        pass
                    try:
                        mask = ~np.isnan(v)
                        valid_idx = np.where(mask)[0]
                        if valid_idx.size > 0:
                            try:
                                vals = v[valid_idx]
                                peak_count = min(self.annotate_top_n, vals.size)
                                peak_order = np.argsort(vals)[::-1]
                                for pidx in peak_order[:peak_count]:
                                    real_idx = valid_idx[pidx]
                                    x = t[real_idx]
                                    y = v[real_idx]
                                    ax.scatter(
                                        [x],
                                        [y],
                                        color="green",
                                        s=(self.markersize * 14),
                                    )
                                    ax.annotate(
                                        self._format_no_sci(y),
                                        xy=(x, y),
                                        xytext=(5, -18),
                                        textcoords="offset points",
                                        color="green",
                                    )
                            except Exception:
                                pass

                            changed = []
                            last_val = None
                            for i in valid_idx:
                                val = v[i]
                                if last_val is None or val != last_val:
                                    changed.append(i)
                                last_val = val

                            if len(changed) == 0:
                                chosen = []
                            elif len(changed) <= self.annotate_top_n:
                                chosen = changed
                            else:
                                step = len(changed) / float(self.annotate_top_n)
                                chosen = [
                                    changed[int(i * step)]
                                    for i in range(self.annotate_top_n)
                                ]

                            for real_idx in chosen:
                                x = t[real_idx]
                                y = v[real_idx]
                                ax.scatter(
                                    [x], [y], color="orange", s=(self.markersize * 12)
                                )
                                ax.annotate(
                                    self._format_no_sci(y),
                                    xy=(x, y),
                                    xytext=(5, -10),
                                    textcoords="offset points",
                                    color="orange",
                                )
                    except Exception:
                        pass
            ax.set_xlabel("run_time (secs)")
            ax.set_ylabel("user_stats value (approx)")
            ax.set_title("user_stats over run_time")
            ax.legend()
            ax.grid(True)
            outp = os.path.join(outdir, "user_stats.png")
            fig.tight_layout()
            fig.savefig(outp, dpi=self.dpi)
            plt.close(fig)
            plots.append(outp)

        return plots


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("jsonpath", help="Path to fuzzer_stats.json")
    parser.add_argument("--outdir", default="plots", help="Output directory for PNGs")
    parser.add_argument(
        "--markersize", type=float, default=2.0, help="Marker size for plot points"
    )
    # annotation defaults: enabled by default, allow disabling with --no-annotate
    parser.add_argument(
        "--annotate",
        dest="annotate",
        action="store_true",
        help="Enable annotations on plots (default: enabled)",
    )
    parser.add_argument(
        "--no-annotate",
        dest="annotate",
        action="store_false",
        help="Disable annotations on plots",
    )
    parser.set_defaults(annotate=True)
    parser.add_argument(
        "--annotate-top-n",
        type=int,
        default=5,
        help="Number of top peaks to annotate per series when annotations are enabled (default: 5)",
    )
    parser.add_argument(
        "--dpi", type=int, default=150, help="DPI for saved PNG figures"
    )
    args = parser.parse_args()

    plotter = FuzzerStatsPlotter(
        markersize=args.markersize,
        dpi=args.dpi,
        annotate=args.annotate,
        annotate_top_n=args.annotate_top_n,
    )
    records = FuzzerStatsParser.load_records(args.jsonpath)
    if not records:
        print("No records found in the JSON file.")
        return
    data = FuzzerStatsParser.collect_series(records)
    plots = plotter.plot_series(data, args.outdir)
    print("Wrote plots:")
    for p in plots:
        print(" -", p)


if __name__ == "__main__":
    main()
