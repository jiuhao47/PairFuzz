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


if __name__ == "__main__":
    unittest.main()
