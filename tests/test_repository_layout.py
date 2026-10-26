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


if __name__ == "__main__":
    unittest.main()
