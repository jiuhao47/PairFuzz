from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class PairSpec:
    name: str
    operation: str
    library_a: str
    library_b: str
    target_a: Path
    target_b: Path
    seed_dir: Path
    contract_path: Path
    dependencies: tuple[str, ...]
    build_supported: bool = False


_BASE = Path("examples/real-targets")
_PAIRS = {
    "png": PairSpec(
        "png",
        "image.png.decode.rgba8",
        "libpng",
        "libspng",
        _BASE / "png/libpng_target.c",
        _BASE / "png/libspng_target.c",
        _BASE / "png/seeds",
        _BASE / "png/contract.json",
        ("zlib", "libpng", "libspng", "xxhash", "xxhash-license"),
        True,
    ),
    "bmp": PairSpec(
        "bmp",
        "image.bmp.decode.rgba8",
        "imagemagick",
        "graphicsmagick",
        _BASE / "bmp/imagemagick_target.c",
        _BASE / "bmp/graphicsmagick_target.c",
        _BASE / "bmp/seeds",
        _BASE / "bmp/contract.json",
        ("imagemagick", "graphicsmagick", "xxhash", "xxhash-license"),
    ),
    "x509": PairSpec(
        "x509",
        "certificate.x509.parse.der",
        "openssl",
        "wolfssl",
        _BASE / "x509/openssl_target.c",
        _BASE / "x509/wolfssl_target.c",
        _BASE / "x509/seeds",
        _BASE / "x509/contract.json",
        ("openssl", "wolfssl", "xxhash", "xxhash-license"),
    ),
}


def pair_spec(pair: str) -> PairSpec:
    try:
        return _PAIRS[pair]
    except KeyError as error:
        raise ValueError(f"unknown pair {pair!r}; expected png, bmp, or x509") from error


def pair_names() -> tuple[str, ...]:
    return tuple(_PAIRS)
