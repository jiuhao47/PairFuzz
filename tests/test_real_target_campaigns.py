import hashlib
import json
import math
import os
import stat
import tempfile
import unittest
from unittest import mock
from pathlib import Path

from scripts.real_targets_lib.campaigns import (
    CampaignError,
    campaign_command,
    load_build_manifest,
    replay_pair,
    run_pair,
)


class NativeCampaignTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="diffray campaign with spaces ")
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
            "target/debug/diffray",
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
        self.executable("target/debug/diffray", "import sys; print('boom'); sys.exit(9)\n")
        with self.assertRaisesRegex(CampaignError, "early"):
            run_pair(self.root, "png", manifest, 0.2, self.cpu, 9)
        roots = sorted((self.root / "run/real-targets/campaigns/png").iterdir())
        self.assertTrue((roots[-1] / "fuzzer.log").is_file())
        self.executable(
            "target/debug/diffray",
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
        self.temporary = tempfile.TemporaryDirectory(prefix="diffray bmp campaign ")
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
            "target/debug/diffray",
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


if __name__ == "__main__":
    unittest.main()
