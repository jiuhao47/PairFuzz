#!/usr/bin/env python3
"""
Reorganize builds under benchmark/<project>/bugs into a deduplicated
all_builds directory and emit a JSON mapping from each bug folder to the
corresponding deduplicated build entries.

Usage examples:
  python3 reorg_builds.py --project libpng --out-dir ./libpng/all_builds
  python3 reorg_builds.py --dry-run

Behavior:
- For each immediate subdirectory under BUGS_DIR, it expects subdirectories
  (e.g. two children) representing compiled builds.
- It computes a content hash for each build directory (walks files, hashing
  relative paths + content). Directories with identical content are treated
  as the same build.
- Unique builds are materialized under OUT_DIR as numbered folders named
  like 0001_<short-hash>. By default, files are created as hardlinks where
  possible; if hardlinking fails, files are copied.
- Outputs a JSON mapping file (map.json by default) with the mapping.

Note: this script is conservative and does not delete or overwrite existing
OUT_DIR entries unless they exactly match the computed hash/name.
"""

import argparse
import hashlib
import json
import os
import shutil
import stat
import sys
from pathlib import Path
from typing import Dict, List, Tuple
# no grouping features needed in simplified script

CHUNK_SIZE = 1024 * 1024


def compute_dir_hash(path: Path) -> str:
    """Compute a SHA256 hash representing the contents and structure of a directory.

    The hash includes relative file paths (sorted) and file contents. Empty
    directories are represented by their path in the hash, so two trees with
    identical files produce the same hash.
    """
    h = hashlib.sha256()
    if not path.exists():
        raise FileNotFoundError(str(path))

    # Walk files in deterministic order
    all_files = []
    all_dirs = []
    for root, dirs, files in os.walk(path):
        rel_root = os.path.relpath(root, path)
        all_dirs.append(rel_root)
        for f in files:
            all_files.append(os.path.join(rel_root, f))

    all_dirs.sort()
    all_files.sort()

    for d in all_dirs:
        # include directory marker
        h.update(f"D:{d}\n".encode())

    for rel in all_files:
        abs_path = path.joinpath(rel)
        # normalize to forward slashes for consistent hashing
        rel_norm = rel.replace(os.path.sep, '/')
        h.update(f"F:{rel_norm}\n".encode())
        try:
            with open(abs_path, 'rb') as f:
                while True:
                    chunk = f.read(CHUNK_SIZE)
                    if not chunk:
                        break
                    h.update(chunk)
        except PermissionError:
            # include a marker so permission differences change the hash
            h.update(f"PERMERR:{rel_norm}\n".encode())
    return h.hexdigest()
from collections import defaultdict


def safe_makedirs(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)


def link_or_copy_tree(src: Path, dst: Path) -> None:
    """Try to replicate the tree under dst using hard links for files when possible;
    fall back to copying when linking fails.
    """
    safe_makedirs(dst)
    for root, dirs, files in os.walk(src):
        rel_root = os.path.relpath(root, src)
        target_root = dst.joinpath(rel_root) if rel_root != '.' else dst
        safe_makedirs(Path(target_root))
        for d in dirs:
            safe_makedirs(Path(target_root) / d)
        for f in files:
            sfile = Path(root) / f
            dfile = Path(target_root) / f
            if dfile.exists():
                # skip existing
                continue
            try:
                os.link(sfile, dfile)
            except Exception:
                # fallback to copy (preserve metadata)
                shutil.copy2(sfile, dfile)


def find_immediate_subdirs(path: Path) -> List[Path]:
    if not path.exists():
        return []
    return [p for p in sorted(path.iterdir()) if p.is_dir()]


