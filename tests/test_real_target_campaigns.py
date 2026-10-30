import hashlib
import json
import math
import os
from datetime import date
import stat
import subprocess
import tempfile
import unittest
from unittest import mock
from pathlib import Path

from scripts.real_targets_lib.campaigns import (
    BuildManifest,
    CampaignError,
    campaign_command,
    load_build_manifest,
    replay_pair,
    run_pair,
    verify_manifests,
    verify_pair,
)
from scripts.real_targets_lib.evidence import (
    EvidenceError,
    write_native_summary,
    write_smoke_evidence,
)
from scripts.real_targets_lib.registry import pair_names


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]


class NativeCampaignTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="pairfuzz campaign with spaces ")
        self.root = Path(self.temporary.name)
        (self.root / "third_party").mkdir()
        (self.root / "examples/real-targets/png/seeds").mkdir(parents=True)
        (self.root / "examples/real-targets/png/seeds/seed").write_bytes(b"seed")
        (self.root / "target/debug").mkdir(parents=True)
        (self.root / "run/build").mkdir(parents=True)
        self.cpu = min(os.sched_getaffinity(0))

    def tearDown(self):
        self.temporary.cleanup()

    def executable(self, relative, body):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("#!/usr/bin/env python3\n" + body)
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
        return path

    def manifest(self, target_a, target_b, profile="release"):
        def record(path):
            return {
                "path": str(path.relative_to(self.root)),
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "ldd": "not a dynamic executable",
            }

        seed_path = self.root / "examples/real-targets/png/seeds/seed"
        payload = {
            "schema_version": 1,
            "status": "complete",
            "pair": "png",
            "operation": "image.png.decode.rgba8",
            "profile": profile,
            "fingerprint": "a" * 64,
            "source_sha256": {"libpng": "b" * 64},
            "seed_sha256": {
                str(seed_path.relative_to(self.root)): hashlib.sha256(seed_path.read_bytes()).hexdigest()
            },
            "targets": {"a": record(target_a), "b": record(target_b)},
        }
        path = self.root / "run/build/build.json"
        path.write_text(json.dumps(payload))
        return path

    def target(self, name, code="0000000000000001", normalized=b"same"):
        return self.executable(
            f"targets/{name}",
            "import json,pathlib,sys\n"
            f"data={normalized!r}\n"
            "pathlib.Path(sys.argv[4]).write_bytes(data)\n"
            f"print(json.dumps({{'schema_version':1,'operation':'image.png.decode.rgba8','class':'accepted','semantic_code':'{code}','width':1,'height':1,'normalized_length':len(data),'facts':{{}},'diagnostic':''}}))\n",
        )

    def test_validation_rejects_bad_duration_cpu_pair_and_profile(self):
        a, b = self.target("a"), self.target("b")
        manifest = self.manifest(a, b)
        load_build_manifest(manifest, expected_pair="png", expected_profile="release")
        with self.assertRaisesRegex(CampaignError, "profile"):
            load_build_manifest(manifest, expected_pair="png", expected_profile="asan")
        for seconds in (0, -1, math.inf, math.nan):
            with self.assertRaises(CampaignError):
                run_pair(self.root, "png", manifest, seconds, self.cpu, 7)
        with self.assertRaises(CampaignError):
            run_pair(self.root, "all", manifest, 1, self.cpu, 7)
        with self.assertRaises(CampaignError):
            run_pair(self.root, "png", manifest, 1, -1, 7)

    def test_manifest_target_hash_mismatch_is_preflight_error(self):
        a, b = self.target("a"), self.target("b")
        manifest = self.manifest(a, b)
        a.write_text(a.read_text() + "# changed\n")
        with self.assertRaisesRegex(CampaignError, "SHA-256"):
            load_build_manifest(manifest, expected_pair="png")

    def test_command_is_deterministic_and_preserves_paths_with_spaces(self):
        a, b = self.target("a"), self.target("b")
        manifest = load_build_manifest(self.manifest(a, b), expected_pair="png")
        output = self.root / "run/output with spaces"
        command = campaign_command(self.root, manifest, output, self.cpu, 0x1234)
        self.assertEqual(command[0], str((self.root / "target/debug/pairfuzz").resolve()))
        for pair in (
            ("--pipeline", "fast"), ("--mode", "single-cov"),
            ("--result-policy", "exact"), ("--signature-version", "2"),
            ("--persistent-mode", "disabled"), ("--mutator", "havoc"),
            ("--schedule", "explore"), ("--campaign-seed", "4660"),
            ("--client-core-id", str(self.cpu)),
        ):
            index = command.index(pair[0])
            self.assertEqual(command[index + 1], pair[1])
        self.assertIn(str(output / "diffs"), command)

    def test_replay_is_three_way_stable_and_detects_fingerprint_collision(self):
        input_path = self.root / "input file"
        input_path.write_bytes(b"x")
        a, b = self.target("a"), self.target("b")
        manifest = self.manifest(a, b)
        result = replay_pair(manifest, input_path, self.root / "audit")
        self.assertTrue(result["stable"])
        self.assertEqual(len(result["runs"]), 3)
        self.assertEqual(result["runs"][0]["a"]["exit_code"], 0)
        collision = self.manifest(a, self.target("different", normalized=b"different"))
        with self.assertRaisesRegex(CampaignError, "collision"):
            replay_pair(collision, input_path, self.root / "collision-audit")

    def test_saved_solution_requires_matching_three_way_coverage_signature(self):
        a = self.target("a", code="0000000000000001")
        b = self.target("b", code="0000000000000002")
        manifest = self.manifest(a, b)
        self.executable(
            "target/debug/pairfuzz",
            "import pathlib,sys,time\n"
            "args=sys.argv[1:]\n"
            "out=pathlib.Path(args[args.index('--output')+1])\n"
            "solutions=out/'client-000'/'solutions'\n"
            "solutions.mkdir(parents=True)\n"
            "(solutions/'id-000').write_bytes(b'x')\n"
            "time.sleep(10)\n",
        )

        def signature_index(_root, solution_dir, _a, _b, **kwargs):
            solution = (solution_dir / "id-000").resolve()
            self.assertEqual(kwargs, {"signature_version": 2, "result_policy": "exact"})
            return {
                str(solution): {
                    "path": str(solution),
                    "return_pairs": [[1, 2], [1, 2], [1, 2]],
                    "signatures": ["ab" * 32] * 3,
                    "stable": True,
                    "differential": True,
                    "discovery_elapsed_nanos": 7,
                }
            }

        with mock.patch(
            "scripts.real_targets_lib.campaigns.generate_signature_index",
            side_effect=signature_index,
        ):
            result = run_pair(self.root, "png", manifest, 0.2, self.cpu, 9)
        summary = json.loads((result / "summary.json").read_text())
        self.assertEqual(summary["stable_signatures"], 1)
        replay = summary["replays"][0]
        self.assertTrue(replay["stable"])
        self.assertEqual(replay["coverage_replay"]["signature"], "ab" * 32)
        self.assertTrue(replay["coverage_replay"]["standalone_codes_match"])

        def unstable_index(root, solution_dir, target_a, target_b, **kwargs):
            unstable = signature_index(root, solution_dir, target_a, target_b, **kwargs)
            record = unstable[str((solution_dir / "id-000").resolve())]
            record["signatures"][2] = "cd" * 32
            record["stable"] = False
            return unstable

        with mock.patch(
            "scripts.real_targets_lib.campaigns.generate_signature_index",
            side_effect=unstable_index,
        ):
            second = run_pair(self.root, "png", manifest, 0.2, self.cpu, 10)
        second_summary = json.loads((second / "summary.json").read_text())
        self.assertEqual(second_summary["stable_signatures"], 0)
        self.assertEqual(second_summary["unstable_or_unreproducible"], 1)

        def mismatched_index(root, solution_dir, target_a, target_b, **kwargs):
            mismatched = signature_index(root, solution_dir, target_a, target_b, **kwargs)
            mismatched[str((solution_dir / "id-000").resolve())]["return_pairs"] = [[1, 3]] * 3
            return mismatched

        with mock.patch(
            "scripts.real_targets_lib.campaigns.generate_signature_index",
            side_effect=mismatched_index,
        ):
            third = run_pair(self.root, "png", manifest, 0.2, self.cpu, 11)
        third_summary = json.loads((third / "summary.json").read_text())
        self.assertFalse(third_summary["replays"][0]["coverage_replay"]["standalone_codes_match"])
        self.assertEqual(third_summary["stable_signatures"], 0)

    def test_early_exit_fails_with_evidence_and_zero_solution_timeout_succeeds(self):
        a, b = self.target("a"), self.target("b")
        manifest = self.manifest(a, b)
        self.executable("target/debug/pairfuzz", "import sys; print('boom'); sys.exit(9)\n")
        with self.assertRaisesRegex(CampaignError, "early"):
            run_pair(self.root, "png", manifest, 0.2, self.cpu, 9)
        roots = sorted((self.root / "run/real-targets/campaigns/png").iterdir())
        self.assertTrue((roots[-1] / "fuzzer.log").is_file())
        self.executable(
            "target/debug/pairfuzz",
            "import pathlib,sys,time\n"
            "args=sys.argv[1:]\n"
            "out=pathlib.Path(args[args.index('--output')+1])\n"
            "(out/'client-000'/'solutions').mkdir(parents=True)\n"
            "time.sleep(10)\n",
        )
        with mock.patch(
            "scripts.real_targets_lib.campaigns.generate_signature_index"
        ) as signature_replay:
            result = run_pair(self.root, "png", manifest, 0.2, self.cpu, 9)
        signature_replay.assert_not_called()
        summary = json.loads((result / "summary.json").read_text())
        self.assertTrue(summary["zero_solutions"])
        self.assertEqual(summary["solutions_saved"], 0)
        self.assertNotEqual(roots[-1], result)


class BmpCampaignTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="pairfuzz bmp campaign ")
        self.root = Path(self.temporary.name)
        (self.root / "third_party").mkdir()
        (self.root / "examples/real-targets/bmp/seeds").mkdir(parents=True)
        (self.root / "examples/real-targets/png/seeds").mkdir(parents=True)
        (self.root / "examples/real-targets/bmp/seeds/seed.bmp").write_bytes(b"bmp")
        (self.root / "examples/real-targets/png/seeds/seed.png").write_bytes(b"png")
        (self.root / "target/debug").mkdir(parents=True)
        (self.root / "run/build").mkdir(parents=True)
        self.cpu = min(os.sched_getaffinity(0))

    def tearDown(self):
        self.temporary.cleanup()

    def executable(self, relative, body):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("#!/usr/bin/env python3\n" + body)
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
        return path

    def target(self, name, *, class_name="accepted", code="0000000000000001", log=None):
        logging = ""
        if log is not None:
            logging = f"pathlib.Path({str(log)!r}).open('a').write(sys.argv[2]+'\\n')\n"
        write_output = (
            "pathlib.Path(sys.argv[4]).write_bytes(b'same')\n"
            if class_name == "accepted"
            else ""
        )
        return self.executable(
            f"targets/{name}",
            "import json,pathlib,sys\n"
            + logging
            + write_output
            + f"print(json.dumps({{'schema_version':1,'operation':'image.bmp.decode.rgba8','class':'{class_name}','semantic_code':'{code}','width':1 if '{class_name}' == 'accepted' else 0,'height':1 if '{class_name}' == 'accepted' else 0,'normalized_length':4 if '{class_name}' == 'accepted' else 0,'facts':{{}},'diagnostic':''}}))\n",
        )

    def manifest(self, target_a, target_b, *, pair="bmp", profile="release"):
        def record(path):
            return {
                "path": str(path.relative_to(self.root)),
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "ldd": "not a dynamic executable",
            }

        operation = {
            "bmp": "image.bmp.decode.rgba8",
            "png": "image.png.decode.rgba8",
            "x509": "certificate.x509.parse.der",
        }[pair]
        seed = self.root / f"examples/real-targets/{pair}/seeds/seed.{pair}"
        if not seed.is_file():
            seed.parent.mkdir(parents=True, exist_ok=True)
            seed.write_bytes(pair.encode())
        payload = {
            "schema_version": 1,
            "status": "complete",
            "pair": pair,
            "operation": operation,
            "profile": profile,
            "fingerprint": "a" * 64,
            "source_sha256": {"library": "b" * 64},
            "seed_sha256": {
                str(seed.relative_to(self.root)): hashlib.sha256(seed.read_bytes()).hexdigest()
            },
            "targets": {"a": record(target_a), "b": record(target_b)},
        }
        path = self.root / "run/build" / f"{pair}.json"
        path.write_text(json.dumps(payload))
        return path

    def sleeping_fuzzer(self, *, solution=False):
        solution_lines = ""
        if solution:
            solution_lines = "(solutions/'id-000').write_bytes(b'witness')\n"
        return self.executable(
            "target/debug/pairfuzz",
            "import pathlib,sys,time\n"
            "args=sys.argv[1:]\n"
            "out=pathlib.Path(args[args.index('--output')+1])\n"
            "solutions=out/'client-000'/'solutions'\n"
            "solutions.mkdir(parents=True)\n"
            + solution_lines
            + "time.sleep(10)\n",
        )

    def test_bmp_command_uses_only_bmp_manifest_and_seed_directory(self):
        a, b = self.target("a"), self.target("b")
        manifest = load_build_manifest(self.manifest(a, b), expected_pair="bmp")
        command = campaign_command(self.root, manifest, self.root / "run/output", self.cpu, 7)
        input_index = command.index("--input")
        self.assertEqual(
            Path(command[input_index + 1]),
            (self.root / "examples/real-targets/bmp/seeds").resolve(),
        )
        self.assertNotIn("real-targets/png", " ".join(command))

    def test_bmp_rejects_png_and_x509_manifests(self):
        a, b = self.target("a"), self.target("b")
        for pair in ("png", "x509"):
            with self.subTest(pair=pair):
                with self.assertRaisesRegex(CampaignError, "does not match"):
                    load_build_manifest(self.manifest(a, b, pair=pair), expected_pair="bmp")

    def test_bmp_zero_solution_campaign_is_valid(self):
        a, b = self.target("a"), self.target("b")
        manifest = self.manifest(a, b)
        self.sleeping_fuzzer()
        result = run_pair(self.root, "bmp", manifest, 0.2, self.cpu, 7)
        summary = json.loads((result / "summary.json").read_text())
        self.assertTrue(summary["zero_solutions"])
        self.assertEqual(summary["solutions_saved"], 0)

    def test_bmp_adapter_error_seed_fails_before_campaign(self):
        a = self.target("a", class_name="adapter-error", code="4000000000000000")
        b = self.target("b")
        manifest = self.manifest(a, b)
        self.sleeping_fuzzer()
        with self.assertRaisesRegex(CampaignError, "adapter-error"):
            run_pair(self.root, "bmp", manifest, 0.2, self.cpu, 7)

    def test_bmp_saved_witness_is_replayed_three_times_by_manifest_targets(self):
        log_a = self.root / "a.log"
        log_b = self.root / "b.log"
        a = self.target("a", code="0000000000000001", log=log_a)
        b = self.target("b", code="0000000000000002", log=log_b)
        manifest = self.manifest(a, b)
        self.sleeping_fuzzer(solution=True)

        def signature_index(_root, solution_dir, _a, _b, **kwargs):
            solution = (solution_dir / "id-000").resolve()
            return {
                str(solution): {
                    "path": str(solution),
                    "return_pairs": [[1, 2]] * 3,
                    "signatures": ["ab" * 32] * 3,
                    "stable": True,
                    "differential": True,
                    "discovery_elapsed_nanos": 7,
                }
            }

        with mock.patch(
            "scripts.real_targets_lib.campaigns.generate_signature_index",
            side_effect=signature_index,
        ):
            result = run_pair(self.root, "bmp", manifest, 0.2, self.cpu, 7)
        solution = str((result / "diffs/client-000/solutions/id-000").resolve())
        self.assertEqual(log_a.read_text().splitlines().count(solution), 3)
        self.assertEqual(log_b.read_text().splitlines().count(solution), 3)
        self.assertEqual(json.loads((result / "summary.json").read_text())["stable_signatures"], 1)


