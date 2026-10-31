import base64
import json
import re
import subprocess
from pathlib import Path
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]


def tracked_identity_offenders(root):
    forbidden = ("diff" + "ray").casefold()
    completed = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=root,
        capture_output=True,
        check=True,
    )
    paths = completed.stdout.decode("utf-8").split("\0")
    offenders = []
    for relative in filter(None, paths):
        if forbidden in relative.casefold():
            offenders.append((relative, "path"))
        path = root / relative
        if not path.is_file():
            continue
        try:
            content = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        if forbidden in content.casefold():
            offenders.append((relative, "content"))
    return offenders


class RepositoryLayoutTests(unittest.TestCase):
    def test_pairfuzz_is_the_only_root_package_and_product_binary(self):
        completed = subprocess.run(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        metadata = json.loads(completed.stdout)
        root = next(
            package
            for package in metadata["packages"]
            if Path(package["manifest_path"]) == ROOT / "Cargo.toml"
        )
        self.assertEqual(root["name"], "pairfuzz")
        self.assertEqual(root["default_run"], "pairfuzz")
        binary_names = {
            target["name"]
            for target in root["targets"]
            if "bin" in target["kind"]
        }
        self.assertEqual(binary_names, {"pairfuzz", "libafl_cc", "libafl_cxx"})

    def test_pairfuzz_help_uses_new_binary(self):
        former = "diff" + "ray"
        former_binary = ROOT / "target" / "debug" / former
        former_binary.unlink(missing_ok=True)

        build = subprocess.run(
            ["cargo", "build", "--locked", "--bin", "pairfuzz"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(build.returncode, 0, build.stderr)

        binary = ROOT / "target" / "debug" / "pairfuzz"
        completed = subprocess.run(
            [str(binary), "--help"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("Usage: pairfuzz", completed.stdout)
        self.assertFalse(former_binary.exists())

    def test_tracked_tree_has_no_former_identity(self):
        self.assertEqual(tracked_identity_offenders(ROOT), [])

    def test_x509_probe_uses_pairfuzz_identity_and_marks_test_key(self):
        generator = (ROOT / "scripts/generate_real_target_fixtures.py").read_text(
            encoding="utf-8"
        )
        match = re.search(
            r'X509_PROBE_DER_BASE64 = """(.*?)"""', generator, re.DOTALL
        )
        self.assertIsNotNone(match)
        probe = base64.b64decode(match.group(1))
        self.assertIn(b"PairFuzz", probe)
        self.assertNotIn(("diff" + "ray").encode("ascii"), probe)
        self.assertIn("Public deterministic test fixture only", generator)
        self.assertIn("never use this key for security or production", generator)

        seed_bytes = b"".join(
            path.read_bytes()
            for path in sorted((ROOT / "examples/real-targets/x509/seeds").iterdir())
            if path.is_file()
        )
        self.assertIn(b"PairFuzz", seed_bytes)
        self.assertNotIn(("diff" + "ray").encode("ascii"), seed_bytes)

    def test_public_repository_excludes_internal_document_artifacts(self):
        forbidden = (
            ROOT / "LICENSE-DOCS.md",
            ROOT / "LICENSES/CC-BY-SA-4.0.txt",
            ROOT / "tests/test_contest_documentation.py",
        )
        for path in forbidden:
            with self.subTest(path=path.relative_to(ROOT)):
                self.assertFalse(path.exists(), str(path))

        self.assertEqual(
            [path for path in (ROOT / "docs").rglob("*") if path.is_file()], []
        )
        tracked = set(subprocess.check_output(
            ["git", "ls-files"], cwd=ROOT, text=True
        ).splitlines())
        deleted = set(subprocess.check_output(
            ["git", "ls-files", "--deleted"], cwd=ROOT, text=True
        ).splitlines())
        tracked -= deleted
        self.assertFalse(any(path.startswith("docs/") for path in tracked))

        gitignore = (ROOT / ".gitignore").read_text(encoding="utf-8")
        contributing = (ROOT / "CONTRIBUTING.md").read_text(encoding="utf-8")
        self.assertNotIn(".superpowers/", gitignore)
        self.assertNotIn("ChinaSoft 2026", contributing)
        self.assertNotIn("## 实验与证据", contributing)

    def test_identity_guard_skips_only_undecodable_content(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            binary = root / "opaque.bin"
            binary.write_bytes(b"\xff" + ("diff" + "ray").encode("ascii"))
            subprocess.run(["git", "add", binary.name], cwd=root, check=True)
            self.assertEqual(tracked_identity_offenders(root), [])

            forbidden_path = root / (("diff" + "ray") + ".bin")
            binary.rename(forbidden_path)
            subprocess.run(["git", "add", "-A"], cwd=root, check=True)
            self.assertEqual(
                tracked_identity_offenders(root),
                [(forbidden_path.name, "path")],
            )

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
            "PAIRFUZZ_INIT()",
            "PAIRFUZZ_RETVAL(r)",
            "PAIRFUZZ_RUN(fn)",
            "PAIRFUZZ_RUN_SEMANTIC(fn)",
            "pairfuzz_result_init(void)",
            "pairfuzz_publish_result(uint64_t value)",
            "libforkserver_libafl_cc",
        ):
            with self.subTest(wrapper_token=token):
                self.assertIn(token, wrapper)

    def test_required_product_paths_exist(self):
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
            "examples/real-targets/png/contract.json",
            "examples/real-targets/bmp/contract.json",
            "examples/real-targets/x509/contract.json",
            "scripts/real_targets.py",
            "scripts/real_targets_lib/replay_report.py",
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

    def test_public_root_documents_do_not_reference_removed_paths(self):
        documents = [ROOT / "README.md", ROOT / "THIRD_PARTY.md"]
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

if __name__ == "__main__":
    unittest.main()