def build_mapping(bugs_dir: Path, out_dir: Path, map_file: Path, dry_run: bool) -> Dict:
    mapping: Dict[str, List[str]] = {}
    builds_index: Dict[str, Dict] = {}
    next_idx = 1
    # per-bug child -> build name mapping
    child_map: Dict[str, List[Dict[str, str]]] = {}

    bug_folders = [p for p in sorted(bugs_dir.iterdir()) if p.is_dir()]

    for bf in bug_folders:
        children = find_immediate_subdirs(bf)
        if not children:
            print(f"warning: skipping '{bf}' - no subdirectories found", file=sys.stderr)
            continue
        rel_children = []
        child_map[str(bf)] = []
        for child in children:
            try:
                h = compute_dir_hash(child)
            except Exception as e:
                print(f"error hashing {child}: {e}", file=sys.stderr)
                continue
            if h not in builds_index:
                short = h[:12]
                # build a suffix from source basenames (first encounter)
                # collect up to two unique basenames
                src_basename = Path(child).name
                # sanitize: keep alnum, dash, underscore
                def _sanitize(s: str) -> str:
                    import re
                    s2 = re.sub(r'[^A-Za-z0-9._-]+', '_', s)
                    return s2[:30]
                suffix = _sanitize(src_basename)
                name = f"{next_idx:04d}_{short}_{suffix}"
                target = out_dir / name
                builds_index[h] = {
                    'id': next_idx,
                    'hash': h,
                    'name': name,
                    'target': str(target),
                    'sources': [str(child)],
                }
                next_idx += 1
                if not dry_run:
                    # create target by linking/copying
                    if target.exists():
                        # If it exists, we verify its hash by recomputing to avoid collisions
                        try:
                            existing_hash = compute_dir_hash(target)
                            if existing_hash != h:
                                print(f"warning: existing target {target} exists but hash differs; creating new folder anyway", file=sys.stderr)
                                # create a unique name
                                name = f"{builds_index[h]['id']:04d}_{short}_{os.getpid()}"
                                target = out_dir / name
                                builds_index[h]['name'] = name
                                builds_index[h]['target'] = str(target)
                                link_or_copy_tree(child, target)
                            else:
                                # identical, reuse
                                pass
                        except Exception:
                            link_or_copy_tree(child, target)
                    else:
                        link_or_copy_tree(child, target)
            else:
                builds_index[h]['sources'].append(str(child))
            # add build name to the mapping for this bug folder, but avoid duplicates
            bname = builds_index[h]['name']
            if bname not in rel_children:
                rel_children.append(bname)
            # record child->build mapping
            child_map[str(bf)].append({'child': str(child), 'build': bname})
        mapping[str(bf)] = rel_children

    # keep all_builds minimal to avoid redundancy in map.json
    all_builds = [
        {
            'id': info['id'],
            'name': info['name'],
            'hash': info['hash'],
            'target': info['target'],
        }
        for info in sorted(builds_index.values(), key=lambda x: x['id'])
    ]

    # group bug folders by their build-sets (order-insensitive)
    groups: Dict[Tuple[str, ...], List[str]] = defaultdict(list)
    for folder, builds in mapping.items():
        key = tuple(sorted(builds))
        groups[key].append(folder)

    # only keep duplicate groups for convenience (groups with >1 folder)
    duplicate_groups = [v for v in groups.values() if len(v) > 1]

    out_full = {
        'all_builds': all_builds,
        'mapping': mapping,
        'child_map': child_map,
        'groups': duplicate_groups,
    }

    # For the on-disk map.json include detailed all_builds information so callers
    # can find targets and sources without rerunning the scanner.
    # Each entry will include id, name, target, hash and sources when available.
    public_all_builds = []
    # builds_index is not directly in scope here; rebuild a small index from all_builds
    for b in all_builds:
        entry = {
            'id': b.get('id'),
            'name': b.get('name'),
            'target': b.get('target'),
        }
        # include hash and sources when present in the original info
        if 'hash' in b:
            entry['hash'] = b['hash']
        if 'sources' in b:
            entry['sources'] = b['sources']
        public_all_builds.append(entry)

    public_out = {
        'all_builds': public_all_builds,
        'mapping': mapping,
    }

    if not dry_run:
        safe_makedirs(map_file.parent)
        with open(map_file, 'w', encoding='utf-8') as f:
            json.dump(public_out, f, indent=2, ensure_ascii=False)

    return out_full


