#!/usr/bin/env python3
"""Deterministic real-target fixture generator (PNG implementation lands in Task 3)."""

import argparse


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--pair", choices=("png",), required=True)
    result.add_argument("--output", required=True)
    return result


def main():
    arguments = parser().parse_args()
    raise SystemExit(
        f"fixture generation for {arguments.pair} is not implemented yet; Task 3 will populate {arguments.output}"
    )


if __name__ == "__main__":
    main()
