import hashlib
import json
import os
import shlex
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


def magick_build_configuration(library: str, prefix: Path) -> dict:
    common = (
        "--disable-shared",
        "--enable-static",
        "--disable-openmp",
        "--without-modules",
        "--without-threads",
        "--without-magick-plus-plus",
        "--without-perl",
        "--without-bzlib",
        "--without-jpeg",
        "--without-png",
        "--without-tiff",
        "--without-webp",
        "--without-xml",
        "--without-zlib",
        "--without-zstd",
        "--without-x",
        f"--prefix={prefix}",
    )
    if library == "imagemagick":
        configure = (
            *common,
            "--with-quantum-depth=8",
            "--disable-hdri",
            "--without-utilities",
            "--without-autotrace",
            "--without-dps",
            "--without-fftw",
            "--without-flif",
            "--without-fpx",
            "--without-fontconfig",
            "--without-freetype",
            "--without-raqm",
            "--without-gslib",
            "--without-gvc",
            "--without-dmr",
            "--without-heic",
            "--without-jbig",
            "--without-jxl",
            "--without-lcms",
            "--without-openjp2",
            "--without-lqr",
            "--without-lzma",
            "--without-openexr",
            "--without-pango",
            "--without-raw",
            "--without-rsvg",
            "--without-uhdr",
            "--without-wmf",
            "--without-zip",
        )
    elif library == "graphicsmagick":
        configure = (
            *common,
            "--with-quantum-depth=8",
            "--without-fpx",
            "--without-gs",
            "--without-jbig",
            "--without-heif",
            "--without-jp2",
            "--without-jxl",
            "--without-lcms2",
            "--without-lzma",
            "--without-ttf",
            "--without-libzip",
            "--without-wmf",
        )
    else:
        raise ValueError(f"unknown Magick library {library!r}")
    return {
        "configure": configure,
        "coder_allowlist": ["BMP"],
        "threads": 1,
        "memory_limit_bytes": 64 * 1024 * 1024,
        "map_limit_bytes": 64 * 1024 * 1024,
    }


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
    if pair == "png":
        return _build_png(root, spec, profile)
    if pair == "bmp":
        return _build_bmp(root, spec, profile)
    raise AssertionError("implemented pair is missing its builder")


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


def _write_imagemagick_policy(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)
    (path / "policy.xml").write_text(
        """<policymap>
  <policy domain="delegate" rights="none" pattern="*" />
  <policy domain="filter" rights="none" pattern="*" />
  <policy domain="coder" rights="none" pattern="*" />
  <policy domain="coder" rights="read" pattern="BMP" />
</policymap>
"""
    )


def _pkg_config_flags(pkgconfig: Path, package: str, option: str) -> list[str]:
    completed = subprocess.run(
        ["pkg-config", option, "--static", package],
        env={"PATH": os.environ["PATH"], "PKG_CONFIG_PATH": str(pkgconfig)},
        capture_output=True,
        text=True,
        check=True,
    )
    return shlex.split(completed.stdout)


