import mmap
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests" / "fixtures" / "semantic"
COMPILER_TARGET = ROOT / "app" / "compiler" / "target" / "debug"


class SemanticRuntimeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.environment = os.environ.copy()
        cls.environment["PATH"] = (
            str(ROOT / "run" / "toolchain" / "bin")
            + os.pathsep
            + cls.environment["PATH"]
        )
        cls.environment["ASAN_OPTIONS"] = "abort_on_error=1:detect_leaks=false"
        cls.build_dir = Path(tempfile.mkdtemp(prefix="diffray-semantic-runtime-"))
        subprocess.run(
            [
                "cargo",
                "build",
                "--locked",
                "--manifest-path",
                str(ROOT / "app" / "compiler" / "Cargo.toml"),
            ],
            cwd=ROOT,
            env=cls.environment,
            check=True,
        )
        cls.binaries = {}
        for name, wrapper in (
            ("semantic-c", "libafl_forkserver_cc"),
            ("semantic-cxx", "libafl_forkserver_cxx"),
            ("legacy", "libafl_forkserver_cc"),
        ):
            source = FIXTURES / (
                "semantic.cpp" if name == "semantic-cxx" else f"{name.split('-')[0]}.c"
            )
            output = cls.build_dir / name
            subprocess.run(
                [str(COMPILER_TARGET / wrapper), str(source), "-o", str(output)],
                cwd=ROOT,
                env=cls.environment,
                check=True,
            )
            cls.binaries[name] = output

    @staticmethod
    def _region():
        fd = os.memfd_create("diffray-semantic-result", flags=0)
        os.ftruncate(fd, 8)
        return fd, mmap.mmap(fd, 8)

    def _run(self, binary, role=None, regions=None, args=(), extra_env=None):
        env = self.environment.copy()
        for key in list(env):
            if key.startswith("__DIFFRAY_"):
                del env[key]
        pass_fds = []
        if role is not None:
            env["__DIFFRAY_RV_ROLE"] = role
        if regions:
            for region_role, (fd, _mapping) in regions.items():
                env[f"__DIFFRAY_RV_SHM_ID_{region_role}"] = str(fd)
                env[f"__DIFFRAY_RV_SHM_ID_{region_role}_SIZE"] = "8"
                pass_fds.append(fd)
        if extra_env:
            env.update(extra_env)
        return subprocess.run(
            [str(binary), *map(str, args)],
            cwd=ROOT,
            env=env,
            pass_fds=tuple(pass_fds),
            capture_output=True,
            check=False,
        )

    def test_semantic_transport_preserves_full_u64_for_c_and_cpp(self):
        wanted = 0x3123456789ABCDEF
        for binary_name in ("semantic-c", "semantic-cxx"):
            with self.subTest(binary=binary_name):
                fd, mapping = self._region()
                try:
                    completed = self._run(
                        self.binaries[binary_name], "A", {"A": (fd, mapping)}, [hex(wanted)]
                    )
                    self.assertEqual(completed.returncode, 0, completed.stderr)
                    self.assertEqual(int.from_bytes(mapping[:], byteorder="little"), wanted)
                finally:
                    mapping.close()
                    os.close(fd)

    def test_semantic_transport_legacy_negative_value_is_zero_extended(self):
        fd, mapping = self._region()
        try:
            completed = self._run(self.binaries["legacy"], "A", {"A": (fd, mapping)})
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(int.from_bytes(mapping[:], byteorder="little"), 0xFFFFFFFF)
        finally:
            mapping.close()
            os.close(fd)


    def test_semantic_transport_uses_explicit_role_when_both_regions_exist(self):
        a = self._region()
        b = self._region()
        try:
            completed = self._run(
                self.binaries["semantic-c"], "B", {"A": a, "B": b}, ["0x55aa"]
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(a[1][:], bytes(8))
            self.assertEqual(int.from_bytes(b[1][:], byteorder="little"), 0x55AA)
        finally:
            for fd, mapping in (a, b):
                mapping.close()
                os.close(fd)

    def test_semantic_transport_allows_standalone_and_rejects_partial_mapping(self):
        standalone = self._run(self.binaries["semantic-c"])
        self.assertEqual(standalone.returncode, 0, standalone.stderr)

        partial = self._run(self.binaries["semantic-c"], "A")
        self.assertNotEqual(partial.returncode, 0)

        invalid_role = self._run(
            self.binaries["semantic-c"], extra_env={"__DIFFRAY_RV_ROLE": "C"}
        )
        self.assertNotEqual(invalid_role.returncode, 0)

        fd, mapping = self._region()
        try:
            short = self._run(
                self.binaries["semantic-c"],
                "A",
                {"A": (fd, mapping)},
                extra_env={"__DIFFRAY_RV_SHM_ID_A_SIZE": "7"},
            )
            self.assertNotEqual(short.returncode, 0)
        finally:
            mapping.close()
            os.close(fd)

        b_fd, b_mapping = self._region()
        try:
            opposite = self._run(
                self.binaries["semantic-c"], "A", {"B": (b_fd, b_mapping)}
            )
            self.assertNotEqual(opposite.returncode, 0)
            self.assertEqual(b_mapping[:], bytes(8))
        finally:
            b_mapping.close()
            os.close(b_fd)

    def test_semantic_transport_publication_has_no_per_call_mapping_syscalls(self):
        if not Path("/usr/bin/strace").is_file():
            self.skipTest("strace is unavailable")
        fd, mapping = self._region()
        trace = self.build_dir / "publication.strace"
        env = self.environment.copy()
        env.update(
            {
                "__DIFFRAY_RV_ROLE": "A",
                "__DIFFRAY_RV_SHM_ID_A": str(fd),
                "__DIFFRAY_RV_SHM_ID_A_SIZE": "8",
            }
        )
        try:
            completed = subprocess.run(
                [
                    "/usr/bin/strace",
                    "-qq",
                    "-e",
                    "trace=mmap,munmap,msync,close,write",
                    "-o",
                    str(trace),
                    str(self.binaries["semantic-c"]),
                    "0x1234",
                    "50",
                ],
                cwd=ROOT,
                env=env,
                pass_fds=(fd,),
                capture_output=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            calls = trace.read_text(encoding="utf-8")
            begin = calls.index("DIFFRAY_PUBLISH_BEGIN")
            end = calls.index("DIFFRAY_PUBLISH_END")
            publication = calls[begin:end]
            for syscall in ("mmap(", "munmap(", "msync(", "close("):
                self.assertNotIn(syscall, publication)
            self.assertEqual(int.from_bytes(mapping[:], byteorder="little"), 0x1234)
        finally:
            mapping.close()
            os.close(fd)


class CompilerProfileTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.environment = os.environ.copy()
        cls.environment["PATH"] = str(ROOT / "run" / "toolchain" / "bin") + os.pathsep + cls.environment["PATH"]
        cls.build_dir = Path(tempfile.mkdtemp(prefix="diffray-compiler-profile-"))
        subprocess.run(
            ["cargo", "build", "--locked", "--manifest-path", str(ROOT / "app" / "compiler" / "Cargo.toml")],
            cwd=ROOT,
            env=cls.environment,
            check=True,
        )

    def _compile(self, profile, name):
        env = self.environment.copy()
        env.pop("DIFFRAY_TARGET_PROFILE", None)
        if profile is not None:
            env["DIFFRAY_TARGET_PROFILE"] = profile
        output = self.build_dir / name
        completed = subprocess.run(
            [str(COMPILER_TARGET / "libafl_forkserver_cc"), str(FIXTURES / "semantic.c"), "-o", str(output)],
            cwd=ROOT,
            env=env,
            capture_output=True,
            check=False,
        )
        return completed, output

    @staticmethod
    def _symbols(binary):
        return subprocess.run(["nm", "-a", str(binary)], capture_output=True, text=True, check=True).stdout

    def test_profiles_control_asan_but_keep_coverage(self):
        for profile, expected_asan in ((None, True), ("asan", True), ("release", False)):
            with self.subTest(profile=profile):
                completed, binary = self._compile(profile, f"target-{profile or 'default'}")
                self.assertEqual(completed.returncode, 0, completed.stderr)
                symbols = self._symbols(binary)
                self.assertEqual("__asan_init" in symbols, expected_asan)
                self.assertIn("__sanitizer_cov_trace_pc_guard", symbols)
                standalone = subprocess.run([str(binary)], capture_output=True, check=False)
                self.assertEqual(standalone.returncode, 0, standalone.stderr)

    def test_invalid_profile_and_probe_modes_are_clean(self):
        invalid, _ = self._compile("unknown", "invalid")
        self.assertNotEqual(invalid.returncode, 0)
        self.assertIn(b"DIFFRAY_TARGET_PROFILE", invalid.stderr)
        cases = (["--version"], ["-c", str(FIXTURES / "semantic.c"), "-o", str(self.build_dir / "probe.o")])
        for arguments in cases:
            completed = subprocess.run(
                [str(COMPILER_TARGET / "libafl_forkserver_cc"), *arguments],
                cwd=ROOT,
                env={**self.environment, "DIFFRAY_TARGET_PROFILE": "release"},
                capture_output=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)


if __name__ == "__main__":
    unittest.main()