class X509CampaignTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="pairfuzz x509 campaign ")
        self.root = Path(self.temporary.name)
        (self.root / "third_party").mkdir()
        (self.root / "run/build").mkdir(parents=True)
        self.input = self.root / "certificate.der"
        self.input.write_bytes(b"der")

    def tearDown(self):
        self.temporary.cleanup()

    def executable(self, relative, body):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("#!/usr/bin/env python3\n" + body)
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
        return path

    def manifest(self, target_a, target_b):
        def record(path):
            return {
                "path": str(path.relative_to(self.root)),
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            }

        payload = {
            "schema_version": 1,
            "status": "complete",
            "pair": "x509",
            "operation": "certificate.x509.parse.der",
            "profile": "asan",
            "targets": {"a": record(target_a), "b": record(target_b)},
        }
        path = self.root / "run/build/x509.json"
        path.write_text(json.dumps(payload))
        return path

    def target(self, name, *, exit_code=0):
        if exit_code:
            body = f"import sys; sys.exit({exit_code})\n"
        else:
            body = (
                "import json,pathlib,sys\n"
                "pathlib.Path(sys.argv[4]).write_bytes(b'canonical')\n"
                "print(json.dumps({'schema_version':1,'operation':'certificate.x509.parse.der','class':'accepted','semantic_code':'0000000000000001','width':0,'height':0,'normalized_length':9,'facts':{},'diagnostic':''}))\n"
            )
        return self.executable(f"targets/{name}", body)

    def test_registry_order_is_png_bmp_x509(self):
        self.assertEqual(pair_names(), ("png", "bmp", "x509"))

    def test_stable_crash_replay_has_matching_exits_and_no_normalized_file(self):
        crash = self.target("crash", exit_code=7)
        accepted = self.target("accepted")
        result = replay_pair(self.manifest(crash, accepted), self.input, self.root / "audit")
        self.assertTrue(result["stable"])
        self.assertTrue(result["differential"])
        self.assertEqual([run["a"]["exit_code"] for run in result["runs"]], [7, 7, 7])
        self.assertEqual([run["a"]["normalized_sha256"] for run in result["runs"]], [None, None, None])


class FinalGateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="pairfuzz final gate ")
        self.root = Path(self.temporary.name)
        (self.root / "third_party").mkdir()

    def tearDown(self):
        self.temporary.cleanup()

    def test_verify_all_attempts_every_pair_and_preserves_failure_report(self):
        manifests = {pair: self.root / f"{pair}.json" for pair in pair_names()}
        attempts = []

        def verifier(_root, pair, profile, manifest):
            attempts.append((pair, profile, manifest))
            if pair == "bmp":
                raise subprocess.TimeoutExpired(["bmp-target"], 30)
            path = self.root / f"{pair}-summary.json"
            path.write_text(json.dumps({"status": "complete"}))
            return path

        with mock.patch(
            "scripts.real_targets_lib.campaigns.verify_pair", side_effect=verifier
        ):
            with self.assertRaisesRegex(CampaignError, "bmp") as raised:
                verify_manifests(self.root, manifests, "release")
        self.assertEqual([attempt[0] for attempt in attempts], list(pair_names()))
        report = Path(str(raised.exception).split("evidence: ", 1)[1])
        payload = json.loads(report.read_text())
        self.assertEqual(payload["attempted"], list(pair_names()))
        self.assertEqual(payload["results"]["bmp"]["status"], "failed")

    def test_fixture_verification_rejects_an_unstable_three_run_replay(self):
        fixture_root = self.root / "examples/real-targets/png"
        fixture_root.mkdir(parents=True)
        (fixture_root / "input.png").write_bytes(b"png")
        (fixture_root / "fixtures.json").write_text(
            json.dumps(
                {
                    "fixtures": [
                        {
                            "name": "unstable",
                            "path": "input.png",
                            "expected_by_target": {"a": "rejected", "b": "rejected"},
                        }
                    ]
                }
            )
        )
        manifest_path = self.root / "manifest.json"
        manifest_path.write_text("{}")
        build = BuildManifest(
            manifest_path,
            self.root,
            {"pair": "png", "profile": "release"},
            {},
        )
        replay = {
            "stable": False,
            "differential": False,
            "runs": [
                {
                    "a": {"class": "rejected"},
                    "b": {"class": "rejected"},
                }
            ],
        }
        with (
            mock.patch(
                "scripts.real_targets_lib.campaigns.load_build_manifest",
                return_value=build,
            ),
            mock.patch(
                "scripts.real_targets_lib.campaigns.replay_pair", return_value=replay
            ),
        ):
            with self.assertRaisesRegex(CampaignError, "unstable"):
                verify_pair(
                    self.root,
                    "png",
                    "release",
                    manifest_path,
                )

    def evidence(self, pair, *, seed=7, profiles=("asan", "release"), complete=True):
        path = self.root / f"{pair}.json"
        payload = {
            "schema_version": 1,
            "label": "smoke-not-performance-ranking",
            "pair": pair,
            "operation": {
                "png": "image.png.decode.rgba8",
                "bmp": "image.bmp.decode.rgba8",
                "x509": "certificate.x509.parse.der",
            }[pair],
            "campaign_seed": seed,
            "source_versions": {"library": "1.0"},
            "profiles": {},
        }
        for profile in profiles:
            payload["profiles"][profile] = {
                "profile": profile,
                "logical_cpu": 3,
                "wall_seconds": 10.0,
                "executions": 100,
                "throughput_executions_per_wall_second": 10.0,
                "solutions_saved": 1,
                "stable_signatures": 1 if complete else 0,
                "unstable_or_unreproducible": 0 if complete else 1,
                "replay_completeness": {
                    "saved": 1,
                    "audited": 1 if complete else 0,
                    "three_run_complete": 1 if complete else 0,
                },
                "build_manifest_sha256": "a" * 64,
                "target_sha256": {"a": "b" * 64, "b": "c" * 64},
            }
        path.write_text(json.dumps(payload))
        return path

    def test_native_summary_rejects_mixed_seed_profile_or_incomplete_replay(self):
        evidences = [self.evidence(pair) for pair in pair_names()]
        summary = self.root / "summary.json"
        write_native_summary(evidences, summary)
        payload = json.loads(summary.read_text())
        self.assertEqual([item["pair"] for item in payload["pairs"]], list(pair_names()))
        self.assertTrue(all(Path(item["evidence"]).name.endswith(".json") for item in payload["pairs"]))

        with self.assertRaisesRegex(EvidenceError, "seed"):
            write_native_summary(
                [self.evidence("png"), self.evidence("bmp", seed=8), self.evidence("x509")],
                summary,
            )
        with self.assertRaisesRegex(EvidenceError, "profiles"):
            write_native_summary(
                [self.evidence("png"), self.evidence("bmp", profiles=("release",)), self.evidence("x509")],
                summary,
            )
        with self.assertRaisesRegex(EvidenceError, "replay"):
            write_native_summary(
                [self.evidence("png"), self.evidence("bmp", complete=False), self.evidence("x509")],
                summary,
            )

    def test_committed_native_evidence_matches_selected_builds(self):
        if os.environ.get("PAIRFUZZ_VERIFY_ARCHIVED_BUILDS") != "1":
            self.skipTest("historical build audit requires PAIRFUZZ_VERIFY_ARCHIVED_BUILDS=1")
        for pair in pair_names():
            evidence_path = (
                REPOSITORY_ROOT
                / "docs/contest/performance"
                / f"native-{pair}-smoke.json"
            )
            evidence = json.loads(evidence_path.read_text(encoding="utf-8"))
            for profile in ("asan", "release"):
                with self.subTest(pair=pair, profile=profile):
                    record = evidence["profiles"][profile]
                    manifest_path = REPOSITORY_ROOT / record["raw_build_manifest"]
                    self.assertTrue(manifest_path.is_file(), manifest_path)
                    self.assertEqual(
                        hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
                        record["build_manifest_sha256"],
                    )
                    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
                    expected_targets = {
                        role: manifest["targets"][role]["sha256"]
                        for role in ("a", "b")
                    }
                    self.assertEqual(record["target_sha256"], expected_targets)
                    for role in ("a", "b"):
                        target = REPOSITORY_ROOT / manifest["targets"][role]["path"]
                        self.assertTrue(target.is_file(), target)
                        self.assertEqual(
                            hashlib.sha256(target.read_bytes()).hexdigest(),
                            expected_targets[role],
                        )

    def test_committed_zero_solution_records_have_zero_replay_counts(self):
        for pair in pair_names():
            evidence_path = (
                REPOSITORY_ROOT
                / "docs/contest/performance"
                / f"native-{pair}-smoke.json"
            )
            evidence = json.loads(evidence_path.read_text(encoding="utf-8"))
            for profile, record in evidence["profiles"].items():
                with self.subTest(pair=pair, profile=profile):
                    saved = record["solutions_saved"]
                    completeness = record["replay_completeness"]
                    self.assertEqual(completeness["saved"], saved)
                    self.assertEqual(completeness["audited"], saved)
                    self.assertEqual(completeness["three_run_complete"], saved)
                    if saved == 0:
                        self.assertTrue(record["zero_solutions"])
                        self.assertEqual(
                            completeness,
                            {"saved": 0, "audited": 0, "three_run_complete": 0},
                        )

    def test_smoke_export_rejects_a_mixed_build_manifest(self):
        dependencies = ("zlib", "libpng", "libspng", "xxhash", "xxhash-license")
        (self.root / "third_party/real-targets.lock.json").write_text(
            json.dumps(
                {
                    "sources": [
                        {"name": name, "version": "1", "sha256": name * 8}
                        for name in dependencies
                    ]
                }
            )
        )
        summaries = {}
        for profile in ("asan", "release"):
            directory = self.root / "run" / profile
            directory.mkdir(parents=True)
            manifest = directory / "build.json"
            manifest_payload = {
                "pair": "png",
                "profile": profile,
                "fingerprint": profile[0] * 64,
                "source_sha256": {name: name * 8 for name in dependencies},
                "targets": {
                    "a": {"sha256": "a" * 64},
                    "b": {"sha256": "b" * 64},
                },
            }
            manifest.write_text(json.dumps(manifest_payload))
            manifest_sha = hashlib.sha256(manifest.read_bytes()).hexdigest()
            (directory / "campaign.json").write_text(
                json.dumps(
                    {
                        "manifest": str(manifest),
                        "seconds": 10,
                        "cpu": 3,
                    }
                )
            )
            summary = directory / "summary.json"
            summary.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "status": "complete",
                        "pair": "png",
                        "profile": profile,
                        "operation": "image.png.decode.rgba8",
                        "seed": 7,
                        "seed_sha256": {"seed": "c" * 64},
                        "build_manifest_sha256": manifest_sha,
                        "target_sha256": {"a": "a" * 64, "b": "b" * 64},
                        "wall_seconds": 10.0,
                        "cpu_seconds": 9.0,
                        "peak_rss_kb": 1,
                        "executions": 100,
                        "solutions_saved": 0,
                        "stable_signatures": 0,
                        "unstable_or_unreproducible": 0,
                        "zero_solutions": True,
                        "replays": [],
                    }
                )
            )
            summaries[profile] = summary
        output = self.root / "smoke.json"
        write_smoke_evidence(self.root, "png", summaries, output, host={})
        compact = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(compact["recorded_at"], date.today().isoformat())
        for record in compact["profiles"].values():
            self.assertTrue(record["zero_solutions"])
            self.assertEqual(
                record["replay_completeness"],
                {"saved": 0, "audited": 0, "three_run_complete": 0},
            )

        release = json.loads(summaries["release"].read_text())
        release["solutions_saved"] = 1
        release["stable_signatures"] = 1
        release["zero_solutions"] = False
        release["replays"] = [
            {
                "runs": [
                    {"a": {"class": "accepted"}, "b": {"class": "rejected"}},
                    {"a": {"class": "accepted"}, "b": {"class": "rejected"}},
                ]
            }
        ]
        summaries["release"].write_text(json.dumps(release))
        with self.assertRaisesRegex(EvidenceError, "three-run"):
            write_smoke_evidence(self.root, "png", summaries, output, host={})

        release["solutions_saved"] = 0
        release["stable_signatures"] = 0
        release["zero_solutions"] = True
        release["replays"] = []
        summaries["release"].write_text(json.dumps(release))
        release = json.loads(summaries["release"].read_text())
        release["target_sha256"]["b"] = "d" * 64
        summaries["release"].write_text(json.dumps(release))
        with self.assertRaisesRegex(EvidenceError, "manifest"):
            write_smoke_evidence(self.root, "png", summaries, output, host={})

    def test_final_recipe_and_docs_cover_all_release_obligations(self):
        justfile = (Path(__file__).parents[1] / "Justfile").read_text()
        documentation = (Path(__file__).parents[1] / "docs/contest/real-native-targets.md").read_text()
        self.assertIn("real-targets-check", justfile)
        for required in (
            "fetch --pair all",
            "--profile asan",
            "--profile release",
            "real-json-verify",
            "verify --pair all",
        ):
            self.assertIn(required, justfile)
        for required in (
            "OpenSSL",
            "wolfSSL",
            "GPL-3.0-or-later",
            "certificate.x509.parse.der",
            "每个差分覆盖路径保存首个语义差异",
            "adapter-error",
        ):
            self.assertIn(required, documentation)


if __name__ == "__main__":
    unittest.main()
