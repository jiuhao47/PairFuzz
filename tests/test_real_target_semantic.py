import json
import os
import shutil
import struct
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
COMMON = ROOT / "examples" / "real-targets" / "common"
FIXTURES = ROOT / "tests" / "fixtures"
LIMIT = 1024 * 1024
MASK64 = (1 << 64) - 1
COMPILER_PATH = str(ROOT / "run" / "toolchain" / "bin") + os.pathsep + os.environ["PATH"]
CLANG = shutil.which("clang", path=COMPILER_PATH) or "clang"
CLANGXX = shutil.which("clang++", path=COMPILER_PATH) or "clang++"
COMPILER_TARGET = ROOT / "app" / "compiler" / "target" / "debug"


def _rotl(value, count):
    value &= MASK64
    return ((value << count) | (value >> (64 - count))) & MASK64


def xxh64(data, seed=0):
    prime1, prime2 = 11400714785074694791, 14029467366897019727
    prime3, prime4, prime5 = 1609587929392839161, 9650029242287828579, 2870177450012600261

    def round64(accumulator, lane):
        accumulator = (accumulator + lane * prime2) & MASK64
        return (_rotl(accumulator, 31) * prime1) & MASK64

    offset = 0
    if len(data) >= 32:
        lanes = [
            (seed + prime1 + prime2) & MASK64,
            (seed + prime2) & MASK64,
            seed & MASK64,
            (seed - prime1) & MASK64,
        ]
        while offset <= len(data) - 32:
            for index in range(4):
                lanes[index] = round64(lanes[index], struct.unpack_from("<Q", data, offset)[0])
                offset += 8
        result = sum(_rotl(lanes[index], (1, 7, 12, 18)[index]) for index in range(4)) & MASK64
        for lane in lanes:
            mixed = round64(0, lane)
            result ^= mixed
            result = (result * prime1 + prime4) & MASK64
    else:
        result = (seed + prime5) & MASK64
    result = (result + len(data)) & MASK64
    while offset <= len(data) - 8:
        lane = round64(0, struct.unpack_from("<Q", data, offset)[0])
        result ^= lane
        result = (_rotl(result, 27) * prime1 + prime4) & MASK64
        offset += 8
    if offset <= len(data) - 4:
        result ^= struct.unpack_from("<I", data, offset)[0] * prime1
        result = (_rotl(result, 23) * prime2 + prime3) & MASK64
        offset += 4
    while offset < len(data):
        result ^= data[offset] * prime5
        result = (_rotl(result, 11) * prime1) & MASK64
        offset += 1
    result ^= result >> 33
    result = (result * prime2) & MASK64
    result ^= result >> 29
    result = (result * prime3) & MASK64
    return (result ^ (result >> 32)) & MASK64


def packed(class_id, payload):
    return (class_id << 60) | ((payload & ((1 << 60) - 1)) if class_id == 0 else 0)


class SemanticContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="pairfuzz-native-semantic-")
        cls.build = Path(cls.temporary.name)
        include = ["-I", str(COMMON)]
        subprocess.run(
            [
                str(CLANG),
                "-std=c11",
                *include,
                str(COMMON / "semantic.c"),
                str(FIXTURES / "native" / "semantic_probe.c"),
                str(FIXTURES / "native" / "synthetic_adapter.c"),
                "-o",
                str(cls.build / "probe"),
            ],
            cwd=ROOT,
            check=True,
        )
        subprocess.run(
            [
                str(CLANG),
                "-std=c11",
                *include,
                "-c",
                str(COMMON / "semantic.c"),
                "-o",
                str(cls.build / "semantic.o"),
            ],
            cwd=ROOT,
            check=True,
        )
        subprocess.run(
            [
                str(CLANG),
                "-std=c11",
                *include,
                str(COMMON / "semantic.c"),
                str(COMMON / "harness_io.c"),
                str(FIXTURES / "native" / "synthetic_adapter.c"),
                "-o",
                str(cls.build / "synthetic"),
            ],
            cwd=ROOT,
            check=True,
        )

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def probe(self, *arguments, data=b""):
        result = subprocess.run(
            [str(self.build / "probe"), *arguments], input=data, capture_output=True, text=False, check=True
        )
        return result.stdout.decode().strip()

    def test_xxh64_fixed_vectors_include_multiblock_input(self):
        vectors = json.loads((FIXTURES / "semantic" / "hash_vectors.json").read_text())["xxh64_seed_0"]
        for vector in vectors:
            data = bytes(range(256)) if "generator" in vector else bytes.fromhex(vector["hex"])
            with self.subTest(vector=vector["name"]):
                self.assertEqual(f"{xxh64(data):016x}", vector["hash"])
                self.assertEqual(self.probe("hash", data=data), vector["hash"])

    def test_pack_classes_preserve_high_bits_and_zero_failure_payloads(self):
        vectors = json.loads((FIXTURES / "semantic" / "hash_vectors.json").read_text())["packed_results"]
        for vector in vectors:
            expected = vector["code"]
            self.assertEqual(f"{packed(vector['class'], int(vector['payload'], 16)):016x}", expected)
            self.assertEqual(self.probe("pack", str(vector["class"]), "0x" + vector["payload"]), expected)

    def test_image_and_der_encodings_are_explicit_little_endian(self):
        pixels = bytes([1, 2, 3, 255, 9, 8, 7, 6])
        image_bytes = b"\x01" + struct.pack("<IIQ", 2, 1, len(pixels)) + pixels
        expected_image = packed(0, xxh64(image_bytes))
        self.assertEqual(self.probe("image", "2", "1", data=pixels), f"{expected_image:016x}")
        der = bytes.fromhex("3003020101")
        der_bytes = b"\x02" + struct.pack("<Q", len(der)) + der
        expected_der = packed(0, xxh64(der_bytes))
        self.assertEqual(self.probe("der", data=der), f"{expected_der:016x}")

    def test_plain_mode_is_quiet_and_does_not_reuse_accepted_fields(self):
        for data in (b"A*", b"R"):
            completed = subprocess.run([str(self.build / "synthetic")], input=data, capture_output=True, check=False)
            self.assertEqual(completed.returncode, 0)
            self.assertEqual(completed.stdout, b"")
            self.assertEqual(completed.stderr, b"")
        first, second = self.probe("stale").split()
        self.assertNotEqual(first, second)
        self.assertEqual(second, "1000000000000000")

    def test_c_contract_links_from_cpp(self):
        binary = self.build / "cpp-probe"
        subprocess.run(
            [
                str(CLANGXX),
                "-std=c++17",
                "-I",
                str(COMMON),
                str(FIXTURES / "native" / "cpp_contract_probe.cpp"),
                str(self.build / "semantic.o"),
                "-o",
                str(binary),
            ],
            cwd=ROOT,
            check=True,
        )
        subprocess.run([str(binary)], check=True)

    def test_instrumented_harness_compiles_in_both_profiles(self):
        environment = os.environ.copy()
        environment["PATH"] = str(ROOT / "run" / "toolchain" / "bin") + os.pathsep + environment["PATH"]
        subprocess.run(
            ["cargo", "build", "--locked", "--manifest-path", str(ROOT / "app" / "compiler" / "Cargo.toml")],
            cwd=ROOT,
            env=environment,
            check=True,
            capture_output=True,
        )
        for profile in ("asan", "release"):
            with self.subTest(profile=profile):
                profile_environment = {**environment, "PAIRFUZZ_TARGET_PROFILE": profile}
                subprocess.run(
                    [
                        str(COMPILER_TARGET / "libafl_forkserver_cc"),
                        "-std=c11",
                        "-I",
                        str(COMMON),
                        str(COMMON / "semantic.c"),
                        str(COMMON / "harness_io.c"),
                        str(FIXTURES / "native" / "synthetic_adapter.c"),
                        "-o",
                        str(self.build / f"instrumented-{profile}"),
                    ],
                    cwd=ROOT,
                    env=profile_environment,
                    check=True,
                    capture_output=True,
                )

    def test_replay_writes_canonical_bytes_and_deterministic_json(self):
        input_path = self.build / "accepted.bin"
        output_path = self.build / "normalized.bin"
        input_path.write_bytes(b"A*")
        command = [
            str(self.build / "synthetic"),
            "--replay",
            str(input_path),
            "--normalized-output",
            str(output_path),
        ]
        environment = {
            name: value
            for name, value in os.environ.items()
            if not name.startswith("__AFL_") and not name.startswith("__PAIRFUZZ_")
        }
        first = subprocess.run(command, env=environment, capture_output=True, text=True, check=True)
        second = subprocess.run(command, env=environment, capture_output=True, text=True, check=True)
        self.assertEqual(first.stdout, second.stdout)
        report = json.loads(first.stdout)
        canonical = b"\x01" + struct.pack("<IIQ", 1, 1, 4) + bytes([42, 2, 3, 255])
        self.assertEqual(output_path.read_bytes(), canonical)
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(report["operation"], "synthetic.bytes.normalize")
        self.assertEqual(report["class"], "accepted")
        self.assertEqual(report["semantic_code"], f"{packed(0, xxh64(canonical)):016x}")

        rejected_input = self.build / "rejected.bin"
        rejected_output = self.build / "rejected.out"
        rejected_input.write_bytes(b"R")
        rejected = subprocess.run(
            [
                str(self.build / "synthetic"),
                "--replay",
                str(rejected_input),
                "--normalized-output",
                str(rejected_output),
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        rejected_report = json.loads(rejected.stdout)
        self.assertEqual((rejected_report["width"], rejected_report["height"]), (0, 0))
        self.assertEqual(rejected_report["normalized_length"], 0)
        self.assertFalse(rejected_output.exists())

        error_input = self.build / "error.bin"
        error_input.write_bytes(b"E")
        adapter_error = subprocess.run(
            [
                str(self.build / "synthetic"),
                "--replay",
                str(error_input),
                "--normalized-output",
                str(self.build / "error.out"),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(adapter_error.returncode, 0)
        self.assertEqual(json.loads(adapter_error.stdout)["class"], "adapter-error")

    def test_input_limit_and_argument_errors_are_explicit(self):
        exact = self.build / "exact.bin"
        over = self.build / "over.bin"
        exact.write_bytes(b"A" + b"x" * (LIMIT - 1))
        over.write_bytes(b"A" + b"x" * LIMIT)
        for path, expected in ((exact, "accepted"), (over, "resource-limit")):
            output = self.build / f"{path.stem}.out"
            result = subprocess.run(
                [str(self.build / "synthetic"), "--replay", str(path), "--normalized-output", str(output)],
                capture_output=True,
                text=True,
                check=True,
            )
            self.assertEqual(json.loads(result.stdout)["class"], expected)
        for arguments in ([], ["--unknown"], ["--replay", str(exact)], ["--replay", str(exact), "--normalized-output"]):
            result = subprocess.run([str(self.build / "synthetic"), *arguments], capture_output=True)
            if not arguments:
                continue
            self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
