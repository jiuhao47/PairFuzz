import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "third_party" / "json-libs.json"


def load_demo():
    path = ROOT / "scripts" / "real_json_demo.py"
    spec = importlib.util.spec_from_file_location("real_json_demo", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class VendorManifestTests(unittest.TestCase):
    def test_exact_versions_and_hashes(self):
        expected = {
            "cJSON": (
                "1.7.19",
                "v1.7.19",
                "c859b25da02955fef659d658b8f324b5cde87be3",
            ),
            "mjson": (
                "1.2.7",
                "1.2.7",
                "032a2eaca1a989e56d88218f2a3cb91a68afebe8",
            ),
        }
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        libraries = {item["name"]: item for item in manifest["libraries"]}
        self.assertEqual(set(libraries), set(expected))

        third_party = (ROOT / "third_party").resolve()
        for name, (version, tag, commit) in expected.items():
            library = libraries[name]
            self.assertEqual(library["version"], version)
            self.assertEqual(library["tag"], tag)
            self.assertEqual(library["commit"], commit)
            self.assertTrue(library["source_url"].startswith("https://github.com/"))
            self.assertTrue(library["files"])
            for relative, wanted_hash in library["files"].items():
                path = (ROOT / relative).resolve()
                path.relative_to(third_party)
                self.assertTrue(path.is_file(), relative)
                actual_hash = hashlib.sha256(path.read_bytes()).hexdigest()
                self.assertEqual(actual_hash, wanted_hash, relative)


class VendorVerifierTests(unittest.TestCase):
    def test_accepts_the_pinned_manifest(self):
        demo = load_demo()
        demo.verify_vendor_manifest(ROOT)

    def test_rejects_a_path_outside_third_party(self):
        demo = load_demo()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "third_party").mkdir()
            outside = root / "outside.txt"
            outside.write_bytes(b"not vendored")
            digest = hashlib.sha256(outside.read_bytes()).hexdigest()
            manifest = {
                "schema_version": 1,
                "libraries": [
                    {
                        "name": "escape",
                        "version": "1",
                        "tag": "1",
                        "commit": "0" * 40,
                        "source_url": "https://github.com/example/escape",
                        "files": {"third_party/../outside.txt": digest},
                    }
                ],
            }
            (root / "third_party" / "json-libs.json").write_text(
                json.dumps(manifest), encoding="utf-8"
            )
            with self.assertRaises(demo.ManifestError):
                demo.verify_vendor_manifest(root)


class TargetContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.demo = load_demo()
        cls.demo.build()

    def test_shared_object_parser_contract(self):
        cases = [
            (b'{"x":1}', [0, 0]),
            (b'  {"x":[true,null]}\n', [0, 0]),
            (b'[]', [1, 1]),
            (b'null', [1, 1]),
            (b'{"x":1}junk', [1, 1]),
            (b'{"x":1}\x0b', [1, 1]),
            (b'{"x":1}\x0c', [1, 1]),
            (b'\x0b{"x":1}', [1, 1]),
            (b'{"x":1}\x00', [1, 1]),
            (b'', [100, 100]),
        ]
        for data, expected in cases:
            with self.subTest(data=data):
                self.assertEqual(self.demo.results(data), expected)

    def test_starting_seeds_are_not_already_differential(self):
        seeds = ROOT / "examples" / "real-json" / "seeds"
        for seed in sorted(seeds.iterdir()):
            with self.subTest(seed=seed.name):
                left, right = self.demo.results(seed.read_bytes())
                self.assertEqual(left, right)

    def test_known_invalid_escape_is_a_real_parser_difference(self):
        known = ROOT / "examples" / "real-json" / "known-difference.json"
        self.assertEqual(self.demo.results(known.read_bytes()), [1, 0])

    def test_one_bit_seed_mutation_reaches_a_real_difference(self):
        seed = ROOT / "examples" / "real-json" / "seeds" / "near-comma.json"
        data = seed.read_bytes()
        self.assertEqual(self.demo.results(data), [1, 1])
        mutated = data.replace(b"-}", b",}")
        self.assertEqual(len(mutated), len(data))
        self.assertEqual(sum(left != right for left, right in zip(data, mutated)), 1)
        self.assertEqual(self.demo.results(mutated), [1, 0])

    def test_campaign_seed_has_many_one_byte_paths_to_a_real_difference(self):
        seed = (
            ROOT
            / "examples"
            / "real-json"
            / "campaign-seeds"
            / "escape-rich.json"
        )
        data = seed.read_bytes()
        self.assertEqual(self.demo.results(data), [0, 0])
        escape_values = [
            index + 1
            for index in range(len(data) - 1)
            if data[index : index + 2] == b"\\n"
        ]
        self.assertGreaterEqual(len(escape_values), 8)
        for index in escape_values:
            mutated = bytearray(data)
            mutated[index] = ord("q")
            with self.subTest(index=index):
                self.assertEqual(self.demo.results(bytes(mutated)), [1, 0])


