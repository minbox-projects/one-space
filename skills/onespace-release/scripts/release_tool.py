#!/usr/bin/env python3
"""OneSpace release helper.

Features:
- show: display current versions in project release files
- validate: ensure versions are consistent
- bump: update versions in release files
- beta: start or advance a beta version for an explicit base
- previous-tag: resolve the tag preceding a release tag
- is-prerelease: report whether a release tag is a prerelease
- tag: run preflight checks then create annotated git tag (optional push)
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Union


SEMVER_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")
BASE_VERSION_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
BETA_VERSION_RE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)-beta\.(0|[1-9]\d*)$"
)


@dataclass(frozen=True)
class ReleaseTarget:
    key: str
    path: Path
    kind: str


@dataclass(frozen=True)
class SemVer:
    major: int
    minor: int
    patch: int
    prerelease: Tuple[Union[int, str], ...] = ()

    @property
    def is_prerelease(self) -> bool:
        return bool(self.prerelease)


def parse_semver(value: str) -> Optional[SemVer]:
    if not SEMVER_RE.match(value):
        return None

    core_text = value.split("+", 1)[0]
    version_text, _, prerelease_text = core_text.partition("-")
    major, minor, patch = (int(part) for part in version_text.split("."))
    prerelease: Tuple[Union[int, str], ...] = ()
    if prerelease_text:
        prerelease = tuple(
            int(part) if part.isdigit() else part
            for part in prerelease_text.split(".")
        )
    return SemVer(major, minor, patch, prerelease)


def semver_key(version: SemVer) -> Tuple[object, ...]:
    prerelease: Tuple[object, ...]
    if version.prerelease:
        identifiers = tuple(
            (0, identifier) if isinstance(identifier, int) else (1, identifier)
            for identifier in version.prerelease
        )
        prerelease = (0, identifiers)
    else:
        prerelease = (1,)
    return (version.major, version.minor, version.patch, prerelease)


def parse_release_tag(tag: str) -> Optional[SemVer]:
    if not tag.startswith("v"):
        return None
    return parse_semver(tag[1:])


def repo_root(start: Path) -> Path:
    current = start.resolve()
    for path in [current, *current.parents]:
        if (path / ".git").exists():
            return path
    raise RuntimeError("Cannot find repository root (.git)")


def release_targets(root: Path) -> List[ReleaseTarget]:
    return [
        ReleaseTarget("package_json", root / "package.json", "json"),
        ReleaseTarget("tauri_conf", root / "src-tauri" / "tauri.conf.json", "json"),
        ReleaseTarget("cargo_toml", root / "src-tauri" / "Cargo.toml", "toml"),
        ReleaseTarget("cargo_lock", root / "src-tauri" / "Cargo.lock", "cargo_lock"),
    ]


def read_versions(targets: List[ReleaseTarget]) -> Dict[str, str]:
    versions: Dict[str, str] = {}
    for target in targets:
        if not target.path.exists():
            raise FileNotFoundError(f"Missing file: {target.path}")

        text = target.path.read_text(encoding="utf-8")
        if target.kind == "json":
            data = json.loads(text)
            version = data.get("version")
            if not isinstance(version, str):
                raise ValueError(f"No string version field in {target.path}")
            versions[target.key] = version
        elif target.kind == "toml":
            match = re.search(r'(?m)^version\s*=\s*"([^"]+)"\s*$', text)
            if not match:
                raise ValueError(f"Cannot find version field in {target.path}")
            versions[target.key] = match.group(1)
        elif target.kind == "cargo_lock":
            version = read_cargo_lock_version(target.path)
            versions[target.key] = version
        else:
            raise ValueError(f"Unsupported target kind: {target.kind}")

    return versions


def cargo_lock_package_block(content: str, package_name: str) -> tuple[int, int, str]:
    block_pattern = re.compile(r"(?sm)^\[\[package\]\]\n.*?(?=^\[\[package\]\]\n|\Z)")
    for block in block_pattern.finditer(content):
        block_text = block.group(0)
        if re.search(rf'(?m)^name\s*=\s*"{re.escape(package_name)}"\s*$', block_text):
            return block.start(), block.end(), block_text
    raise ValueError(f"Cannot locate [[package]] block for {package_name}")


def read_cargo_lock_version(path: Path) -> str:
    content = path.read_text(encoding="utf-8")
    _start, _end, block_text = cargo_lock_package_block(content, "OneSpace")
    match = re.search(r'(?m)^version\s*=\s*"([^"]+)"\s*$', block_text)
    if not match:
        raise ValueError(f"Cannot find version field in OneSpace block: {path}")
    return match.group(1)


def validate_versions(versions: Dict[str, str]) -> List[str]:
    errors: List[str] = []
    values = list(versions.values())

    for key, value in versions.items():
        if not SEMVER_RE.match(value):
            errors.append(f"{key} has invalid semver: {value}")

    if len(set(values)) != 1:
        errors.append(
            "Version mismatch: "
            + ", ".join(f"{k}={v}" for k, v in versions.items())
        )

    return errors


def update_json_version(path: Path, version: str) -> None:
    data = json.loads(path.read_text(encoding="utf-8"))
    data["version"] = version
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def update_cargo_version(path: Path, version: str) -> None:
    content = path.read_text(encoding="utf-8")

    package_block = re.search(r"(?s)^\[package\].*?(?=^\[|\Z)", content, re.M)
    if not package_block:
        raise ValueError(f"Cannot locate [package] block in {path}")

    block_text = package_block.group(0)
    new_block = re.sub(
        r'(?m)^version\s*=\s*"[^"]+"\s*$',
        f'version = "{version}"',
        block_text,
        count=1,
    )

    if block_text == new_block:
        raise ValueError(f"Cannot update version line in [package] block: {path}")

    new_content = content[: package_block.start()] + new_block + content[package_block.end() :]
    path.write_text(new_content, encoding="utf-8")


def update_cargo_lock_version(path: Path, version: str) -> None:
    content = path.read_text(encoding="utf-8")
    start, end, block_text = cargo_lock_package_block(content, "OneSpace")
    new_block = re.sub(
        r'(?m)^version\s*=\s*"[^"]+"\s*$',
        f'version = "{version}"',
        block_text,
        count=1,
    )
    if block_text == new_block:
        raise ValueError(f"Cannot update OneSpace version in {path}")
    new_content = content[:start] + new_block + content[end:]
    path.write_text(new_content, encoding="utf-8")


def bump_versions(targets: List[ReleaseTarget], version: str, dry_run: bool) -> None:
    if not SEMVER_RE.match(version):
        raise ValueError(f"Invalid semver version: {version}")

    if dry_run:
        print(f"[dry-run] Would set version to {version} in:")
        for target in targets:
            print(f"  - {target.path}")
        return

    for target in targets:
        if target.kind == "json":
            update_json_version(target.path, version)
        elif target.kind == "toml":
            update_cargo_version(target.path, version)
        elif target.kind == "cargo_lock":
            update_cargo_lock_version(target.path, version)
        else:
            raise ValueError(f"Unsupported target kind: {target.kind}")


def compute_beta_version(current: str, base: str) -> str:
    base_match = BASE_VERSION_RE.match(base)
    if not base_match:
        raise ValueError(f"Invalid beta base: {base} (expected X.Y.Z)")

    target_core = tuple(int(base_match.group(index)) for index in (1, 2, 3))

    beta_match = BETA_VERSION_RE.match(current)
    if beta_match:
        current_core = tuple(int(beta_match.group(index)) for index in (1, 2, 3))
        beta_number = int(beta_match.group(4))
        if target_core < current_core:
            raise ValueError(
                f"Target base {base} is lower than current beta base "
                f"{'.'.join(str(part) for part in current_core)}"
            )
        if target_core == current_core:
            return f"{base}-beta.{beta_number + 1}"
        return f"{base}-beta.1"

    stable_match = BASE_VERSION_RE.match(current)
    if stable_match:
        current_core = tuple(int(stable_match.group(index)) for index in (1, 2, 3))
        if target_core <= current_core:
            raise ValueError(
                f"Target base {base} must be greater than current stable version {current}"
            )
        return f"{base}-beta.1"

    raise ValueError(f"Current version {current} is not a stable or beta version")


def run_git(args: List[str], root: Path) -> None:
    cmd = ["git", *args]
    try:
        subprocess.run(cmd, cwd=root, check=True)
    except subprocess.CalledProcessError as exc:
        raise RuntimeError(f"git command failed: {' '.join(cmd)}") from exc


def run_git_capture(args: List[str], root: Path) -> str:
    cmd = ["git", *args]
    try:
        completed = subprocess.run(
            cmd,
            cwd=root,
            check=True,
            text=True,
            capture_output=True,
        )
    except subprocess.CalledProcessError as exc:
        stderr = (exc.stderr or "").strip()
        detail = f": {stderr}" if stderr else ""
        raise RuntimeError(f"git command failed: {' '.join(cmd)}{detail}") from exc
    return completed.stdout


def git_ref_exists(root: Path, ref: str) -> bool:
    completed = subprocess.run(
        ["git", "rev-parse", "--verify", "--quiet", f"{ref}^{{commit}}"],
        cwd=root,
        text=True,
        capture_output=True,
    )
    return completed.returncode == 0


def reachable_release_tags(root: Path, tag: str) -> List[str]:
    merged_ref = tag if git_ref_exists(root, tag) else "HEAD"
    output = run_git_capture(["tag", "--merged", merged_ref], root)
    tags: List[str] = []
    for line in output.splitlines():
        name = line.strip()
        if name and parse_release_tag(name) is not None:
            tags.append(name)
    return tags


def create_tag(root: Path, tag_name: str, message: str, push: bool, dry_run: bool) -> None:
    if dry_run:
        print(f"[dry-run] Would run: git tag -a {tag_name} -m {message!r}")
        if push:
            print(f"[dry-run] Would run: git push origin {tag_name}")
        return

    run_git(["tag", "-a", tag_name, "-m", message], root)
    if push:
        run_git(["push", "origin", tag_name], root)


def collect_tag_preflight_errors(root: Path, version: str, tag_name: str, prefix: str) -> List[str]:
    errors: List[str] = []
    targets = release_targets(root)
    versions = read_versions(targets)
    errors.extend(validate_versions(versions))

    distinct_versions = sorted(set(versions.values()))
    if len(distinct_versions) == 1 and distinct_versions[0] != version:
        errors.append(
            f"Requested tag version is {version}, but release files are {distinct_versions[0]}. "
            "Run bump first."
        )

    if prefix != "v":
        errors.append("Tag prefix must be 'v' for this repository policy.")

    status = run_git_capture(["status", "--short"], root).strip()
    if status:
        errors.append("Working tree is not clean. Commit or stash changes before tagging.")

    local_tag = run_git_capture(["tag", "--list", tag_name], root).strip()
    if local_tag:
        errors.append(f"Tag already exists locally: {tag_name}")

    remote_refs = run_git_capture(["ls-remote", "--tags", "origin", tag_name], root).strip()
    if remote_refs:
        errors.append(f"Tag already exists on origin: {tag_name}")

    return errors


def cmd_show(root: Path) -> int:
    targets = release_targets(root)
    versions = read_versions(targets)
    for key, value in versions.items():
        print(f"{key}: {value}")
    return 0


def cmd_validate(root: Path) -> int:
    targets = release_targets(root)
    versions = read_versions(targets)
    errors = validate_versions(versions)

    if errors:
        print("Validation failed:")
        for err in errors:
            print(f"- {err}")
        return 1

    print("Validation passed: all release versions are consistent.")
    return 0


def cmd_bump(root: Path, version: str, dry_run: bool) -> int:
    targets = release_targets(root)
    bump_versions(targets, version, dry_run=dry_run)

    if dry_run:
        return 0

    print(f"Updated versions to {version}")
    return cmd_validate(root)


def cmd_beta(root: Path, base: str, dry_run: bool) -> int:
    targets = release_targets(root)
    versions = read_versions(targets)
    errors = validate_versions(versions)

    if errors:
        print("Beta update failed:")
        for err in errors:
            print(f"- {err}")
        return 1

    current = next(iter(versions.values()))
    next_version = compute_beta_version(current, base)

    if dry_run:
        print(f"[dry-run] Next beta version: {next_version}")
        return 0

    bump_versions(targets, next_version, dry_run=False)
    print(f"Updated versions to {next_version}")
    return cmd_validate(root)


def cmd_previous_tag(root: Path, tag: str) -> int:
    current = parse_release_tag(tag)
    if current is None:
        print(f"Invalid release tag: {tag}", file=sys.stderr)
        return 1

    current_key = semver_key(current)
    candidates: List[Tuple[str, SemVer]] = []
    for name in reachable_release_tags(root, tag):
        candidate = parse_release_tag(name)
        if candidate is None:
            continue
        if semver_key(candidate) >= current_key:
            continue
        if not current.is_prerelease and candidate.is_prerelease:
            continue
        candidates.append((name, candidate))

    if candidates:
        previous_name, _ = max(candidates, key=lambda item: semver_key(item[1]))
        print(previous_name)
    return 0


def cmd_is_prerelease(tag: str) -> int:
    version = parse_release_tag(tag)
    if version is None:
        print(f"Invalid release tag: {tag}", file=sys.stderr)
        return 1

    print("true" if version.is_prerelease else "false")
    return 0


def cmd_tag(root: Path, version: str, prefix: str, push: bool, dry_run: bool) -> int:
    if not SEMVER_RE.match(version):
        print(f"Invalid semver: {version}", file=sys.stderr)
        return 1

    tag_name = f"{prefix}{version}"
    message = f"release: {tag_name}"

    preflight_errors = collect_tag_preflight_errors(root, version, tag_name, prefix)
    if preflight_errors:
        print("Tag preflight failed:")
        for err in preflight_errors:
            print(f"- {err}")
        return 1

    create_tag(root, tag_name, message, push=push, dry_run=dry_run)
    print(f"Prepared tag operation for {tag_name}")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="OneSpace release helper")
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("show", help="Show versions from release files")
    subparsers.add_parser("validate", help="Validate semver and consistency")

    bump_parser = subparsers.add_parser("bump", help="Set all release versions")
    bump_parser.add_argument("version", help="Target semver (e.g. 0.1.5)")
    bump_parser.add_argument("--dry-run", action="store_true", help="Do not write files")

    beta_parser = subparsers.add_parser("beta", help="Set the next beta version for a base")
    beta_parser.add_argument("base", help="Target base version (e.g. 0.1.58)")
    beta_parser.add_argument("--dry-run", action="store_true", help="Do not write files")

    previous_tag_parser = subparsers.add_parser(
        "previous-tag", help="Print the tag preceding a release tag"
    )
    previous_tag_parser.add_argument("tag", help="Release tag (e.g. v0.1.58-beta.2)")

    is_prerelease_parser = subparsers.add_parser(
        "is-prerelease", help="Print whether a tag version is a prerelease"
    )
    is_prerelease_parser.add_argument("tag", help="Release tag (e.g. v0.1.58-beta.2)")

    tag_parser = subparsers.add_parser("tag", help="Create annotated release tag")
    tag_parser.add_argument("version", help="Target semver (e.g. 0.1.5)")
    tag_parser.add_argument("--prefix", default="v", help="Tag prefix (default: v)")
    tag_parser.add_argument("--push", action="store_true", help="Push tag to origin")
    tag_parser.add_argument("--dry-run", action="store_true", help="Do not run git commands")

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()

    try:
        root = repo_root(Path.cwd())

        if args.command == "show":
            return cmd_show(root)
        if args.command == "validate":
            return cmd_validate(root)
        if args.command == "bump":
            return cmd_bump(root, args.version, args.dry_run)
        if args.command == "beta":
            return cmd_beta(root, args.base, args.dry_run)
        if args.command == "previous-tag":
            return cmd_previous_tag(root, args.tag)
        if args.command == "is-prerelease":
            return cmd_is_prerelease(args.tag)
        if args.command == "tag":
            return cmd_tag(root, args.version, args.prefix, args.push, args.dry_run)

        parser.print_help()
        return 1
    except Exception as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
