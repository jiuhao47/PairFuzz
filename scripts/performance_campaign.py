#!/usr/bin/env python3
"""Run and summarize reproducible DiffRay performance campaigns."""

from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
from typing import Any


class ManifestError(RuntimeError):
    """The performance workload manifest is invalid or incomplete."""


class CampaignManifest:
    """Validated immutable inputs for one family of performance trials."""

    __slots__ = (
        "schema_version",
        "workload",
        "warmup_seconds",
        "trial_seconds",
        "trials",
        "campaign_seed",
        "rng_seed_controlled",
        "seed_dir",
        "target_a",
        "target_b",
        "configurations",
    )

    def __init__(
        self,
        *,
        schema_version: int,
        workload: str,
        warmup_seconds: float,
        trial_seconds: float,
        trials: int,
        campaign_seed: int,
        rng_seed_controlled: bool,
        seed_dir: Path,
        target_a: Path,
        target_b: Path,
        configurations: tuple[str, ...],
    ) -> None:
        self.schema_version = schema_version
        self.workload = workload
        self.warmup_seconds = warmup_seconds
        self.trial_seconds = trial_seconds
        self.trials = trials
        self.campaign_seed = campaign_seed
        self.rng_seed_controlled = rng_seed_controlled
        self.seed_dir = seed_dir
        self.target_a = target_a
        self.target_b = target_b
        self.configurations = configurations

    @classmethod
    def load(cls, path: Path) -> CampaignManifest:
        path = path.resolve()
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise ManifestError(f"cannot read campaign manifest {path}: {error}") from error
        if not isinstance(payload, dict):
            raise ManifestError("campaign manifest must be a JSON object")

        schema_version = payload.get("schema_version")
        if type(schema_version) is not int or schema_version != 1:
            raise ManifestError("unsupported campaign manifest schema")

        repository_root = payload.get("repository_root")
        if not isinstance(repository_root, str) or not repository_root:
            raise ManifestError("repository_root must be a non-empty string")
        root = (path.parent / repository_root).resolve()

        workload = payload.get("workload")
        if not isinstance(workload, str) or not workload:
            raise ManifestError("workload must be a non-empty string")

        warmup_seconds = _positive_finite(payload.get("warmup_seconds"), "warmup_seconds")
        trial_seconds = _positive_finite(payload.get("trial_seconds"), "trial_seconds")

        trials = payload.get("trials")
        if type(trials) is not int or trials <= 0:
            raise ManifestError("trials must be a positive integer")

        campaign_seed = payload.get("campaign_seed")
        if (
            type(campaign_seed) is not int
            or campaign_seed < 0
            or campaign_seed > (1 << 64) - 1
        ):
            raise ManifestError("campaign_seed must be an unsigned 64-bit integer")

        rng_seed_controlled = payload.get("rng_seed_controlled")
        if type(rng_seed_controlled) is not bool:
            raise ManifestError("rng_seed_controlled must be a boolean")

        seed_dir = _repository_path(root, payload.get("seed_dir"), "seed_dir")
        if not seed_dir.is_dir():
            raise ManifestError(f"seed directory does not exist: {seed_dir}")
        target_a = _repository_path(root, payload.get("target_a"), "target_a")
        target_b = _repository_path(root, payload.get("target_b"), "target_b")

        raw_configurations = payload.get("configurations")
        if not isinstance(raw_configurations, dict) or not raw_configurations:
            raise ManifestError("configurations must be a non-empty object")
        for name, configuration in raw_configurations.items():
            if not isinstance(name, str) or not name or not isinstance(configuration, dict):
                raise ManifestError("each configuration must have a name and object value")

        return cls(
            schema_version=schema_version,
            workload=workload,
            warmup_seconds=warmup_seconds,
            trial_seconds=trial_seconds,
            trials=trials,
            campaign_seed=campaign_seed,
            rng_seed_controlled=rng_seed_controlled,
            seed_dir=seed_dir,
            target_a=target_a,
            target_b=target_b,
            configurations=tuple(sorted(raw_configurations)),
        )


def _positive_finite(value: Any, field: str) -> float:
    if type(value) not in (int, float) or not math.isfinite(value) or value <= 0:
        raise ManifestError(f"{field} must be a finite positive number")
    return float(value)


def _repository_path(root: Path, value: Any, field: str) -> Path:
    if not isinstance(value, str) or not value:
        raise ManifestError(f"{field} must be a non-empty string")
    candidate = (root / value).resolve()
    try:
        candidate.relative_to(root)
    except ValueError as error:
        raise ManifestError(f"{field} escapes repository root: {value}") from error
    return candidate


def rotate_configurations(names: tuple[str, ...], trial_index: int) -> tuple[str, ...]:
    ordered = tuple(sorted(names))
    if not ordered:
        return ()
    offset = trial_index % len(ordered)
    return ordered[offset:] + ordered[:offset]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()