class WorkflowTests(unittest.TestCase):
    def setUp(self):
        self.demo = load_demo()

    @staticmethod
    def _metadata(rv_a=1, rv_b=0, note="rv_zero_nonzero"):
        return {
            "metadata": {
                "map": {
                    "123": [
                        123,
                        {"note": note, "rv_a": rv_a, "rv_b": rv_b, "crash": None},
                    ]
                }
            },
            "exec_time": None,
            "executions": 42,
        }

    def test_duration_must_be_positive_and_finite(self):
        for value in ("0", "-1", "nan", str(math.inf)):
            with self.subTest(value=value):
                with self.assertRaises(argparse.ArgumentTypeError):
                    self.demo.positive_seconds(value)
        self.assertEqual(self.demo.positive_seconds("0.25"), 0.25)

    def test_metadata_must_exist_and_be_json(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = Path(directory) / "difference"
            saved.write_bytes(b'{"x":"\\q"}')
            with self.assertRaises(RuntimeError):
                self.demo.validate_metadata(saved, (1, 0))
            Path(f"{saved}.metadata").write_text("not json", encoding="utf-8")
            with self.assertRaises(RuntimeError):
                self.demo.validate_metadata(saved, (1, 0))

    def test_metadata_requires_note_and_matching_return_values(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = Path(directory) / "difference"
            saved.write_bytes(b'{"x":"\\q"}')
            sidecar = Path(f"{saved}.metadata")
            for payload in (
                self._metadata(note=None),
                self._metadata(rv_a=0, rv_b=1),
                self._metadata(rv_a=True, rv_b=False),
                self._metadata(rv_a=1.0, rv_b=0.0),
            ):
                with self.subTest(payload=payload):
                    sidecar.write_text(json.dumps(payload), encoding="utf-8")
                    with self.assertRaises(RuntimeError):
                        self.demo.validate_metadata(saved, (1, 0))

    def test_metadata_rejects_a_non_numeric_collision_suffix(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = Path(directory) / "difference"
            saved.write_bytes(b'{"x":"\\q"}')
            unrelated = saved.parent / f".{saved.name}_other.metadata"
            unrelated.write_text(
                json.dumps(self._metadata()), encoding="utf-8"
            )
            with self.assertRaises(RuntimeError):
                self.demo.validate_metadata(saved, (1, 0))

    def test_metadata_returns_the_matching_record(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = Path(directory) / "difference"
            saved.write_bytes(b'{"x":"\\q"}')
            sidecar = saved.parent / f".{saved.name}_1.metadata"
            sidecar.write_text(json.dumps(self._metadata()), encoding="utf-8")
            record = self.demo.validate_metadata(saved, (1, 0))
            self.assertEqual(record["note"], "rv_zero_nonzero")
            self.assertEqual((record["rv_a"], record["rv_b"]), (1, 0))

    def test_early_campaign_exit_is_an_error_and_is_reaped(self):
        process = subprocess.Popen(
            [sys.executable, "-c", "pass"], start_new_session=True
        )
        with self.assertRaises(RuntimeError):
            self.demo.wait_for_campaign(process, 1.0)
        self.assertIsNotNone(process.poll())

    def test_stop_group_escalates_and_waits(self):
        class StubbornProcess:
            pid = 1234

            def __init__(self):
                self.wait_count = 0

            def wait(self, timeout=None):
                self.wait_count += 1
                if self.wait_count == 1:
                    raise subprocess.TimeoutExpired("target", timeout)
                return -signal.SIGKILL

        process = StubbornProcess()
        with mock.patch.object(os, "killpg") as killpg:
            self.demo.stop_group(process)
        self.assertEqual(
            killpg.call_args_list,
            [mock.call(1234, signal.SIGTERM), mock.call(1234, signal.SIGKILL)],
        )
        self.assertEqual(process.wait_count, 2)

    @unittest.skipUnless(os.name == "posix", "requires POSIX process groups")
    def test_stop_group_kills_a_child_after_the_leader_exits(self):
        with tempfile.TemporaryDirectory() as directory:
            ready = Path(directory) / "ready"
            leader = subprocess.Popen(
                [sys.executable, "-c", "import time; time.sleep(30)"],
                process_group=0,
            )
            child = subprocess.Popen(
                [
                    sys.executable,
                    "-c",
                    (
                        "import pathlib, signal, sys, time; "
                        "signal.signal(signal.SIGTERM, signal.SIG_IGN); "
                        "pathlib.Path(sys.argv[1]).write_text('ready'); "
                        "time.sleep(30)"
                    ),
                    str(ready),
                ],
                process_group=leader.pid,
            )
            try:
                for _ in range(100):
                    if ready.is_file():
                        break
                    time.sleep(0.01)
                self.assertTrue(ready.is_file(), "child did not become ready")
                self.demo.stop_group(leader)
                self.assertEqual(child.wait(timeout=1), -signal.SIGKILL)
            finally:
                for process in (leader, child):
                    if process.poll() is None:
                        process.kill()
                    process.wait()


class DocumentationTests(unittest.TestCase):
    def test_documented_real_json_recipes_are_available(self):
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        commands = {
            "real-json-build": [],
            "real-json-verify": [],
            "real-json-demo": ["30"],
            "real-json-replay": [],
        }
        for recipe, arguments in commands.items():
            with self.subTest(recipe=recipe):
                self.assertIn(f"just {recipe}", readme)
                subprocess.run(
                    ["just", "--dry-run", recipe, *arguments],
                    cwd=ROOT,
                    capture_output=True,
                    text=True,
                    check=True,
                )

    def test_provenance_records_exact_upstream_revisions(self):
        provenance = (ROOT / "docs" / "contest" / "provenance.md").read_text(
            encoding="utf-8"
        )
        expected = (
            ("cJSON", "1.7.19", "c859b25da02955fef659d658b8f324b5cde87be3"),
            ("mjson", "1.2.7", "032a2eaca1a989e56d88218f2a3cb91a68afebe8"),
        )
        for name, version, commit in expected:
            with self.subTest(library=name):
                self.assertIn(name, provenance)
                self.assertIn(version, provenance)
                self.assertIn(commit, provenance)


if __name__ == "__main__":
    unittest.main()