def _build_bmp(root: Path, spec, profile: str) -> Path:
    lock = load_lock(root / "third_party" / "real-targets.lock.json")
    entries = source_entries(lock)
    cache = root / "run" / "real-targets" / "sources"
    sources = {
        name: fetch_source(entries[name], cache)
        for name in ("imagemagick", "graphicsmagick")
    }
    wrapper = root / "app" / "compiler" / "target" / "debug" / "libafl_forkserver_cc"
    cxx_wrapper = root / "app" / "compiler" / "target" / "debug" / "libafl_forkserver_cxx"
    tool_path = root / "run" / "toolchain" / "bin"
    subprocess.run(
        ["cargo", "build", "--locked", "--manifest-path", str(root / "app/compiler/Cargo.toml")],
        cwd=root,
        env={**os.environ, "PATH": str(tool_path) + os.pathsep + os.environ["PATH"]},
        check=True,
        stdout=subprocess.DEVNULL,
    )
    compiler_env = {
        **os.environ,
        "PATH": str(tool_path) + os.pathsep + os.environ["PATH"],
        "DIFFRAY_TARGET_PROFILE": profile,
    }
    compiler_version = subprocess.run(
        [str(wrapper), "--version"],
        env=compiler_env,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    probe_cc = "/usr/bin/cc"
    probe_cxx = "/usr/bin/c++"
    probe_compiler_version = subprocess.run(
        [probe_cc, "--version"], capture_output=True, text=True, check=True
    ).stdout.splitlines()[0]
    flags = profile_flags(profile)
    configurations = {
        name: magick_build_configuration(name, Path(f"/fingerprint/{name}"))
        for name in ("imagemagick", "graphicsmagick")
    }
    tracked = [root / spec.target_a, root / spec.target_b, root / spec.contract_path]
    tracked.extend(sorted((root / "examples/real-targets/common").glob("*.c")))
    tracked.extend(sorted((root / "examples/real-targets/common").glob("*.h")))
    tracked.extend(sorted((root / "examples/real-targets/bmp").glob("bmp_scope.*")))
    inputs = {
        "profile": profile,
        "compiler": {"path": str(wrapper.relative_to(root)), "version": compiler_version},
        "configure_probe_compiler": {
            "cc": probe_cc,
            "cxx": probe_cxx,
            "version": probe_compiler_version,
        },
        "flags": {"compile": flags.compile, "link": flags.link},
        "preprocessor_flags": ("-D_GNU_SOURCE",),
        "dependencies": {name: entries[name] for name in spec.dependencies},
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
        "wrapper_runtime_sha256": _sha256(
            root / "app/compiler/target/debug/liblibforkserver_libafl_cc.a"
        ),
        "configure": configurations,
        "runtime_policy": {
            "coder_allowlist": ["BMP"],
            "delegates": "none",
            "configuration_path": "build-owned",
        },
    }
    fingerprint = build_fingerprint(inputs)
    build_root = root / "run/real-targets/build" / spec.name / profile / fingerprint
    manifest = build_root / "build.json"
    if manifest.is_file():
        recorded = json.loads(manifest.read_text())
        if recorded.get("status") == "complete" and all(
            (root / value["path"]).is_file() for value in recorded["targets"].values()
        ):
            return manifest
    if build_root.exists():
        shutil.rmtree(build_root)
    work = build_root / "work"
    install = build_root / "install"
    targets = build_root / "targets"
    policy = build_root / "policy"
    for path in (work, install, targets, policy):
        path.mkdir(parents=True)
    _write_imagemagick_policy(policy / "imagemagick")
    (policy / "graphicsmagick").mkdir()

    base_env = clean_build_environment(
        compiler_env,
        profile=profile,
        cc=str(wrapper),
        cxx=str(cxx_wrapper),
        ar="/usr/bin/ar",
        ranlib="/usr/bin/ranlib",
    )
    base_env["CFLAGS"] = " ".join(flags.compile)
    base_env["CXXFLAGS"] = " ".join(flags.compile)
    base_env["CPPFLAGS"] = "-D_GNU_SOURCE"
    base_env["LDFLAGS"] = " ".join(flags.link)
    probe_env = dict(base_env)
    probe_env.update({"CC": probe_cc, "CXX": probe_cxx, "LDFLAGS": ""})
    actual_configurations = {
        name: magick_build_configuration(name, install / name)
        for name in ("imagemagick", "graphicsmagick")
    }

    identity = {
        "schema_version": 1,
        "fingerprint": fingerprint,
        "fingerprint_inputs": inputs,
    }
    with BuildManifestTransaction(build_root, identity) as transaction:
        def run(argv, *, cwd, env=None):
            effective = dict(base_env if env is None else env)
            recorded_environment = {
                key: effective[key]
                for key in sorted(effective)
                if key
                in {
                    "PATH",
                    "CC",
                    "CXX",
                    "AR",
                    "RANLIB",
                    "CFLAGS",
                    "CXXFLAGS",
                    "CPPFLAGS",
                    "LDFLAGS",
                    "DIFFRAY_TARGET_PROFILE",
                }
            }
            transaction.record_command(argv, recorded_environment)
            with (build_root / "build.log").open("a", encoding="utf-8") as log:
                subprocess.run(
                    argv,
                    cwd=cwd,
                    env=effective,
                    check=True,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                )

        for name in ("imagemagick", "graphicsmagick"):
            source = work / f"source-{name}"
            build = work / f"build-{name}"
            shutil.copytree(_source_root(sources[name]), source)
            build.mkdir()
            run(
                [str(source / "configure"), *actual_configurations[name]["configure"]],
                cwd=build,
                env=probe_env,
            )
            compiler_overrides = [f"CC={wrapper}", f"CXX={cxx_wrapper}"]
            run(["make", *compiler_overrides, "--jobs=4", "install"], cwd=build)

        common = root / "examples/real-targets/common"
        bmp = root / "examples/real-targets/bmp"
        packages = {
            "a": ("imagemagick", "MagickCore", root / spec.target_a),
            "b": ("graphicsmagick", "GraphicsMagick", root / spec.target_b),
        }
        for role, (name, package, adapter) in packages.items():
            pkgconfig = install / name / "lib/pkgconfig"
            compile_flags = _pkg_config_flags(pkgconfig, package, "--cflags")
            link_flags = _pkg_config_flags(pkgconfig, package, "--libs")
            output = targets / f"bmp-{role}"
            config_path = policy / name
            run(
                [
                    str(wrapper),
                    *flags.compile,
                    *compile_flags,
                    "-I",
                    str(common),
                    "-I",
                    str(bmp),
                    f'-DDIFFRAY_MAGICK_CONFIG_PATH="{config_path}"',
                    str(common / "semantic.c"),
                    str(common / "harness_io.c"),
                    str(bmp / "bmp_scope.c"),
                    str(adapter),
                    *link_flags,
                    *flags.link,
                    "-o",
                    str(output),
                ],
                cwd=root,
            )

        target_records = {}
        for role in ("a", "b"):
            path = targets / f"bmp-{role}"
            ldd = subprocess.run(
                ["ldd", str(path)], capture_output=True, text=True, check=True
            ).stdout
            target_records[role] = {
                "path": str(path.relative_to(root)),
                "sha256": _sha256(path),
                "ldd": ldd,
            }
        coverage_archives = {}
        for name in ("imagemagick", "graphicsmagick"):
            archives = sorted((install / name).rglob("*.a"))
            coverage_archives[name] = [
                str(path.relative_to(root))
                for path in archives
                if b"__sanitizer_cov_trace_pc_guard" in subprocess.run(
                    ["nm", str(path)], capture_output=True, check=True
                ).stdout
            ]
        return transaction.complete(
            {
                "pair": spec.name,
                "operation": spec.operation,
                "profile": profile,
                "source_sha256": {
                    name: entries[name]["sha256"] for name in spec.dependencies
                },
                "target_source_sha256": inputs["sources"],
                "seed_sha256": inputs["seeds"],
                "effective_flags": inputs["flags"],
                "compiler": inputs["compiler"],
                "licenses": {
                    name: entries[name]["license"] for name in spec.dependencies
                },
                "configure": actual_configurations,
                "runtime_environment": {
                    "MAGICK_CONFIGURE_PATH": {
                        role: str((policy / name).relative_to(root))
                        for role, (name, _package, _adapter) in packages.items()
                    },
                    "coder_allowlist": ["BMP"],
                    "delegates": "none",
                },
                "coverage_archives": coverage_archives,
                "targets": target_records,
            }
        )
