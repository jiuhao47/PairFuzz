import hashlib
import io
import json
import os
import tarfile
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from scripts.real_targets_lib.builds import (
    BuildManifestTransaction,
    build_fingerprint,
    clean_build_environment,
    profile_flags,
)
from scripts.real_targets_lib.registry import pair_spec
from scripts.real_targets_lib.sources import SourceError, fetch_source, load_lock


ROOT = Path(__file__).resolve().parents[1]


def tar_bytes(members):
    payload = io.BytesIO()
    with tarfile.open(fileobj=payload, mode="w:gz") as archive:
        for member in members:
            info = tarfile.TarInfo(member[0])
            if member[1] == "file":
                data = member[2]
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
            else:
                info.type = tarfile.SYMTYPE
                info.linkname = member[2]
                archive.addfile(info)
    return payload.getvalue()


def entry_for(url, payload, name="fixture"):
    return {
        "name": name,
        "version": "1.0",
        "tag": "v1.0",
        "resolved_commit": "a" * 40,
        "url": url,
        "sha256": hashlib.sha256(payload).hexdigest(),
        "archive": "tar.gz",
        "license": "MIT",
        "verification_reference": "https://example.invalid/releases/v1.0",
    }


class SourceLockTests(unittest.TestCase):
    def setUp(self):
        self.tempdir = tempfile.TemporaryDirectory(prefix="diffray-source-tests-")
        self.root = Path(self.tempdir.name)
        self.cache = self.root / "cache"

    def tearDown(self):
        self.tempdir.cleanup()

    def _archive(self, members):
        payload = tar_bytes(members)
        path = self.root / "source.tar.gz"
        path.write_bytes(payload)
        return payload, path

    def test_load_lock_rejects_missing_provenance(self):
        path = self.root / "lock.json"
        path.write_text(json.dumps({"schema_version": 1, "sources": [{"name": "bad"}]}))
        with self.assertRaisesRegex(SourceError, "missing"):
            load_lock(path)

    def test_repository_lock_and_vendored_xxhash_match_approved_sources(self):
        lock = load_lock(ROOT / "third_party" / "real-targets.lock.json")
        entries = {entry["name"]: entry for entry in lock["sources"]}
        self.assertEqual(
            set(entries),
            {
                "imagemagick",
                "graphicsmagick",
                "libpng",
                "libspng",
                "openssl",
                "wolfssl",
                "zlib",
                "xxhash",
                "xxhash-license",
            },
        )
        for name, relative in (
            ("xxhash", "xxhash.h"),
            ("xxhash-license", "xxhash.LICENSE"),
        ):
            payload = (ROOT / "examples" / "real-targets" / "common" / relative).read_bytes()
            self.assertEqual(hashlib.sha256(payload).hexdigest(), entries[name]["sha256"])

    def test_wrong_hash_is_rejected_before_extraction(self):
        payload, path = self._archive([("package/sentinel", "file", b"trusted")])
        entry = entry_for(path.as_uri(), payload)
        entry["sha256"] = "0" * 64
        with self.assertRaisesRegex(SourceError, "SHA-256"):
            fetch_source(entry, self.cache)
        self.assertFalse(any(self.cache.rglob("sentinel")))

    def test_valid_cache_is_stable_and_reused_without_source(self):
        payload, path = self._archive([("package/value.txt", "file", b"one")])
        entry = entry_for(path.as_uri(), payload)
        first = fetch_source(entry, self.cache)
        self.assertEqual((first / "package" / "value.txt").read_bytes(), b"one")
        path.unlink()
        second = fetch_source(entry, self.cache)
        self.assertEqual(first, second)

    def test_archive_paths_and_links_cannot_escape(self):
        cases = {
            "parent": [("../escaped", "file", b"bad")],
            "absolute": [("/tmp/diffray-escaped", "file", b"bad")],
            "link": [("package/link", "link", "../../escaped")],
        }
        for label, members in cases.items():
            with self.subTest(label=label):
                payload, path = self._archive(members)
                with self.assertRaisesRegex(SourceError, "unsafe"):
                    fetch_source(entry_for(path.as_uri(), payload, label), self.cache)

    def test_interrupted_download_retries_with_fresh_temporary_file(self):
        payload = tar_bytes([("package/value", "file", b"complete")])

        class Handler(BaseHTTPRequestHandler):
            attempts = 0

            def do_GET(self):
                type(self).attempts += 1
                self.send_response(200)
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                if type(self).attempts == 1:
                    self.wfile.write(payload[: len(payload) // 2])
                    self.close_connection = True
                else:
                    self.wfile.write(payload)

            def log_message(self, *_args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        url = f"http://127.0.0.1:{server.server_port}/source.tar.gz"
        try:
            with self.assertRaises(Exception):
                fetch_source(entry_for(url, payload), self.cache)
            result = fetch_source(entry_for(url, payload), self.cache)
            self.assertEqual((result / "package" / "value").read_bytes(), b"complete")
            self.assertFalse(any(path.name.startswith(".fetch-") for path in self.cache.iterdir()))
        finally:
            server.shutdown()
            thread.join()
            server.server_close()


class BuildProvenanceTests(unittest.TestCase):
    def test_pair_registry_is_immutable_and_complete(self):
        png = pair_spec("png")
        self.assertEqual(png.operation, "image.png.decode.rgba8")
        self.assertEqual((png.library_a, png.library_b), ("libpng", "libspng"))
        self.assertTrue(str(png.contract_path).endswith("contract.json"))
        with self.assertRaisesRegex(ValueError, "unknown pair"):
            pair_spec("not-a-pair")

    def test_fingerprint_changes_for_every_build_identity_dimension(self):
        base = {
            "profile": "asan",
            "compiler": {"path": "/cc", "version": "clang 20"},
            "flags": ["-O1", "-g"],
            "dependencies": {"zlib": "aaa", "libpng": "bbb"},
            "sources": {"target.c": "ccc"},
            "wrapper_runtime_sha256": "ddd",
            "configure": ["--disable-shared"],
        }
        original = build_fingerprint(base)
        mutations = [
            {**base, "profile": "release"},
            {**base, "compiler": {"path": "/cc", "version": "clang 21"}},
            {**base, "flags": ["-O3", "-g"]},
            {**base, "dependencies": {"zlib": "changed", "libpng": "bbb"}},
            {**base, "sources": {"target.c": "changed"}},
            {**base, "wrapper_runtime_sha256": "changed"},
            {**base, "configure": ["--enable-shared"]},
        ]
        for mutation in mutations:
            self.assertNotEqual(original, build_fingerprint(mutation))

    def test_profile_flags_reach_compile_and_link_and_environment_is_clean(self):
        asan = profile_flags("asan")
        release = profile_flags("release")
        self.assertIn("-O1", asan.compile)
        self.assertIn("-fno-omit-frame-pointer", asan.compile)
        self.assertIn("-fsanitize=address", asan.link)
        self.assertIn("-O3", release.compile)
        self.assertNotIn("-fsanitize=address", release.link)
        self.assertNotIn("-march=native", release.compile)
        env = clean_build_environment(
            {"PATH": "/bin", "HOME": "/home/test", "SECRET_TOKEN": "nope"},
            profile="release",
            cc="/wrapper/cc",
            cxx="/wrapper/cxx",
            ar="/llvm/ar",
            ranlib="/llvm/ranlib",
        )
        self.assertEqual(env["CC"], "/wrapper/cc")
        self.assertEqual(env["DIFFRAY_TARGET_PROFILE"], "release")
        self.assertNotIn("SECRET_TOKEN", env)
        self.assertNotIn("__AFL_SHM_ID", env)
        self.assertNotIn("__DIFFRAY_RV_A", env)

    def test_complete_manifest_is_atomic_and_failure_never_claims_complete(self):
        with tempfile.TemporaryDirectory(prefix="diffray-manifest-") as raw:
            root = Path(raw)
            try:
                with BuildManifestTransaction(root, {"schema_version": 1}) as transaction:
                    transaction.record_command(["cc", "-O3", "a.c"], {"CC": "cc"})
                    raise RuntimeError("link failed")
            except RuntimeError:
                pass
            self.assertFalse((root / "build.json").exists())
            with BuildManifestTransaction(root, {"schema_version": 1}) as transaction:
                transaction.record_command(["cc", "-O3", "a.c"], {"CC": "cc"})
                manifest = transaction.complete({"targets": ["a", "b"]})
            data = json.loads(manifest.read_text())
            self.assertEqual(data["status"], "complete")
            self.assertIn("-O3", data["commands"][0]["argv"])


if __name__ == "__main__":
    unittest.main()
