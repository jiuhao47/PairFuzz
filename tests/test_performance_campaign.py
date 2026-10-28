import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "performance_campaign.py"
MANIFEST = ROOT / "examples" / "real-json" / "performance-campaign.json"


def load_campaign_module():
    if not SCRIPT.is_file():
        raise AssertionError(f"missing performance campaign runner: {SCRIPT}")
    spec = importlib.util.spec_from_file_location("performance_campaign", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class CampaignManifestTests(unittest.TestCase):
    def test_manifest_pins_real_json_workload(self):
        campaign = load_campaign_module()
        manifest = campaign.CampaignManifest.load(MANIFEST)

        self.assertEqual(manifest.schema_version, 1)
        self.assertEqual(manifest.workload, "real-json")
        self.assertEqual(manifest.warmup_seconds, 10.0)
        self.assertEqual(manifest.trial_seconds, 300.0)
        self.assertEqual(manifest.trials, 5)
        self.assertEqual(manifest.campaign_seed, 0xD1FFA720261026)
        self.assertFalse(manifest.rng_seed_controlled)
        self.assertEqual(
            manifest.seed_dir, ROOT / "examples" / "real-json" / "campaign-seeds"
        )
        self.assertEqual(
            manifest.target_a, ROOT / "target" / "real-json" / "cjson_target"
        )
        self.assertEqual(
            manifest.target_b, ROOT / "target" / "real-json" / "mjson_target"
        )
        self.assertEqual(manifest.configurations, ("baseline",))

    def test_manifest_rejects_non_positive_or_non_finite_duration(self):
        campaign = load_campaign_module()
        invalid_values = (0, -1, math.inf, -math.inf, math.nan, True)

        for field in ("warmup_seconds", "trial_seconds"):
            for invalid in invalid_values:
                with self.subTest(field=field, invalid=invalid):
                    with self._temporary_manifest(**{field: invalid}) as manifest_path:
                        with self.assertRaises(campaign.ManifestError):
                            campaign.CampaignManifest.load(manifest_path)

    def test_manifest_rejects_missing_seed_directory(self):
        campaign = load_campaign_module()
        with self._temporary_manifest(seed_dir="examples/real-json/missing") as path:
            with self.assertRaises(campaign.ManifestError):
                campaign.CampaignManifest.load(path)

    def test_rotation_moves_each_configuration_to_each_position(self):
        campaign = load_campaign_module()
        names = ("zeta", "alpha", "middle")

        self.assertEqual(
            campaign.rotate_configurations(names, 0),
            ("alpha", "middle", "zeta"),
        )
        self.assertEqual(
            campaign.rotate_configurations(names, 1),
            ("middle", "zeta", "alpha"),
        )
        self.assertEqual(
            campaign.rotate_configurations(names, 2),
            ("zeta", "alpha", "middle"),
        )
        self.assertEqual(
            campaign.rotate_configurations(names, 3),
            ("alpha", "middle", "zeta"),
        )

    def test_sha256_file_matches_known_vector(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input"
            path.write_bytes(b"DiffRay\n")
            expected = hashlib.sha256(b"DiffRay\n").hexdigest()

            self.assertEqual(campaign.sha256_file(path), expected)

    def _temporary_manifest(self, **overrides):
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        manifest_dir = root / "examples" / "real-json"
        seed_dir = manifest_dir / "campaign-seeds"
        seed_dir.mkdir(parents=True)
        payload = {
            "schema_version": 1,
            "repository_root": "../..",
            "workload": "real-json",
            "warmup_seconds": 10.0,
            "trial_seconds": 300.0,
            "trials": 5,
            "campaign_seed": 0xD1FFA720261026,
            "rng_seed_controlled": False,
            "seed_dir": "examples/real-json/campaign-seeds",
            "target_a": "target/real-json/cjson_target",
            "target_b": "target/real-json/mjson_target",
            "configurations": {"baseline": {"mode": "single-cov"}},
        }
        payload.update(overrides)
        path = manifest_dir / "performance-campaign.json"
        path.write_text(json.dumps(payload), encoding="utf-8")

        class TemporaryManifest:
            def __enter__(self):
                return path

            def __exit__(self, exc_type, exc_value, traceback):
                temporary.cleanup()

        return TemporaryManifest()


class ProcessRunnerTests(unittest.TestCase):
    def test_clean_environment_removes_fuzzer_shared_memory_variables(self):
        campaign = load_campaign_module()
        with mock.patch.dict(
            os.environ,
            {
                "__AFL_SHM_ID": "stale-afl",
                "__DIFFRAY_SHM_ID": "stale-diffray",
                "DIFFRAY_KEEP": "visible",
            },
            clear=False,
        ):
            environment = campaign.clean_environment(ROOT)

        self.assertNotIn("__AFL_SHM_ID", environment)
        self.assertNotIn("__DIFFRAY_SHM_ID", environment)
        self.assertEqual(environment["DIFFRAY_KEEP"], "visible")
        self.assertEqual(environment["CARGO_TARGET_DIR"], str(ROOT / "target"))

    def test_bounded_runner_records_early_exit(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            record = campaign.run_bounded(
                [
                    sys.executable,
                    "-c",
                    "print('evidence-before-exit', flush=True); raise SystemExit(7)",
                ],
                cwd=root,
                env=os.environ.copy(),
                duration=2.0,
                cpu=self._available_cpu(),
                log_path=root / "early.log",
            )

            self.assertTrue(record.completed_early)
            self.assertFalse(record.timed_out)
            self.assertEqual(record.exit_code, 7)
            self.assertGreaterEqual(record.wall_seconds, 0)
            self.assertEqual(
                (root / "early.log").read_text().strip(), "evidence-before-exit"
            )

    def test_bounded_runner_terminates_process_group(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pid_path = root / "pid"
            program = (
                "import os,pathlib,time; "
                f"pathlib.Path({str(pid_path)!r}).write_text(str(os.getpid())); "
                "time.sleep(60)"
            )
            record = campaign.run_bounded(
                [sys.executable, "-c", program],
                cwd=root,
                env=os.environ.copy(),
                duration=0.2,
                cpu=self._available_cpu(),
                log_path=root / "timeout.log",
            )

            self.assertTrue(record.timed_out)
            self.assertFalse(record.completed_early)
            self.assertTrue(record.term_sent)
            self.assertFalse(self._pid_exists(int(pid_path.read_text())))

    def test_bounded_runner_counts_reaped_descendant_cpu_time(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            child = (
                "import time; end=time.process_time()+0.02; "
                "exec('while time.process_time() < end: pass')"
            )
            parent = (
                "import subprocess,sys,time; "
                f"[subprocess.run([sys.executable, '-c', {child!r}]) for _ in range(20)]; "
                "time.sleep(60)"
            )
            record = campaign.run_bounded(
                [sys.executable, "-c", parent],
                cwd=root,
                env=os.environ.copy(),
                duration=0.8,
                cpu=self._available_cpu(),
                log_path=root / "cpu.log",
            )

            self.assertGreater(record.cpu_seconds, 0.3)

    @unittest.skipUnless(os.name == "posix", "requires POSIX process groups")
    def test_bounded_runner_kills_stubborn_descendant(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            child_pid_path = root / "child-pid"
            child = (
                "import os,pathlib,signal,time; "
                "signal.signal(signal.SIGTERM, signal.SIG_IGN); "
                f"pathlib.Path({str(child_pid_path)!r}).write_text(str(os.getpid())); "
                "time.sleep(60)"
            )
            parent = (
                "import subprocess,sys,time; "
                f"subprocess.Popen([sys.executable, '-c', {child!r}]); "
                "time.sleep(60)"
            )
            record = campaign.run_bounded(
                [sys.executable, "-c", parent],
                cwd=root,
                env=os.environ.copy(),
                duration=0.2,
                cpu=self._available_cpu(),
                log_path=root / "stubborn.log",
            )

            child_pid = int(child_pid_path.read_text())
            self.assertTrue(record.term_sent)
            self.assertTrue(record.kill_sent)
            self.assertFalse(self._pid_exists(child_pid))

    @staticmethod
    def _available_cpu():
        return min(os.sched_getaffinity(0))

    @staticmethod
    def _pid_exists(pid):
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        stat = Path(f"/proc/{pid}/stat")
        if stat.is_file() and stat.read_text().split()[2] == "Z":
            return False
        return True


class AggregationTests(unittest.TestCase):
    def test_signature_aggregation_collapses_distinct_witnesses_and_reports_unstable(self):
        campaign = load_campaign_module()
        records = [
            campaign.ReplayRecord(
                sha256="bytes-a",
                paths=("a",),
                return_pairs=((0, 1),) * 3,
                signatures=("v1-same",) * 3,
                stable=True,
                differential=True,
                discovery_elapsed_nanos=10,
            ),
            campaign.ReplayRecord(
                sha256="bytes-b",
                paths=("b",),
                return_pairs=((0, 1),) * 3,
                signatures=("v1-same",) * 3,
                stable=True,
                differential=True,
                discovery_elapsed_nanos=20,
            ),
            campaign.ReplayRecord(
                sha256="bytes-c",
                paths=("c",),
                return_pairs=((0, 1), (0, 0), (0, 1)),
                signatures=("v1-a", "v1-b", "v1-a"),
                stable=False,
                differential=False,
                discovery_elapsed_nanos=30,
            ),
        ]

        metrics = campaign.summarize_replays(records)

        self.assertEqual(metrics["stable_nonduplicate_differences"], 1)
        self.assertEqual(metrics["signature_duplicates"], 1)
        self.assertEqual(metrics["unstable_solutions"], 1)
        self.assertEqual(metrics["time_to_first_nanos"], 10)

    def test_upgrade_only_label_reuses_the_single_pinned_mode(self):
        campaign = load_campaign_module()
        manifest = campaign.CampaignManifest.load(MANIFEST)

        selected = campaign.select_configurations(manifest, "upgrade-only")

        self.assertEqual(selected, (("upgrade-only", "single-cov"),))

    def test_trial_paths_never_collide(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "campaign"
            first = campaign.create_trial(root, "baseline", 0)
            second = campaign.create_trial(root, "baseline", 0)

            self.assertNotEqual(first.root, second.root)
            for trial in (first, second):
                self.assertTrue(trial.differences.is_dir())
                self.assertTrue(trial.event_log.is_dir())
                self.assertEqual(trial.fuzzer_log.parent, trial.root)
                self.assertEqual(trial.environment_json.parent, trial.root)
                self.assertEqual(trial.trial_json.parent, trial.root)

    def test_replay_requires_three_equal_return_pairs(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            solutions = Path(directory)
            (solutions / "stable").write_bytes(b"stable difference")
            (solutions / "stable-copy").write_bytes(b"stable difference")
            (solutions / "unstable").write_bytes(b"unstable")
            (solutions / "matching").write_bytes(b"matching")
            calls = {b"unstable": 0}

            def result_for(data):
                if data == b"stable difference":
                    return [0, 1]
                if data == b"matching":
                    return [0, 0]
                calls[data] += 1
                return [0, calls[data] % 2]

            with mock.patch.object(
                campaign, "real_json_results", side_effect=result_for
            ) as results:
                records = campaign.replay_raw_solutions(solutions)

            self.assertEqual(results.call_count, 9)
            self.assertEqual(len(records), 3)
            stable = next(record for record in records if record.differential)
            unstable = next(record for record in records if not record.stable)
            self.assertEqual(stable.return_pairs, ((0, 1), (0, 1), (0, 1)))
            self.assertEqual(len(stable.paths), 2)
            self.assertFalse(unstable.differential)

    def test_zero_solution_trial_is_reported(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            paths = campaign.create_trial(Path(directory) / "campaign", "baseline", 0)
            record = self._trial(campaign, paths, stable=0, executions=100)

            summary = campaign.aggregate_trials([record])

            baseline = summary["configurations"]["baseline"]
            self.assertEqual(baseline["stable_nonduplicate_differences"], 0)
            self.assertEqual(baseline["trial_count"], 1)
            self.assertEqual(
                summary["behavioral_signature"], "diffray-v1-blake3-coverage"
            )
            self.assertEqual(baseline["trials"][0]["raw_directory"], str(paths.root))
            self.assertEqual(baseline["trials"][0]["fuzzer_log"], str(paths.fuzzer_log))

    def test_cpu_hour_denominator_uses_client_count_times_wall_time(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            campaign_root = Path(directory) / "campaign"
            first = self._trial(
                campaign,
                campaign.create_trial(campaign_root, "baseline", 0),
                stable=2,
                executions=1_000,
                wall=1800.0,
                clients=2,
            )
            second = self._trial(
                campaign,
                campaign.create_trial(campaign_root, "baseline", 1),
                stable=1,
                executions=2_000,
                wall=3600.0,
                clients=3,
            )

            summary = campaign.aggregate_trials([first, second])

            baseline = summary["configurations"]["baseline"]
            self.assertEqual(baseline["allocated_cpu_hours"], 4.0)
            self.assertEqual(
                baseline["stable_nonduplicate_differences_per_cpu_hour"], 0.75
            )
            self.assertEqual(baseline["executions"], 3_000)

    @staticmethod
    def _trial(
        campaign,
        paths,
        *,
        stable,
        executions,
        wall=300.0,
        clients=1,
    ):
        return campaign.TrialRecord(
            configuration="baseline",
            trial_index=0,
            paths=paths,
            wall_seconds=wall,
            client_count=clients,
            executions=executions,
            cpu_seconds=wall * 0.9,
            peak_rss_kb=4096,
            stable_nonduplicate_differences=stable,
            unstable_solutions=0,
            byte_duplicates=0,
            rng_seed_controlled=False,
        )


class EvidenceCompatibilityTests(unittest.TestCase):
    def test_signature_replay_accepts_explicit_version_and_policy(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.assertEqual(
                campaign.generate_signature_index(
                    root,
                    root / "missing",
                    root / "a",
                    root / "b",
                    signature_version=2,
                    result_policy="exact",
                ),
                {},
            )

    def test_backfill_signatures_preserves_environment_and_updates_v1_metrics(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            trial = root / "trial"
            differences = trial / "diffs"
            differences.mkdir(parents=True)
            solution = differences / "solution"
            solution.write_bytes(b"difference")
            evidence = root / "evidence.json"
            environment = {
                "corpus_sha256": "fixed-corpus",
                "target_sha256": {"a": "hash-a", "b": "hash-b"},
            }
            evidence.write_text(
                json.dumps(
                    {
                        "environment": environment,
                        "results": {
                            "rng_seed_controlled": False,
                            "configurations": {
                                "baseline": {
                                    "allocated_cpu_hours": 0.5,
                                    "trials": [
                                        {
                                            "raw_directory": str(trial),
                                            "stable_nonduplicate_differences": 1,
                                            "unstable_solutions": 0,
                                        }
                                    ],
                                }
                            },
                        },
                    }
                ),
                encoding="utf-8",
            )
            replay = {
                str(solution): {
                    "path": str(solution),
                    "signatures": ["same"] * 3,
                    "return_pairs": [[1, 0]] * 3,
                    "stable": True,
                    "differential": True,
                    "discovery_elapsed_nanos": None,
                }
            }

            with mock.patch.object(
                campaign, "generate_signature_index", return_value=replay
            ):
                campaign.backfill_signatures([evidence])

            updated = json.loads(evidence.read_text(encoding="utf-8"))
            self.assertEqual(updated["environment"], environment)
            self.assertFalse(updated["results"]["rng_seed_controlled"])
            metrics = updated["results"]["configurations"]["baseline"]
            self.assertEqual(metrics["stable_nonduplicate_differences"], 1)
            self.assertEqual(metrics["signature_duplicates"], 0)
            self.assertEqual(
                updated["results"]["behavioral_signature"],
                "diffray-v1-blake3-coverage",
            )

    def test_validate_sidecars_replays_every_referenced_solution(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            trial = root / "trial"
            differences = trial / "diffs"
            differences.mkdir(parents=True)
            solution = differences / "solution"
            solution.write_bytes(b"difference")
            evidence = root / "evidence.json"
            evidence.write_text(
                json.dumps(
                    {
                        "results": {
                            "configurations": {
                                "baseline": {
                                    "trials": [{"raw_directory": str(trial)}]
                                }
                            }
                        }
                    }
                ),
                encoding="utf-8",
            )

            with mock.patch.object(
                campaign, "real_json_results", return_value=[1, 0]
            ) as results, mock.patch.object(
                campaign, "real_json_validate_metadata"
            ) as validate:
                report = campaign.validate_sidecars(evidence)

            self.assertEqual(report["validated_solutions"], 1)
            results.assert_called_once_with(b"difference")
            validate.assert_called_once_with(solution, (1, 0))

    def test_compare_labels_dependency_only_and_rejects_identity_drift(self):
        campaign = load_campaign_module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            baseline = root / "baseline.json"
            candidate = root / "candidate.json"
            self._write_evidence(baseline, "baseline", "old-libafl", "same-corpus")
            self._write_evidence(
                candidate, "upgrade-only", "new-libafl", "same-corpus"
            )

            comparison = campaign.compare_evidence(baseline, candidate)

            self.assertEqual(comparison["change_scope"], "dependency-only")
            self.assertEqual(comparison["baseline"]["configuration"], "baseline")
            self.assertEqual(
                comparison["candidate"]["configuration"], "upgrade-only"
            )

            self._write_evidence(candidate, "upgrade-only", "new-libafl", "drift")
            with self.assertRaises(campaign.ManifestError):
                campaign.compare_evidence(baseline, candidate)

    @staticmethod
    def _write_evidence(path, configuration, libafl_commit, corpus_hash):
        path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "environment": {
                        "libafl_commit": libafl_commit,
                        "measurement_commit": f"{libafl_commit}-measurement",
                        "corpus_sha256": corpus_hash,
                        "target_sha256": {
                            "/root/cjson_target": "cjson-hash",
                            "/root/mjson_target": "mjson-hash",
                        },
                    },
                    "results": {
                        "configurations": {
                            configuration: {
                                "executions_per_second": 10.0,
                                "stable_nonduplicate_differences_per_cpu_hour": 2.0,
                                "stable_nonduplicate_differences": 1,
                            }
                        }
                    },
                }
            ),
            encoding="utf-8",
        )


if __name__ == "__main__":
    unittest.main()
