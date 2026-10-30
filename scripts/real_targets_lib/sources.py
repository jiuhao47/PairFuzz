import hashlib
import json
import os
import shutil
import tarfile
import tempfile
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath


class SourceError(RuntimeError):
    pass


_REQUIRED_FIELDS = (
    "name",
    "version",
    "tag",
    "url",
    "sha256",
    "archive",
    "license",
    "verification_reference",
)


def load_lock(path: Path) -> dict:
    try:
        lock = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SourceError(f"cannot read source lock {path}: {error}") from error
    if lock.get("schema_version") != 1 or not isinstance(lock.get("sources"), list):
        raise SourceError("source lock requires schema_version 1 and a sources list")
    names = set()
    for index, entry in enumerate(lock["sources"]):
        for field in _REQUIRED_FIELDS:
            if not entry.get(field):
                raise SourceError(f"source {index} is missing {field}")
        if "resolved_commit" not in entry:
            raise SourceError(f"source {index} is missing resolved_commit")
        if entry["name"] in names:
            raise SourceError(f"duplicate source name {entry['name']}")
        names.add(entry["name"])
        digest = entry["sha256"]
        if len(digest) != 64 or any(character not in "0123456789abcdef" for character in digest):
            raise SourceError(f"source {entry['name']} has invalid sha256")
    return lock


def _safe_parts(name: str) -> tuple[str, ...]:
    path = PurePosixPath(name)
    if path.is_absolute() or not path.parts or any(part in ("", ".", "..") for part in path.parts):
        raise SourceError(f"unsafe archive path: {name!r}")
    return path.parts


def _resolve_link(base: tuple[str, ...], target: str) -> None:
    target_path = PurePosixPath(target)
    if target_path.is_absolute():
        raise SourceError(f"unsafe archive link: {target!r}")
    stack = list(base)
    for part in target_path.parts:
        if part in ("", "."):
            continue
        if part == "..":
            if not stack:
                raise SourceError(f"unsafe archive link: {target!r}")
            stack.pop()
        else:
            stack.append(part)


def _validate_tar(archive: tarfile.TarFile) -> None:
    for member in archive.getmembers():
        parts = _safe_parts(member.name)
        if member.issym():
            _resolve_link(parts[:-1], member.linkname)
        elif member.islnk():
            _resolve_link((), member.linkname)
        elif not (member.isfile() or member.isdir()):
            raise SourceError(f"unsafe archive member type: {member.name!r}")


def _validate_zip(archive: zipfile.ZipFile) -> None:
    for member in archive.infolist():
        _safe_parts(member.filename)
        mode = member.external_attr >> 16
        if mode & 0o170000 == 0o120000:
            raise SourceError(f"unsafe zip link: {member.filename!r}")


def _download(url: str, destination: Path) -> str:
    digest = hashlib.sha256()
    try:
        with urllib.request.urlopen(url, timeout=120) as response, destination.open("wb") as output:
            while True:
                chunk = response.read(1024 * 1024)
                if not chunk:
                    break
                digest.update(chunk)
                output.write(chunk)
            output.flush()
            os.fsync(output.fileno())
    except Exception:
        destination.unlink(missing_ok=True)
        raise
    return digest.hexdigest()


def _extract(entry: dict, archive_path: Path, destination: Path) -> None:
    archive_kind = entry["archive"]
    if archive_kind in ("tar.gz", "tar.xz", "tar.bz2"):
        with tarfile.open(archive_path, mode="r:*") as archive:
            _validate_tar(archive)
            archive.extractall(destination, filter="data")
    elif archive_kind == "zip":
        with zipfile.ZipFile(archive_path) as archive:
            _validate_zip(archive)
            archive.extractall(destination)
    elif archive_kind == "raw":
        filename = entry.get("filename") or Path(urllib.parse.urlparse(entry["url"]).path).name
        _safe_parts(filename)
        shutil.copyfile(archive_path, destination / filename)
    else:
        raise SourceError(f"unsupported archive type {archive_kind!r}")


def fetch_source(entry: dict, cache: Path) -> Path:
    for field in _REQUIRED_FIELDS:
        if not entry.get(field):
            raise SourceError(f"source entry is missing {field}")
    cache.mkdir(parents=True, exist_ok=True)
    destination = cache / f"{entry['name']}-{entry['sha256']}"
    marker = destination / ".pairfuzz-source.json"
    if marker.is_file():
        try:
            recorded = json.loads(marker.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            recorded = {}
        if recorded.get("sha256") == entry["sha256"] and recorded.get("status") == "complete":
            return destination

    temporary = Path(tempfile.mkdtemp(prefix=".fetch-", dir=cache))
    archive_path = temporary / "archive"
    extracted = temporary / "source"
    extracted.mkdir()
    try:
        actual = _download(entry["url"], archive_path)
        if actual != entry["sha256"]:
            raise SourceError(
                f"SHA-256 mismatch for {entry['name']}: expected {entry['sha256']}, got {actual}"
            )
        _extract(entry, archive_path, extracted)
        (extracted / ".pairfuzz-source.json").write_text(
            json.dumps(
                {"schema_version": 1, "status": "complete", "name": entry["name"], "sha256": actual},
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        try:
            extracted.replace(destination)
        except FileExistsError:
            if not marker.is_file():
                raise SourceError(f"incomplete source cache already exists: {destination}")
        return destination
    finally:
        shutil.rmtree(temporary, ignore_errors=True)


def source_entries(lock: dict) -> dict[str, dict]:
    return {entry["name"]: entry for entry in lock["sources"]}
