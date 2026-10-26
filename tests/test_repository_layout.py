import subprocess
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]


class RepositoryLayoutTests(unittest.TestCase):
    def test_required_competition_paths_exist(self):
        required = (
            "src/main.rs",
            "app/compiler/Cargo.toml",
            "app/compiler/Cargo.lock",
            "examples/real-json/Makefile",
            "examples/contest/Makefile",
            "scripts/real_json_demo.py",
            "scripts/contest_demo.py",
            "third_party/json-libs.json",
            "tests/test_real_json_demo.py",
            "LibAFL",
            "README.md",
            "Justfile",
            "docs/contest/README.md",
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
            fields[1], "90cb9a2919faf386e0678870e52784070cdac4b6"
        )

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
            },
        )

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

    def test_provenance_records_remote_recovery_point(self):
        content = (ROOT / "docs" / "contest" / "provenance.md").read_text(
            encoding="utf-8"
        )
        self.assertIn("internal-source-remote", content)
        self.assertIn("dcf9efb0347e488b7a1e12f6ec42fc5299539035", content)

    def test_structure_lists_every_retained_product_area(self):
        content = (ROOT / "docs" / "STRUCTURE.md").read_text(encoding="utf-8")
        retained = (
            "src/",
            "app/compiler/",
            "examples/real-json/",
            "examples/contest/",
            "scripts/",
            "tests/",
            "third_party/",
            "docs/contest/",
            "LibAFL/",
        )
        for path in retained:
            with self.subTest(path=path):
                self.assertIn(path, content)


if __name__ == "__main__":
    unittest.main()
