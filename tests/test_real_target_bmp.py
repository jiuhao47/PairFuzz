import ctypes
import hashlib
import json
import os
import re
import shutil
import struct
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BMP = ROOT / "examples/real-targets/bmp"


def windows_bmp(width=1, height=1, bpp=24, compression=0, pixels=None):
    row_bytes = ((width * bpp + 31) // 32) * 4
    if pixels is None:
        pixels = bytes(row_bytes * abs(height))
    offset = 14 + 40
    size = offset + len(pixels)
    header = b"BM" + struct.pack("<IHHI", size, 0, 0, offset)
    dib = struct.pack(
        "<IiiHHIIiiII",
        40,
        width,
        height,
        1,
        bpp,
        compression,
        len(pixels),
        0,
        0,
        0,
        0,
    )
    return header + dib + pixels


def os2_bmp(width=1, height=1, bpp=24):
    row_bytes = ((width * bpp + 31) // 32) * 4
    pixels = bytes(row_bytes * height)
    offset = 14 + 12
    size = offset + len(pixels)
    return b"BM" + struct.pack("<IHHI", size, 0, 0, offset) + struct.pack(
        "<IHHHH", 12, width, height, 1, bpp
    ) + pixels


class BmpHeader(ctypes.Structure):
    _fields_ = [
        ("width", ctypes.c_uint32),
        ("height", ctypes.c_uint32),
        ("pixel_offset", ctypes.c_uint32),
        ("declared_file_size", ctypes.c_uint32),
        ("dib_size", ctypes.c_uint32),
        ("compression", ctypes.c_uint32),
        ("bits_per_pixel", ctypes.c_uint16),
        ("top_down", ctypes.c_uint8),
        ("image_count", ctypes.c_uint8),
    ]


class BmpScopeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="pairfuzz-bmp-scope-")
        library = Path(cls.temporary.name) / "libbmp_scope.so"
        subprocess.run(
            [
                "cc",
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-shared",
                "-fPIC",
                str(BMP / "bmp_scope.c"),
                "-o",
                str(library),
            ],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        cls.library = ctypes.CDLL(str(library))
        cls.classify = cls.library.bmp_scope_classify
        cls.classify.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.POINTER(BmpHeader)]
        cls.classify.restype = ctypes.c_int

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def result(self, data):
        buffer = ctypes.create_string_buffer(data)
        header = BmpHeader()
        return self.classify(buffer, len(data), ctypes.byref(header)), header

    def test_windows_os2_and_signed_height_are_in_scope(self):
        for data, width, height, top_down in (
            (windows_bmp(3, 2), 3, 2, 0),
            (windows_bmp(3, -2), 3, 2, 1),
            (os2_bmp(3, 2), 3, 2, 0),
        ):
            with self.subTest(width=width, height=height, top_down=top_down):
                result, header = self.result(data)
                self.assertEqual(result, 0)
                self.assertEqual((header.width, header.height), (width, height))
                self.assertEqual(header.top_down, top_down)
                self.assertEqual(header.image_count, 1)

    def test_signature_truncation_planes_and_dib_are_rejected(self):
        malformed = [
            b"",
            b"BM" + bytes(11),
            b"ZZ" + windows_bmp()[2:],
            windows_bmp()[:16],
            bytearray(windows_bmp()),
        ]
        malformed[-1][26:28] = struct.pack("<H", 2)
        for data in malformed:
            with self.subTest(length=len(data)):
                self.assertEqual(self.result(bytes(data))[0], 2)

    def test_embedded_compression_arrays_and_concatenation_are_unsupported(self):
        for compression in (4, 5):
            data = bytearray(windows_bmp())
            data[30:34] = struct.pack("<I", compression)
            self.assertEqual(self.result(bytes(data))[0], 1)
        array = b"BA" + windows_bmp()[2:]
        self.assertEqual(self.result(array)[0], 1)
        first = windows_bmp()
        result, header = self.result(first + windows_bmp())
        self.assertEqual(result, 1)
        self.assertEqual(header.image_count, 2)

    def test_dimension_and_pixel_limits_precede_pixel_allocation(self):
        for width, height in ((4097, 1), (1, 4097), (2049, 2048), (2**31 - 1, 1)):
            data = windows_bmp(width, height, pixels=b"")
            self.assertEqual(self.result(data)[0], 3)
        boundary = windows_bmp(4096, 1, bpp=32)
        result, header = self.result(boundary)
        self.assertEqual(result, 0)
        self.assertEqual((header.width, header.height), (4096, 1))

    def test_compression_and_bit_depth_combinations_are_checked(self):
        for bpp, compression, expected in (
            (8, 1, 0),
            (4, 2, 0),
            (24, 1, 2),
            (8, 2, 2),
            (16, 3, 0),
            (24, 3, 2),
        ):
            with self.subTest(bpp=bpp, compression=compression):
                data = windows_bmp(1, 1, bpp=bpp, compression=compression, pixels=b"\x00" * 16)
                self.assertEqual(self.result(data)[0], expected)


class BmpDecoderTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.contract = json.loads((BMP / "contract.json").read_text())
        cls.fixtures = json.loads((BMP / "fixtures.json").read_text())["fixtures"]
        cls.profile = os.environ.get("PAIRFUZZ_BMP_TEST_PROFILE", "release")
        cls.temporary = tempfile.TemporaryDirectory(prefix="pairfuzz-bmp-contract-")
        cls.manifest = None

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    @classmethod
    def ensure_manifest(cls):
        if cls.manifest is None:
            completed = subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts" / "real_targets.py"),
                    "build",
                    "--pair",
                    "bmp",
                    "--profile",
                    cls.profile,
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=True,
                env={
                    **os.environ,
                    "MAGICK_CONFIGURE_PATH": "",
                    "MAGICK_CODER_MODULE_PATH": "",
                    "MAGICK_CODER_FILTER_PATH": "",
                },
            )
            cls.manifest = json.loads(Path(completed.stdout.strip()).read_text())
        return cls.manifest

    def replay(self, target, fixture, sequence=0):
        manifest = self.ensure_manifest()
        output = Path(self.temporary.name) / f"{target}-{fixture['name']}-{sequence}.canonical"
        binary = ROOT / manifest["targets"][target]["path"]
        completed = subprocess.run(
            [
                str(binary),
                "--replay",
                str(BMP / fixture["path"]),
                "--normalized-output",
                str(output),
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
            env={
                **os.environ,
                "MAGICK_CONFIGURE_PATH": "",
                "MAGICK_CODER_MODULE_PATH": "",
                "MAGICK_CODER_FILTER_PATH": "",
            },
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        return json.loads(completed.stdout), output

    def sequence_driver(self, target):
        manifest = self.ensure_manifest()
        build_root = (ROOT / manifest["targets"][target]["path"]).parents[1]
        name, package, adapter = {
            "a": ("imagemagick", "MagickCore", "imagemagick_target.c"),
            "b": ("graphicsmagick", "GraphicsMagick", "graphicsmagick_target.c"),
        }[target]
        pkgconfig = build_root / "install" / name / "lib" / "pkgconfig"
        environment = {
            **os.environ,
            "PATH": str(ROOT / "run/toolchain/bin") + os.pathsep + os.environ["PATH"],
            "PKG_CONFIG_PATH": str(pkgconfig),
            "PAIRFUZZ_TARGET_PROFILE": self.profile,
        }
        cflags = subprocess.run(
            ["pkg-config", "--cflags", package],
            env=environment,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.split()
        libraries = subprocess.run(
            ["pkg-config", "--libs", package],
            env=environment,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.split()
        binary = Path(self.temporary.name) / f"sequence-{target}"
        profile_flags = (
            ["-O1", "-g", "-fno-omit-frame-pointer", "-fsanitize=address"]
            if self.profile == "asan"
            else ["-O3", "-g"]
        )
        subprocess.run(
            [
                str(ROOT / "app/compiler/target/debug/libafl_forkserver_cc"),
                *profile_flags,
                *cflags,
                "-I",
                str(ROOT / "examples/real-targets/common"),
                "-I",
                str(BMP),
                f'-DPAIRFUZZ_MAGICK_CONFIG_PATH="{build_root / "policy" / name}"',
                str(ROOT / "examples/real-targets/common/semantic.c"),
                str(BMP / "bmp_scope.c"),
                str(BMP / adapter),
                str(ROOT / "tests/fixtures/bmp_sequence_driver.c"),
                *libraries,
                *(["-fsanitize=address"] if self.profile == "asan" else []),
                "-o",
                str(binary),
            ],
            cwd=ROOT,
            env=environment,
            capture_output=True,
            text=True,
            check=True,
        )
        return binary

    def test_fixture_matrix_covers_required_bmp_behaviors(self):
        required = {
            "indexed1",
            "indexed4",
            "indexed8",
            "rgb16-bitfields",
            "rgb24-bottom-up-odd",
            "rgb24-top-down-odd",
            "rgb32-unused-alpha",
            "rgba32-bitfields",
            "rle4",
            "rle8",
            "truncated-pixels",
            "truncated-palette",
            "bad-offset",
            "second-image",
            "embedded-png",
            "width-4097",
            "pixel-over-limit",
        }
        self.assertTrue(required.issubset({fixture["name"] for fixture in self.fixtures}))

    def test_both_decoders_match_independent_valid_fixture_expectations(self):
        matching = [fixture for fixture in self.fixtures if fixture.get("matching")]
        self.assertGreaterEqual(len(matching), 10)
        for fixture in matching:
            with self.subTest(fixture=fixture["name"]):
                reports = {}
                outputs = {}
                for target in ("a", "b"):
                    reports[target], output = self.replay(target, fixture)
                    self.assertEqual(reports[target]["class"], "accepted")
                    outputs[target] = output.read_bytes()
                    self.assertEqual(
                        (reports[target]["width"], reports[target]["height"]),
                        tuple(fixture["dimensions"]),
                    )
                    self.assertEqual(
                        hashlib.sha256(outputs[target]).hexdigest(),
                        fixture["canonical_sha256"],
                    )
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

    def test_valid_invalid_valid_sequence_never_reuses_pixels(self):
        by_name = {fixture["name"]: fixture for fixture in self.fixtures}
        self.assertIn("rgb24-bottom-up-odd", by_name)
        self.assertIn("truncated-pixels", by_name)
        valid = by_name["rgb24-bottom-up-odd"]
        invalid = by_name["truncated-pixels"]
        for target in ("a", "b"):
            completed = subprocess.run(
                [
                    str(self.sequence_driver(target)),
                    str(BMP / valid["path"]),
                    str(BMP / invalid["path"]),
                    str(BMP / valid["path"]),
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=False,
                env={**os.environ, "ASAN_OPTIONS": "abort_on_error=1:detect_leaks=false"},
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            outcomes = [line.split() for line in completed.stdout.splitlines()]
            self.assertEqual([outcome[0] for outcome in outcomes], ["0", "1", "0"])
            self.assertEqual(outcomes[0][1], outcomes[2][1])

    def test_targets_do_not_probe_ambient_magick_configuration(self):
        if shutil.which("strace") is None:
            self.skipTest("strace is required for the native configuration audit")
        manifest = self.ensure_manifest()
        build_root = Path(manifest["targets"]["a"]["path"]).parents[1].resolve()
        fixture = next(item for item in self.fixtures if item["name"] == "rgb24-bottom-up-odd")
        configuration_names = {
            "coder.xml",
            "delegates.xml",
            "delegates.mgk",
            "log.mgk",
            "log.xml",
            "magic.xml",
            "modules.mgk",
            "policy.xml",
            "type.xml",
        }
        for target in ("a", "b"):
            trace = Path(self.temporary.name) / f"{target}-openat.log"
            output = Path(self.temporary.name) / f"{target}-trace.canonical"
            binary = ROOT / manifest["targets"][target]["path"]
            completed = subprocess.run(
                [
                    "strace",
                    "-f",
                    "-qq",
                    "-e",
                    "trace=openat",
                    "-o",
                    str(trace),
                    str(binary),
                    "--replay",
                    str(BMP / fixture["path"]),
                    "--normalized-output",
                    str(output),
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=False,
                env={
                    **os.environ,
                    "MAGICK_CONFIGURE_PATH": "",
                    "MAGICK_CODER_MODULE_PATH": "",
                    "MAGICK_CODER_FILTER_PATH": "",
                },
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            opened = [Path(path) for path in re.findall(r'openat\([^,]+, "([^"]+)"', trace.read_text())]
            magick_configuration = [
                path
                for path in opened
                if path.name in configuration_names
                or ".config/ImageMagick" in path.as_posix()
            ]
            self.assertTrue(magick_configuration)
            for path in magick_configuration:
                self.assertTrue(
                    path.is_relative_to(build_root),
                    f"{target} probed ambient Magick configuration: {path}",
                )


if __name__ == "__main__":
    unittest.main()