def main() -> None:
    p = argparse.ArgumentParser(description='Reorganize builds folders into deduplicated all_builds')
    p.add_argument('--project', type=str, default='libpng', help='project folder under benchmark (e.g. libpng, libspng)')
    p.add_argument('--bugs-dir', type=Path, default=None, help='path to <project>/bugs (overrides --project)')
    p.add_argument('--out-dir', type=Path, default=None, help='path to output deduplicated builds (defaults to <project>/all_builds)')
    p.add_argument('--map-file', type=Path, default=None, help='path to output mapping json (defaults to <project>/map.json)')
    p.add_argument('--dry-run', action='store_true', help='do everything except creating files (prints summary and mapping to stdout)')
    # Simplified: we only support dedup -> out_dir and per-bug child symlinks (--link-bugs)
    p.add_argument('--link-bugs', action='store_true', help='replace each bug subdirectory child with a symlink to the corresponding all_builds entry (destructive)')
    p.add_argument('--apply-physical', action='store_true', help='physically merge bug-folders that require the same set of builds (destructive)')
    p.add_argument('--confirm', action='store_true', help='must be provided to actually perform destructive filesystem operations')
    args = p.parse_args()

    # compute defaults based on --project when explicit paths not provided
    script_dir = Path(__file__).resolve().parent
    project = args.project
    default_project_dir = script_dir / project

    bugs_dir: Path = args.bugs_dir if args.bugs_dir is not None else default_project_dir / 'bugs'
    out_dir: Path = args.out_dir if args.out_dir is not None else default_project_dir / 'all_builds'
    map_file: Path = args.map_file if args.map_file is not None else default_project_dir / 'map.json'

    print(f"Scanning bugs directory: {bugs_dir}")
    print(f"Output directory: {out_dir}")
    print(f"Map file: {map_file}")
    if args.dry_run:
        print("DRY RUN: no files will be created or modified")

    # compute mapping / materialize unique builds

    if not bugs_dir.exists():
        print(f"Error: bugs dir '{bugs_dir}' does not exist", file=sys.stderr)
        sys.exit(2)

    result = build_mapping(bugs_dir, out_dir, map_file, args.dry_run)

    # If requested, replace each bug-folder's child dirs with symlinks to all_builds
    if args.link_bugs:
        if not (args.dry_run or args.confirm):
            print('Refusing to replace bug children with symlinks without --confirm (or use --dry-run to preview)', file=sys.stderr)
            sys.exit(3)
        # child_map maps bug-folder -> list of {child, build}
        cm = result.get('child_map', {})
        planned = []
        for bugfolder, entries in cm.items():
            for e in entries:
                child = Path(e['child'])
                build_name = e['build']
                target = out_dir / build_name
                planned.append((child, target))
        # perform operations
        for child, target in planned:
            if args.dry_run:
                print(f"DRY RUN: would replace {child} -> symlink to {target}")
                continue
            # remove child dir and create relative symlink
            try:
                if child.exists() and child.is_dir():
                    shutil.rmtree(child)
                rel = os.path.relpath(str(target), start=str(child.parent))
                os.symlink(rel, child)
                print(f"replaced {child} with symlink -> {rel}")
            except Exception as e:
                print(f"failed to replace {child} with symlink: {e}", file=sys.stderr)

    # no group-based merge/symlink functionality in simplified script

    # print compact summary
    print('\nSummary:')
    print(f"  unique builds: {len(result['all_builds'])}")
    print(f"  bug folders processed: {len(result['mapping'])}")
    if args.dry_run:
        # output mapping to stdout in dry-run mode
        print('\nMapping JSON:')
        print(json.dumps(result, indent=2, ensure_ascii=False))
    else:
        print(f"Wrote mapping to: {map_file}")

    # If requested, physically merge bug-folders that share the same build-set
    if args.apply_physical:
        if not (args.dry_run or args.confirm):
            print('Refusing to perform physical merge without --confirm (or use --dry-run to preview)', file=sys.stderr)
            sys.exit(4)

        groups = result.get('groups', [])
        if not groups:
            print('No duplicate groups found; nothing to merge')
        else:
            # groups is a list of lists of folder paths; only process groups with >1
            for grp in groups:
                if len(grp) <= 1:
                    continue
                # choose canonical folder deterministically (sorted order)
                canonical = Path(sorted(grp)[0])
                others = [Path(p) for p in sorted(grp)[1:]]
                print(f"Group: canonical={canonical} others={', '.join(str(p) for p in others)}")
                for src in others:
                    if not src.exists():
                        print(f"  skipping missing source folder: {src}")
                        continue
                    # move each immediate child from src into canonical unless name collides
                    children = list(src.iterdir())
                    for child in children:
                        dest = canonical / child.name
                        # if child is a symlink pointing into out_dir, and canonical already has same target -> safe to remove
                        try:
                            if child.is_symlink():
                                # resolve symlink target as an absolute path
                                link_target = (child.parent / os.readlink(str(child))).resolve()
                                try:
                                    out_dir_resolved = out_dir.resolve()
                                except Exception:
                                    out_dir_resolved = out_dir
                                if str(link_target).startswith(str(out_dir_resolved)):
                                    # canonical may already have the same symlink or a moved child
                                    if canonical.exists() and (canonical / child.name).exists():
                                        # safe to remove this symlink from src
                                        if args.dry_run:
                                            print(f"  DRY RUN: would remove symlink {child} (points to {link_target})")
                                        else:
                                            try:
                                                child.unlink()
                                                print(f"  removed symlink {child} -> {link_target}")
                                            except Exception as e:
                                                print(f"  failed to remove symlink {child}: {e}", file=sys.stderr)
                                        continue
                                    else:
                                        # canonical doesn't have it: move the symlink
                                        if args.dry_run:
                                            print(f"  DRY RUN: would move symlink {child} -> {dest}")
                                            continue
                                        try:
                                            shutil.move(str(child), str(dest))
                                            print(f"  moved symlink {child} -> {dest}")
                                            continue
                                        except Exception as e:
                                            print(f"  failed to move symlink {child} -> {dest}: {e}", file=sys.stderr)
                                # if symlink doesn't point into out_dir, fall back to move/skip logic below
                        except OSError:
                            # unreadable symlink; fall back
                            pass

                        # regular directory/file handling
                        if dest.exists():
                            print(f"  would skip moving {child} -> {dest} (destination exists)")
                            continue
                        if args.dry_run:
                            print(f"  DRY RUN: would move {child} -> {dest}")
                            continue
                        try:
                            shutil.move(str(child), str(dest))
                            print(f"  moved {child} -> {dest}")
                        except Exception as e:
                            print(f"  failed to move {child} -> {dest}: {e}", file=sys.stderr)
                    # after attempting moves, remove src if empty
                    if args.dry_run:
                        print(f"  DRY RUN: would remove folder if empty: {src}")
                    else:
                        try:
                            # only remove if directory is empty
                            if src.exists() and (not any(src.iterdir())):
                                src.rmdir()
                                print(f"  removed empty folder: {src}")
                            else:
                                print(f"  not removing {src} (not empty) ")
                        except Exception as e:
                            print(f"  failed to remove {src}: {e}", file=sys.stderr)

            # After physical operations, recompute mapping and rewrite map.json to reflect changes
            if args.dry_run:
                print('\nDRY RUN: physical merge preview complete; run without --dry-run and with --confirm to apply changes')
            else:
                print('\nRebuilding mapping after physical merge...')
                # recompute mapping to update map.json and internal structures
                result = build_mapping(bugs_dir, out_dir, map_file, False)
                print(f"Wrote mapping to: {map_file}")


if __name__ == '__main__':
    main()
