import ctypes
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
        cls.temporary = tempfile.TemporaryDirectory(prefix="diffray-bmp-scope-")
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


if __name__ == "__main__":
    unittest.main()
