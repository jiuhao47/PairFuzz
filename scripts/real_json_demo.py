#!/usr/bin/env python3
"""Build, fuzz and replay DiffRay's real JSON parser targets."""

import argparse
import hashlib
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "third_party" / "json-libs.json"


class ManifestError(RuntimeError):
    """The vendored-source manifest is missing, malformed or stale."""


def verify_vendor_manifest(root: Path) -> None:
    root = root.resolve()
    manifest_path = root / "third_party" / "json-libs.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ManifestError(f"cannot read {manifest_path}: {error}") from error

    if not isinstance(manifest, dict) or manifest.get("schema_version") != 1:
        raise ManifestError("unsupported vendor manifest schema")
    libraries = manifest.get("libraries")
    if not isinstance(libraries, list) or not libraries:
        raise ManifestError("vendor manifest must contain libraries")

    third_party = (root / "third_party").resolve()
    required = {"name", "version", "tag", "commit", "source_url", "files"}
    for library in libraries:
        if not isinstance(library, dict) or not required.issubset(library):
            raise ManifestError("vendor manifest library entry is incomplete")
        files = library["files"]
        if not isinstance(files, dict) or not files:
            raise ManifestError(f"{library['name']} has no vendored files")
        for relative, wanted_hash in files.items():
            if not isinstance(relative, str) or not isinstance(wanted_hash, str):
                raise ManifestError("vendor file paths and hashes must be strings")
            path = (root / relative).resolve()
            try:
                path.relative_to(third_party)
            except ValueError as error:
                raise ManifestError(f"vendor path escapes third_party: {relative}") from error
            if not path.is_file():
                raise ManifestError(f"vendored file is missing: {relative}")
            actual_hash = hashlib.sha256(path.read_bytes()).hexdigest()
            if actual_hash != wanted_hash:
                raise ManifestError(
                    f"checksum mismatch for {relative}: {actual_hash} != {wanted_hash}"
                )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("verify-vendor", help="verify pinned source provenance")
    args = parser.parse_args()
    if args.command == "verify-vendor":
        verify_vendor_manifest(ROOT)
        print("PASS: vendored cJSON and mjson sources match the pinned manifest")


if __name__ == "__main__":
    try:
        main()
    except (ManifestError, OSError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        sys.exit(1)
