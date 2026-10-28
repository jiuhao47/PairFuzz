import hashlib
import json
import os
import tempfile
from dataclasses import dataclass
from pathlib import Path

from .registry import pair_spec


@dataclass(frozen=True)
class ProfileFlags:
    compile: tuple[str, ...]
    link: tuple[str, ...]


def profile_flags(profile: str) -> ProfileFlags:
    common = ("-g",)
    if profile == "asan":
        return ProfileFlags(("-O1", *common, "-fno-omit-frame-pointer"), ("-fsanitize=address",))
    if profile == "release":
        return ProfileFlags(("-O3", *common), ())
    raise ValueError(f"unknown target profile {profile!r}; expected asan or release")


def build_fingerprint(inputs: dict) -> str:
    encoded = json.dumps(inputs, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()
    return hashlib.sha256(encoded).hexdigest()


def clean_build_environment(base: dict, *, profile: str, cc: str, cxx: str, ar: str, ranlib: str) -> dict:
    profile_flags(profile)
    environment = {}
    for name in ("PATH", "HOME", "TMPDIR", "LANG", "LC_ALL", "SOURCE_DATE_EPOCH"):
        if name in base:
            environment[name] = base[name]
    environment.update(
        {
            "CC": cc,
            "CXX": cxx,
            "AR": ar,
            "RANLIB": ranlib,
            "DIFFRAY_TARGET_PROFILE": profile,
        }
    )
    return environment


class BuildManifestTransaction:
    def __init__(self, root: Path, identity: dict):
        self.root = root
        self.identity = dict(identity)
        self.commands = []
        self.completed = False

    def __enter__(self):
        self.root.mkdir(parents=True, exist_ok=True)
        return self

    def record_command(self, argv, environment):
        self.commands.append({"argv": [str(value) for value in argv], "env": dict(sorted(environment.items()))})

    def complete(self, fields: dict) -> Path:
        manifest = {
            **self.identity,
            **fields,
            "status": "complete",
            "commands": self.commands,
        }
        descriptor, temporary_name = tempfile.mkstemp(prefix=".build-", suffix=".json", dir=self.root)
        temporary = Path(temporary_name)
        try:
            with os.fdopen(descriptor, "w", encoding="utf-8") as output:
                json.dump(manifest, output, indent=2, sort_keys=True)
                output.write("\n")
                output.flush()
                os.fsync(output.fileno())
            final = self.root / "build.json"
            temporary.replace(final)
            self.completed = True
            return final
        finally:
            temporary.unlink(missing_ok=True)

    def __exit__(self, _type, _value, _traceback):
        return False


def build_pair(root: Path, pair: str, profile: str) -> Path:
    spec = pair_spec(pair)
    profile_flags(profile)
    if not spec.build_supported:
        raise RuntimeError(
            f"build support for {pair} is not implemented yet; fetch is available and the adapter plan must land first"
        )
    raise AssertionError("implemented pair is missing its builder")
