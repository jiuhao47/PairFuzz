import hashlib
import json
import os
import sys
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PNG = ROOT / "examples" / "real-targets" / "png"


class PngContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if "discover" in sys.argv and os.environ.get("DIFFRAY_REAL_TARGET_INTEGRATION") != "1":
            raise unittest.SkipTest("native PNG integration is exercised by its explicit release gate")
        cls.contract_path = PNG / "contract.json"
        cls.fixtures_path = PNG / "fixtures.json"
        if not cls.contract_path.is_file() or not cls.fixtures_path.is_file():
            raise AssertionError("PNG contract and fixtures must exist; this integration suite never skips")
        cls.contract = json.loads(cls.contract_path.read_text())
        cls.fixtures = json.loads(cls.fixtures_path.read_text())["fixtures"]
        cls.profile = os.environ.get("DIFFRAY_PNG_TEST_PROFILE", "release")
        completed = subprocess.run(
            [
                "python3",
                str(ROOT / "scripts" / "real_targets.py"),
                "build",
                "--pair",
                "png",
                "--profile",
                cls.profile,
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        cls.manifest = json.loads(Path(completed.stdout.strip()).read_text())
        cls.temporary = tempfile.TemporaryDirectory(prefix="diffray-png-contract-")

    @classmethod
    def tearDownClass(cls):
        if hasattr(cls, "temporary"):
            cls.temporary.cleanup()

    def replay(self, target, fixture):
        output = Path(self.temporary.name) / f"{target}-{fixture['name']}.canonical"
        binary = ROOT / self.manifest["targets"][target]["path"]
        completed = subprocess.run(
            [
                str(binary),
                "--replay",
                str(PNG / fixture["path"]),
                "--normalized-output",
                str(output),
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertIn(completed.returncode, (0,), completed.stderr)
        report = json.loads(completed.stdout)
        return report, output

    def test_contract_pins_conversion_and_resource_policy(self):
        self.assertEqual(self.contract["schema_version"], 1)
        self.assertEqual(self.contract["operation"], "image.png.decode.rgba8")
        self.assertEqual(self.contract["semantic_schema"], 1)
        self.assertEqual(self.contract["input_limit"], 1048576)
        self.assertEqual(self.contract["max_dimension"], 4096)
        self.assertEqual(self.contract["max_pixels"], 4194304)
        self.assertEqual(self.contract["sample_16_to_8"], "high-byte")
        self.assertFalse(self.contract["apply_gamma"])
        self.assertFalse(self.contract["apply_sbit"])

    def test_fixture_matrix_covers_required_png_behaviors(self):
        required = {
            "rgb8",
            "rgba8",
            "palette-trns",
            "gray-low-bit",
            "gray-alpha",
            "rgba16",
            "adam7",
            "gamma-srgb",
            "truncated",
            "bad-crc",
            "missing-iend",
            "bad-iend",
            "apng",
            "width-4096",
            "width-4097",
            "zero-width",
            "pixel-limit",
            "pixel-over-limit",
        }
        self.assertTrue(required.issubset({fixture["name"] for fixture in self.fixtures}))

    def test_both_decoders_match_independent_valid_fixture_expectations(self):
        for fixture in self.fixtures:
            if not fixture.get("matching"):
                continue
            with self.subTest(fixture=fixture["name"]):
                reports = {}
                outputs = {}
                for target in ("a", "b"):
                    reports[target], output = self.replay(target, fixture)
                    outputs[target] = output.read_bytes()
                    self.assertEqual(reports[target]["class"], fixture["expected_class"])
                    self.assertEqual(
                        (reports[target]["width"], reports[target]["height"]),
                        tuple(fixture["dimensions"]),
                    )
                    self.assertEqual(hashlib.sha256(outputs[target]).hexdigest(), fixture["canonical_sha256"])
                self.assertEqual(reports["a"]["semantic_code"], reports["b"]["semantic_code"])
                self.assertEqual(outputs["a"], outputs["b"])

    def test_malformed_scope_and_resource_fixtures_have_pinned_per_library_classes(self):
        for fixture in self.fixtures:
            expected = fixture.get("expected_by_target")
            if expected is None:
                continue
            with self.subTest(fixture=fixture["name"]):
                for target in ("a", "b"):
                    report, output = self.replay(target, fixture)
                    self.assertEqual(report["class"], expected[target])
                    if report["class"] != "accepted":
                        self.assertFalse(output.exists())

    def test_terminal_failure_after_success_does_not_reuse_pixels(self):
        by_name = {fixture["name"]: fixture for fixture in self.fixtures}
        self.replay("a", by_name["rgba8"])
        report, output = self.replay("a", by_name["bad-iend"])
        self.assertNotEqual(report["class"], "accepted")
        self.assertEqual(report["normalized_length"], 0)
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
