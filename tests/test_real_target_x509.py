import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
X509 = ROOT / "examples/real-targets/x509"
FACT_KEYS = {
    "version",
    "serial_hex",
    "subject",
    "issuer",
    "not_before",
    "not_after",
    "public_key_algorithm_oid",
    "signature_algorithm_oid",
}


class X509ContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.profile = os.environ.get("DIFFRAY_X509_TEST_PROFILE", "release")
        completed = subprocess.run(
            [
                "python3",
                str(ROOT / "scripts/real_targets.py"),
                "build",
                "--pair",
                "x509",
                "--profile",
                cls.profile,
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
            env={**os.environ, "PATH": str(ROOT / "run/toolchain/bin") + os.pathsep + os.environ["PATH"]},
        )
        cls.manifest_path = Path(completed.stdout.strip())
        cls.manifest = json.loads(cls.manifest_path.read_text())
        cls.fixtures = json.loads((X509 / "fixtures.json").read_text())["fixtures"]
        cls.by_name = {fixture["name"]: fixture for fixture in cls.fixtures}
        cls.temporary = tempfile.TemporaryDirectory(prefix="diffray-x509-contract-")

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def replay(self, role, fixture, repetition=0):
        output = Path(self.temporary.name) / f"{role}-{fixture['name']}-{repetition}.canonical"
        target = ROOT / self.manifest["targets"][role]["path"]
        completed = subprocess.run(
            [
                str(target),
                "--replay",
                str(X509 / fixture["path"]),
                "--normalized-output",
                str(output),
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
            env={
                name: value
                for name, value in os.environ.items()
                if not name.startswith(("__AFL_", "__DIFFRAY_"))
            },
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        return json.loads(completed.stdout), output

    def test_fixture_matrix_covers_complete_der_and_fact_boundaries(self):
        required = {
            "rsa-v3",
            "ec-public-key",
            "rsa-v1",
            "long-serial",
            "multi-valued-name",
            "time-variants",
            "unknown-oid",
            "trailing-byte",
            "truncated-tbs",
            "non-certificate",
            "pem",
            "indefinite-length",
            "nonminimal-length",
            "overflow-length",
            "empty",
            "exact-1mib",
            "over-1mib",
        }
        self.assertTrue(required.issubset(self.by_name))

    def test_matching_certificates_reencode_identically_and_stably(self):
        matching = [fixture for fixture in self.fixtures if fixture.get("matching")]
        self.assertGreaterEqual(len(matching), 7)
        for fixture in matching:
            with self.subTest(fixture=fixture["name"]):
                baseline = None
                for repetition in range(3):
                    reports = {}
                    outputs = {}
                    for role in ("a", "b"):
                        reports[role], output = self.replay(role, fixture, repetition)
                        self.assertEqual(reports[role]["class"], "accepted")
                        outputs[role] = output.read_bytes()
                    self.assertEqual(reports["a"]["semantic_code"], reports["b"]["semantic_code"])
                    self.assertEqual(outputs["a"], outputs["b"])
                    self.assertEqual(hashlib.sha256(outputs["a"]).hexdigest(), fixture["canonical_sha256"])
                    current = (reports, outputs)
                    if baseline is not None:
                        self.assertEqual(current, baseline)
                    baseline = current

    def test_malformed_trailing_and_size_boundaries_have_pinned_classes(self):
        for fixture in self.fixtures:
            expected = fixture.get("expected_by_target")
            if expected is None:
                continue
            with self.subTest(fixture=fixture["name"]):
                for role in ("a", "b"):
                    report, output = self.replay(role, fixture)
                    self.assertEqual(report["class"], expected[role])
                    self.assertEqual(output.exists(), expected[role] == "accepted")

    def test_replay_facts_are_deterministic_bounded_and_json_safe(self):
        for name in (
            "rsa-v3",
            "long-serial",
            "multi-valued-name",
            "time-variants",
            "unknown-oid",
            "unicode-name",
            "embedded-nul-name",
        ):
            fixture = self.by_name[name]
            for role in ("a", "b"):
                first, _ = self.replay(role, fixture, 10)
                second, _ = self.replay(role, fixture, 11)
                self.assertEqual(first, second)
                self.assertEqual(set(first["facts"]), FACT_KEYS)
                self.assertLessEqual(len(json.dumps(first["facts"], ensure_ascii=True)), 511)
                for value in first["facts"].values():
                    self.assertTrue(value is None or isinstance(value, (str, int)))

    def test_adapters_call_library_parsers_without_a_shared_der_prefilter(self):
        openssl = (X509 / "openssl_target.c").read_text()
        wolfssl = (X509 / "wolfssl_target.c").read_text()
        self.assertIn("d2i_X509(NULL, &cursor", openssl)
        self.assertIn("wolfSSL_d2i_X509(NULL, &cursor", wolfssl)
        self.assertIn("cursor != input + length", openssl)
        self.assertIn("cursor != input + length", wolfssl)
        for source in (openssl, wolfssl):
            self.assertNotIn("der_scope", source)
            self.assertNotIn("read_der_length", source)

    def test_valid_invalid_valid_sequence_never_reuses_der(self):
        valid = self.by_name["rsa-v3"]
        invalid = self.by_name["truncated-tbs"]
        for role in ("a", "b"):
            first, first_output = self.replay(role, valid, 20)
            rejected, rejected_output = self.replay(role, invalid, 21)
            final, final_output = self.replay(role, valid, 22)
            self.assertEqual((first["class"], rejected["class"], final["class"]), ("accepted", "rejected", "accepted"))
            self.assertFalse(rejected_output.exists())
            self.assertEqual(first_output.read_bytes(), final_output.read_bytes())
            self.assertEqual(first["semantic_code"], final["semantic_code"])

    def test_targets_do_not_open_trust_configuration_or_network_paths(self):
        if shutil.which("strace") is None:
            self.skipTest("strace is required for X509 trust/config audit")
        fixture = self.by_name["rsa-v3"]
        forbidden = ("/etc/ssl", "openssl.cnf", "cert.pem", "ca-bundle", "/certs/", "/dev/tcp")
        for role in ("a", "b"):
            target = ROOT / self.manifest["targets"][role]["path"]
            trace = Path(self.temporary.name) / f"{role}-openat.log"
            output = Path(self.temporary.name) / f"{role}-trace.canonical"
            completed = subprocess.run(
                [
                    "strace", "-f", "-qq", "-e", "trace=openat,connect", "-o", str(trace),
                    str(target), "--replay", str(X509 / fixture["path"]),
                    "--normalized-output", str(output),
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            lowered = trace.read_text().lower()
            self.assertFalse(any(item.lower() in lowered for item in forbidden), lowered)

    def test_pair_verifier_accepts_der_fixtures_without_image_dimensions(self):
        from scripts.real_targets_lib.campaigns import verify_pair

        summary_path = verify_pair(ROOT, "x509", self.profile)
        summary = json.loads(summary_path.read_text())
        self.assertEqual(summary["status"], "complete")
        self.assertEqual(len(summary["fixtures"]), len(self.fixtures))
        self.assertTrue(all(record["stable"] for record in summary["fixtures"]))


if __name__ == "__main__":
    unittest.main()
