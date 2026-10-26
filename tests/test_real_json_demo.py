import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


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


if __name__ == "__main__":
    unittest.main()
