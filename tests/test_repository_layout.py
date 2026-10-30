import json
import subprocess
from pathlib import Path
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]


class RepositoryLayoutTests(unittest.TestCase):
    def test_performance_evidence_uses_reachable_sanitized_project_commits(self):
        expected_migration = {
            "schema_version": 1,
            "project_commit_ids": "mapped_to_sanitized_history",
        }
        for relative in (
            "docs/contest/performance/baseline-0.15.4.json",
            "docs/contest/performance/upgrade-only-0.16.1.json",
        ):
            with self.subTest(evidence=relative):
                evidence = json.loads((ROOT / relative).read_text(encoding="utf-8"))
                self.assertEqual(
                    evidence.get("history_privacy_migration"),
                    expected_migration,
                )
                environment = evidence["environment"]
                for field in ("source_baseline", "measurement_commit"):
                    value = environment[field]
                    self.assertRegex(value, r"^[0-9a-f]{40}$")
                    result = subprocess.run(
                        ["git", "merge-base", "--is-ancestor", value, "HEAD"],
                        cwd=ROOT,
                        capture_output=True,
                        check=False,
                    )
                    self.assertEqual(result.returncode, 0, field)

    def test_rust_entrypoint_is_thin_and_runtime_is_separate(self):
        runtime = ROOT / "src" / "driver" / "runtime.rs"
        self.assertTrue(runtime.is_file())

        main = (ROOT / "src" / "main.rs").read_text(encoding="utf-8")
        self.assertLessEqual(len(main.splitlines()), 80)
        for token in (
            "ForkserverExecutor",
            "DiffExecutor",
            "StdState",
            "fuzz_loop",
            "ReplayCov",
        ):
            with self.subTest(token=token):
                self.assertNotIn(token, main)

    def test_runtime_selects_clients_but_does_not_build_pipeline(self):
        client = ROOT / "src" / "driver" / "client.rs"
        pipeline = ROOT / "src" / "driver" / "pipeline.rs"
        self.assertTrue(client.is_file())
        self.assertTrue(pipeline.is_file())
        runtime = (ROOT / "src" / "driver" / "runtime.rs").read_text(
            encoding="utf-8"
        )
        self.assertIn("run_legacy_client", runtime)
        self.assertIn("run_fast_client", runtime)
        for token in ("ForkserverExecutor", "StdState", "StdMOptMutator", "fuzz_loop"):
            with self.subTest(forbidden=token):
                self.assertNotIn(token, runtime)

    def test_legacy_client_keeps_upgrade_checkpoint_algorithm(self):
        client = (ROOT / "src" / "driver" / "client.rs").read_text(
            encoding="utf-8"
        )
        for token in (
            "QueueScheduler",
            "HavocScheduledMutator",
            "ShmemComboUpdater",
            "SimpleRestartingEventManager",
        ):
            with self.subTest(required=token):
                self.assertIn(token, client)
        for token in ("StdMOptMutator", "CentralizedLauncher", "CachedOnDiskCorpus"):
            with self.subTest(forbidden=token):
                self.assertNotIn(token, client)

    def test_compiler_checkpoint_keeps_normal_instrumentation_abi(self):
        manifest = (ROOT / "app" / "compiler" / "Cargo.toml").read_text(
            encoding="utf-8"
        )
        wrapper = (
            ROOT / "app" / "compiler" / "src" / "bin" / "libafl_forkserver_cc.rs"
        ).read_text(encoding="utf-8")

        for token in (
            'name = "libforkserver_libafl_cc"',
            'name = "libafl_forkserver_cc"',
            'name = "libafl_forkserver_cxx"',
            '"sancov_pcguard_hitcounts"',
            '"pointer_maps"',
        ):
            with self.subTest(manifest_token=token):
                self.assertIn(token, manifest)
        for token in (
            "-fsanitize-coverage=trace-pc-guard",
            "DIFFRAY_INIT()",
            "DIFFRAY_RETVAL(r)",
            "DIFFRAY_RUN(fn)",
            "libforkserver_libafl_cc",
        ):
            with self.subTest(wrapper_token=token):
                self.assertIn(token, wrapper)

    def test_required_competition_paths_exist(self):
        required = (
            "src/main.rs",
            "app/compiler/Cargo.toml",
            "app/compiler/Cargo.lock",
            "examples/real-json/Makefile",
            "examples/contest/Makefile",
            "scripts/real_json_demo.py",
            "scripts/contest_demo.py",
            "scripts/performance_campaign.py",
            "examples/real-json/performance-campaign.json",
            "third_party/json-libs.json",
            "tests/test_real_json_demo.py",
            "LibAFL",
            "README.md",
            "Justfile",
            "docs/contest/README.md",
            "docs/contest/real-native-targets.md",
            "docs/contest/performance/README.md",
            "docs/contest/performance/native-png-smoke.json",
            "docs/contest/performance/native-bmp-smoke.json",
            "docs/contest/performance/native-x509-smoke.json",
            "docs/contest/performance/native-targets-summary.json",
            "examples/real-targets/png/contract.json",
            "examples/real-targets/bmp/contract.json",
            "examples/real-targets/x509/contract.json",
            "scripts/real_targets.py",
            "scripts/real_targets_lib/evidence.py",
            "third_party/real-targets.lock.json",
        )
        for relative in required:
            with self.subTest(path=relative):
                self.assertTrue((ROOT / relative).exists(), relative)

    def test_research_only_paths_are_absent(self):
        removed = (
            ".github",
            ".dockerignore",
            "benchmark",
            "docker",
            "experiments",
            "misc",
            "app/.gitignore",
            "app/bins",
            "app/differential_libs",
            "app/libs",
            "app/script",
            "app/tools",
            "app/verify",
        )
        for relative in removed:
            with self.subTest(path=relative):
                self.assertFalse((ROOT / relative).exists(), relative)

    def test_libafl_gitlink_is_pinned(self):
        completed = subprocess.run(
            ["git", "ls-files", "--stage", "--", "LibAFL"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        fields = completed.stdout.split()
        self.assertGreaterEqual(len(fields), 4)
        self.assertEqual(fields[0], "160000")
        self.assertEqual(
            fields[1], "92ad89454765440c7c03b87b11d5dbde753f9633"
        )

    def test_rust_toolchain_matches_libafl_minimum(self):
        with (ROOT / "rust-toolchain.toml").open("rb") as source:
            toolchain = tomllib.load(source)["toolchain"]

        self.assertEqual(toolchain["channel"], "1.93.1")
        self.assertEqual(toolchain["profile"], "minimal")
        self.assertEqual(toolchain["components"], ["rustfmt"])

    def test_root_justfile_exposes_only_supported_recipes(self):
        completed = subprocess.run(
            ["just", "--summary"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(
            set(completed.stdout.split()),
            {
                "check",
                "contest-build",
                "contest-demo",
                "contest-replay",
                "contest-verify",
                "contest-paper",
                "contest-paper-clean",
                "real-json-build",
                "real-json-demo",
                "real-json-replay",
                "real-json-verify",
                "performance-build",
                "performance-smoke",
                "performance-baseline",
                "real-targets-fetch",
                "real-targets-build",
                "real-targets-verify",
                "real-targets-verify-all",
                "real-targets-run",
                "real-targets-replay",
                "real-targets-x509-run",
                "real-targets-x509-replay",
                "real-targets-check",
            },
        )

    def test_performance_baseline_builds_targets_before_trials(self):
        completed = subprocess.run(
            ["just", "--dry-run", "performance-baseline", "0"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        commands = completed.stdout + completed.stderr
        build = "python3 scripts/real_json_demo.py build"
        run = "python3 scripts/performance_campaign.py run"
        self.assertIn(build, commands)
        self.assertIn(run, commands)
        self.assertLess(commands.index(build), commands.index(run))

    def test_performance_runner_exposes_measurement_commands(self):
        completed = subprocess.run(
            ["python3", "scripts/performance_campaign.py", "--help"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        for command in ("run", "aggregate", "export"):
            with self.subTest(command=command):
                self.assertIn(command, completed.stdout)

    def test_check_builds_synthetic_targets_before_verifying_them(self):
        completed = subprocess.run(
            ["just", "--dry-run", "check"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        commands = completed.stdout + completed.stderr
        build = "python3 scripts/contest_demo.py build"
        verify = "python3 scripts/contest_demo.py verify"
        self.assertIn(build, commands)
        self.assertIn(verify, commands)
        self.assertLess(commands.index(build), commands.index(verify))

    def test_root_justfile_has_no_removed_or_absolute_paths(self):
        content = (ROOT / "Justfile").read_text(encoding="utf-8")
        forbidden = (
            "benchmark/",
            "docker/",
            "experiments/",
            "misc/",
            "app/bins/",
            "app/differential_libs/",
            "/home/",
            "/root/",
        )
        for token in forbidden:
            with self.subTest(token=token):
                self.assertNotIn(token, content)

    def test_participant_docs_do_not_reference_removed_paths(self):
        documents = [ROOT / "README.md", ROOT / "docs" / "STRUCTURE.md"]
        documents.extend(
            path
            for path in sorted((ROOT / "docs" / "contest").glob("*.md"))
            if path.name != "provenance.md"
        )
        forbidden = (
            "benchmark/",
            "docker/",
            "experiments/",
            "misc/",
            "app/bins/",
            "app/differential_libs/",
            "app/libs/",
            "app/script/",
            "app/tools/",
            "app/verify/",
            "LocateFix",
            "fuzzers/",
        )
        for document in documents:
            content = document.read_text(encoding="utf-8")
            for token in forbidden:
                with self.subTest(document=document.name, token=token):
                    self.assertNotIn(token, content)

    def test_provenance_records_source_archive_recovery_point(self):
        content = (ROOT / "docs" / "contest" / "provenance.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("私有源码归档", content)
        self.assertNotIn("internal-" + "source-remote", content)
        self.assertIn("dcf9efb0347e488b7a1e12f6ec42fc5299539035", content)

    def test_structure_lists_every_retained_product_area(self):
        content = (ROOT / "docs" / "STRUCTURE.md").read_text(encoding="utf-8")
        retained = (
            "src/",
            "app/compiler/",
            "examples/real-json/",
            "examples/contest/",
            "examples/real-targets/",
            "scripts/",
            "tests/",
            "third_party/",
            "docs/contest/",
            "LibAFL/",
            "driver/runtime.rs",
            "driver/monitor.rs",
            "driver/target.rs",
            "driver/replay.rs",
            "feedbacks/factory.rs",
            "real-native-targets.md",
        )
        for path in retained:
            with self.subTest(path=path):
                self.assertIn(path, content)


if __name__ == "__main__":
    unittest.main()
