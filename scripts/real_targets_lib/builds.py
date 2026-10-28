import hashlib
import json
import os
import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

from .registry import pair_spec
from .sources import fetch_source, load_lock, source_entries


@dataclass(frozen=True)
class ProfileFlags:
    compile: tuple[str, ...]
    link: tuple[str, ...]


def profile_flags(profile: str) -> ProfileFlags:
    common = ("-g",)
    if profile == "asan":
        return ProfileFlags(("-O1", *common, "-fno-omit-frame-pointer"), ("-fsanitize=address",))
    if profile == "release":
        return ProfileFlags(("-O3", *common), ())
    raise ValueError(f"unknown target profile {profile!r}; expected asan or release")


def build_fingerprint(inputs: dict) -> str:
    encoded = json.dumps(inputs, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()
    return hashlib.sha256(encoded).hexdigest()


def clean_build_environment(base: dict, *, profile: str, cc: str, cxx: str, ar: str, ranlib: str) -> dict:
    profile_flags(profile)
    environment = {}
    for name in ("PATH", "HOME", "TMPDIR", "LANG", "LC_ALL", "SOURCE_DATE_EPOCH"):
        if name in base:
            environment[name] = base[name]
    environment.update(
        {
            "CC": cc,
            "CXX": cxx,
            "AR": ar,
            "RANLIB": ranlib,
            "DIFFRAY_TARGET_PROFILE": profile,
        }
    )
    return environment


class BuildManifestTransaction:
    def __init__(self, root: Path, identity: dict):
        self.root = root
        self.identity = dict(identity)
        self.commands = []
        self.completed = False

    def __enter__(self):
        self.root.mkdir(parents=True, exist_ok=True)
        return self

    def record_command(self, argv, environment):
        self.commands.append({"argv": [str(value) for value in argv], "env": dict(sorted(environment.items()))})

    def complete(self, fields: dict) -> Path:
        manifest = {
            **self.identity,
            **fields,
            "status": "complete",
            "commands": self.commands,
        }
        descriptor, temporary_name = tempfile.mkstemp(prefix=".build-", suffix=".json", dir=self.root)
        temporary = Path(temporary_name)
        try:
            with os.fdopen(descriptor, "w", encoding="utf-8") as output:
                json.dump(manifest, output, indent=2, sort_keys=True)
                output.write("\n")
                output.flush()
                os.fsync(output.fileno())
            final = self.root / "build.json"
            temporary.replace(final)
            self.completed = True
            return final
        finally:
            temporary.unlink(missing_ok=True)

    def __exit__(self, _type, _value, _traceback):
        return False


def build_pair(root: Path, pair: str, profile: str) -> Path:
    spec = pair_spec(pair)
    profile_flags(profile)
    if not spec.build_supported:
        raise RuntimeError(
            f"build support for {pair} is not implemented yet; fetch is available and the adapter plan must land first"
        )
    if pair != "png":
        raise AssertionError("implemented pair is missing its builder")
    return _build_png(root, spec, profile)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _source_root(cache_path: Path) -> Path:
    children = [path for path in cache_path.iterdir() if path.name != ".diffray-source.json"]
    if len(children) != 1 or not children[0].is_dir():
        raise RuntimeError(f"expected one extracted source directory in {cache_path}")
    return children[0]


def _build_png(root: Path, spec, profile: str) -> Path:
    lock = load_lock(root / "third_party" / "real-targets.lock.json")
    entries = source_entries(lock)
    cache = root / "run" / "real-targets" / "sources"
    sources = {name: fetch_source(entries[name], cache) for name in ("zlib", "libpng", "libspng")}
    wrapper = root / "app" / "compiler" / "target" / "debug" / "libafl_forkserver_cc"
    tool_path = root / "run" / "toolchain" / "bin"
    subprocess.run(
        ["cargo", "build", "--locked", "--manifest-path", str(root / "app/compiler/Cargo.toml")],
        cwd=root,
        env={**os.environ, "PATH": str(tool_path) + os.pathsep + os.environ["PATH"]},
        check=True,
        stdout=subprocess.DEVNULL,
    )
    compiler_env = {**os.environ, "PATH": str(tool_path) + os.pathsep + os.environ["PATH"], "DIFFRAY_TARGET_PROFILE": profile}
    compiler_version = subprocess.run([str(wrapper), "--version"], env=compiler_env, capture_output=True, text=True, check=True).stdout.strip()
    flags = profile_flags(profile)
    lock_subset = {name: entries[name] for name in spec.dependencies}
    tracked = [root / spec.target_a, root / spec.target_b, root / spec.contract_path]
    tracked.extend(sorted((root / "examples/real-targets/common").glob("*.c")))
    tracked.extend(sorted((root / "examples/real-targets/common").glob("*.h")))
    inputs = {
        "profile": profile,
        "compiler": {"path": str(wrapper.relative_to(root)), "version": compiler_version},
        "flags": {"compile": flags.compile, "link": flags.link},
        "dependencies": lock_subset,
        "sources": {str(path.relative_to(root)): _sha256(path) for path in tracked},
        "build_logic": {
            str(path.relative_to(root)): _sha256(path)
            for path in (
                root / "scripts/real_targets.py",
                root / "scripts/real_targets_lib/builds.py",
                root / "scripts/real_targets_lib/registry.py",
            )
        },
        "seeds": {
            str(path.relative_to(root)): _sha256(path)
            for path in sorted((root / spec.seed_dir).glob("*"))
            if path.is_file()
        },
        "wrapper_runtime_sha256": _sha256(root / "app/compiler/target/debug/liblibforkserver_libafl_cc.a"),
        "configure": {"shared": False, "tests": False, "hardware_optimizations": False},
    }
    fingerprint = build_fingerprint(inputs)
    build_root = root / "run" / "real-targets" / "build" / spec.name / profile / fingerprint
    manifest = build_root / "build.json"
    if manifest.is_file():
        recorded = json.loads(manifest.read_text())
        if recorded.get("status") == "complete" and all((root / value["path"]).is_file() for value in recorded["targets"].values()):
            return manifest
    if build_root.exists():
        shutil.rmtree(build_root)
    work = build_root / "work"
    install = build_root / "install"
    targets = build_root / "targets"
    work.mkdir(parents=True)
    install.mkdir()
    targets.mkdir()
    base_env = clean_build_environment(
        compiler_env,
        profile=profile,
        cc=str(wrapper),
        cxx=str(root / "app/compiler/target/debug/libafl_forkserver_cxx"),
        ar="/usr/bin/ar",
        ranlib="/usr/bin/ranlib",
    )
    base_env["CFLAGS"] = " ".join(flags.compile)
    base_env["LDFLAGS"] = " ".join(flags.link)

    with BuildManifestTransaction(build_root, {"schema_version": 1, "fingerprint": fingerprint, "fingerprint_inputs": inputs}) as transaction:
        def run(argv, env=None):
            effective = dict(base_env if env is None else env)
            transaction.record_command(argv, {key: effective[key] for key in sorted(effective) if key in {"PATH", "CC", "CXX", "AR", "RANLIB", "CFLAGS", "LDFLAGS", "DIFFRAY_TARGET_PROFILE"}})
            with (build_root / "build.log").open("a", encoding="utf-8") as log:
                subprocess.run(argv, cwd=root, env=effective, check=True, stdout=log, stderr=subprocess.STDOUT)

        common_cmake = ["-DCMAKE_BUILD_TYPE=Release", "-DCMAKE_C_FLAGS_RELEASE=", f"-DCMAKE_C_COMPILER={wrapper}", "-DCMAKE_AR=/usr/bin/ar", "-DCMAKE_RANLIB=/usr/bin/ranlib", f"-DCMAKE_C_FLAGS={' '.join(flags.compile)}"]
        zlib_build, zlib_install = work / "zlib", install / "zlib"
        zlib_source = work / "source-zlib"
        shutil.copytree(_source_root(sources["zlib"]), zlib_source)
        run(["cmake", "-S", str(zlib_source), "-B", str(zlib_build), *common_cmake, "-DZLIB_BUILD_EXAMPLES=OFF"])
        run(["cmake", "--build", str(zlib_build), "--target", "zlibstatic", "--parallel", "4"])
        (zlib_install / "include").mkdir(parents=True)
        (zlib_install / "lib").mkdir()
        shutil.copy2(zlib_source / "zlib.h", zlib_install / "include/zlib.h")
        shutil.copy2(zlib_build / "zconf.h", zlib_install / "include/zconf.h")
        shutil.copy2(zlib_build / "libz.a", zlib_install / "lib/libz.a")
        png_build, png_install = work / "libpng", install / "libpng"
        run(["cmake", "-S", str(_source_root(sources["libpng"])), "-B", str(png_build), *common_cmake, "-DPNG_SHARED=OFF", "-DPNG_STATIC=ON", "-DPNG_TESTS=OFF", "-DPNG_TOOLS=OFF", "-DPNG_EXECUTABLES=OFF", "-DPNG_HARDWARE_OPTIMIZATIONS=OFF", f"-DZLIB_ROOT={zlib_install}", f"-DCMAKE_INSTALL_PREFIX={png_install}"])
        run(["cmake", "--build", str(png_build), "--target", "install", "--parallel", "4"])
        spng_build, spng_install = work / "libspng", install / "libspng"
        run(["cmake", "-S", str(_source_root(sources["libspng"])), "-B", str(spng_build), *common_cmake, "-DSPNG_SHARED=OFF", "-DSPNG_STATIC=ON", "-DBUILD_EXAMPLES=OFF", "-DENABLE_OPT=OFF", f"-DZLIB_ROOT={zlib_install}", f"-DCMAKE_INSTALL_PREFIX={spng_install}"])
        run(["cmake", "--build", str(spng_build), "--target", "install", "--parallel", "4"])
        zlib_archive = next(zlib_install.rglob("libz.a"))
        png_archive = next(png_install.rglob("libpng16.a"))
        spng_archive = next(spng_install.rglob("libspng_static.a"))
        common = root / "examples/real-targets/common"
        for target, adapter, include, archives in (
            ("a", root / spec.target_a, png_install / "include", [png_archive, zlib_archive]),
            ("b", root / spec.target_b, spng_install / "include", [spng_archive, zlib_archive]),
        ):
            output = targets / f"png-{target}"
            run([str(wrapper), *flags.compile, "-I", str(common), "-I", str(include), str(common / "semantic.c"), str(common / "harness_io.c"), str(adapter), *map(str, archives), "-lm", *flags.link, "-o", str(output)])
        target_records = {}
        for target in ("a", "b"):
            path = targets / f"png-{target}"
            relative = path.relative_to(root)
            ldd = subprocess.run(["ldd", str(path)], capture_output=True, text=True, check=True).stdout
            target_records[target] = {"path": str(relative), "sha256": _sha256(path), "ldd": ldd}
        return transaction.complete({"pair": spec.name, "operation": spec.operation, "profile": profile, "source_sha256": {name: entries[name]["sha256"] for name in spec.dependencies}, "target_source_sha256": inputs["sources"], "seed_sha256": inputs["seeds"], "effective_flags": inputs["flags"], "compiler": inputs["compiler"], "licenses": {name: entries[name]["license"] for name in spec.dependencies}, "targets": target_records})
