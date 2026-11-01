#!/usr/bin/env bash
#
# Behavior test for the OneSpace release helper (release_tool.py).
#
# Drives the helper against temporary git repositories only. It never touches
# the real repository or the network.
#
# Usage:
#   bash skills/onespace-release/scripts/release_tool.test.sh

set -u
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RELEASE_TOOL="$SCRIPT_DIR/release_tool.py"

ok() {
  printf 'ok - %s\n' "$1"
}

fail() {
  printf 'FAIL - %s\n' "$1" >&2
  exit 1
}

WORK_DIR=""
cleanup() {
  if [ -n "$WORK_DIR" ] && [ -d "$WORK_DIR" ]; then
    rm -rf "$WORK_DIR"
  fi
}
trap cleanup EXIT

WORK_DIR="$(mktemp -d)" || fail "could not create temporary work dir"
ok "temporary work dir: $WORK_DIR"

# Create a minimal release repository at the given version, committed and clean.
# Args: <dir> <version>
make_repo() {
  local dir="$1" version="$2"
  mkdir -p "$dir/src-tauri" || return 1
  printf '{"name":"OneSpace","version":"%s"}\n' "$version" > "$dir/package.json" || return 1
  printf '{"productName":"OneSpace","version":"%s"}\n' "$version" > "$dir/src-tauri/tauri.conf.json" || return 1
  printf '[package]\nname = "OneSpace"\nversion = "%s"\n' "$version" > "$dir/src-tauri/Cargo.toml" || return 1
  printf '[[package]]\nname = "OneSpace"\nversion = "%s"\n' "$version" > "$dir/src-tauri/Cargo.lock" || return 1
  git -C "$dir" init -q || return 1
  git -C "$dir" config user.email "test@example.com" || return 1
  git -C "$dir" config user.name "test" || return 1
  git -C "$dir" add -A || return 1
  git -C "$dir" commit -q -m "init $version" || return 1
  # Provide an origin so tag preflight's ls-remote does not fail.
  git -C "$dir" init -q --bare "$dir/origin.git" >/dev/null 2>&1 || return 1
  git -C "$dir" remote add origin "$dir/origin.git" || return 1
}

file_versions() {
  # Print all four version strings joined by space.
  local dir="$1"
  python3 - "$dir" <<'PY'
import json, re, sys
d = sys.argv[1]
pkg = json.load(open(d + "/package.json"))["version"]
tau = json.load(open(d + "/src-tauri/tauri.conf.json"))["version"]
cargo = re.search(r'(?m)^version\s*=\s*"([^"]+)"', open(d + "/src-tauri/Cargo.toml").read()).group(1)
lock = open(d + "/src-tauri/Cargo.lock").read()
import re as _r
m = _r.search(r'name = "OneSpace"\nversion = "([^"]+)"', lock)
lockv = m.group(1)
print(pkg, tau, cargo, lockv)
PY
}

snapshot_files() {
  local dir="$1" snap="$2"
  mkdir -p "$snap"
  cp "$dir/package.json" "$snap/package.json"
  cp "$dir/src-tauri/tauri.conf.json" "$snap/tauri.conf.json"
  cp "$dir/src-tauri/Cargo.toml" "$snap/Cargo.toml"
  cp "$dir/src-tauri/Cargo.lock" "$snap/Cargo.lock"
}

files_unchanged() {
  local dir="$1" snap="$2"
  cmp -s "$dir/package.json" "$snap/package.json" &&
  cmp -s "$dir/src-tauri/tauri.conf.json" "$snap/tauri.conf.json" &&
  cmp -s "$dir/src-tauri/Cargo.toml" "$snap/Cargo.toml" &&
  cmp -s "$dir/src-tauri/Cargo.lock" "$snap/Cargo.lock"
}

# ---------------------------------------------------------------------------
# AC-001: first beta from stable
# ---------------------------------------------------------------------------
REPO1="$WORK_DIR/repo1"
make_repo "$REPO1" "1.2.3" || fail "could not create repo1"
cd "$REPO1" || fail "cd repo1"
if ! python3 "$RELEASE_TOOL" beta 1.2.4 >/tmp/rt1.out 2>&1; then
  fail "AC-001: beta 1.2.4 must succeed from 1.2.3"
fi
V1="$(file_versions "$REPO1")"
[ "$V1" = "1.2.4-beta.1 1.2.4-beta.1 1.2.4-beta.1 1.2.4-beta.1" ] \
  || fail "AC-001: expected all files 1.2.4-beta.1, got: $V1"
if ! python3 "$RELEASE_TOOL" validate >/tmp/rt1v.out 2>&1; then
  fail "AC-001: validate must pass after beta write"
fi
ok "AC-001 first beta writes 1.2.4-beta.1 and validate passes"

# ---------------------------------------------------------------------------
# AC-002: next beta of same base
# ---------------------------------------------------------------------------
REPO2="$WORK_DIR/repo2"
make_repo "$REPO2" "1.2.4-beta.1" || fail "could not create repo2"
cd "$REPO2" || fail "cd repo2"
if ! python3 "$RELEASE_TOOL" beta 1.2.4 >/tmp/rt2.out 2>&1; then
  fail "AC-002: beta 1.2.4 must succeed from 1.2.4-beta.1"
fi
V2="$(file_versions "$REPO2")"
[ "$V2" = "1.2.4-beta.2 1.2.4-beta.2 1.2.4-beta.2 1.2.4-beta.2" ] \
  || fail "AC-002: expected all files 1.2.4-beta.2, got: $V2"
ok "AC-002 next beta writes 1.2.4-beta.2"

# ---------------------------------------------------------------------------
# AC-003: invalid/regressive base rejected, no file changes
# ---------------------------------------------------------------------------
REPO3="$WORK_DIR/repo3"
make_repo "$REPO3" "1.2.4" || fail "could not create repo3"
cd "$REPO3" || fail "cd repo3"
SNAP3="$WORK_DIR/snap3"
snapshot_files "$REPO3" "$SNAP3"
if python3 "$RELEASE_TOOL" beta 1.2.4 >/tmp/rt3.out 2>&1; then
  fail "AC-003: beta 1.2.4 must fail from stable 1.2.4"
fi
files_unchanged "$REPO3" "$SNAP3" || fail "AC-003: files changed on rejected beta 1.2.4"

REPO4="$WORK_DIR/repo4"
make_repo "$REPO4" "1.2.4-beta.2" || fail "could not create repo4"
cd "$REPO4" || fail "cd repo4"
SNAP4="$WORK_DIR/snap4"
snapshot_files "$REPO4" "$SNAP4"
if python3 "$RELEASE_TOOL" beta 1.2.3 >/tmp/rt4.out 2>&1; then
  fail "AC-003: beta 1.2.3 must fail from 1.2.4-beta.2"
fi
files_unchanged "$REPO4" "$SNAP4" || fail "AC-003: files changed on rejected beta 1.2.3"
ok "AC-003 invalid/regressive bases rejected with no changes"

# ---------------------------------------------------------------------------
# beta rejects non-base value such as 1.2.4.5
# ---------------------------------------------------------------------------
REPO5="$WORK_DIR/repo5"
make_repo "$REPO5" "1.2.3" || fail "could not create repo5"
cd "$REPO5" || fail "cd repo5"
SNAP5="$WORK_DIR/snap5"
snapshot_files "$REPO5" "$SNAP5"
if python3 "$RELEASE_TOOL" beta 1.2.4.5 >/tmp/rt5.out 2>&1; then
  fail "beta must reject non-base value 1.2.4.5"
fi
files_unchanged "$REPO5" "$SNAP5" || fail "files changed on rejected non-base beta"
ok "beta rejects non-base value 1.2.4.5"

# ---------------------------------------------------------------------------
# AC-004: dry run writes nothing
# ---------------------------------------------------------------------------
REPO6="$WORK_DIR/repo6"
make_repo "$REPO6" "1.2.3" || fail "could not create repo6"
cd "$REPO6" || fail "cd repo6"
SNAP6="$WORK_DIR/snap6"
snapshot_files "$REPO6" "$SNAP6"
if ! python3 "$RELEASE_TOOL" beta 1.2.4 --dry-run >/tmp/rt6.out 2>&1; then
  fail "AC-004: beta 1.2.4 --dry-run must succeed"
fi
grep -q "1.2.4-beta.1" /tmp/rt6.out || fail "AC-004: dry run must report next beta"
files_unchanged "$REPO6" "$SNAP6" || fail "AC-004: dry run changed files"
ok "AC-004 dry run reports next beta and leaves files unchanged"

# ---------------------------------------------------------------------------
# AC-005: previous-tag and is-prerelease
# ---------------------------------------------------------------------------
REPO7="$WORK_DIR/repo7"
mkdir -p "$REPO7/src-tauri" || fail "mkdir repo7"
printf '{"name":"OneSpace","version":"1.2.4"}\n' > "$REPO7/package.json"
git -C "$REPO7" init -q || fail "git init repo7"
git -C "$REPO7" config user.email "test@example.com" || fail "config email"
git -C "$REPO7" config user.name "test" || fail "config name"
# commit 1 + stable tag
printf 'a\n' > "$REPO7/a.txt"
git -C "$REPO7" add -A && git -C "$REPO7" commit -q -m "c1" || fail "commit1"
git -C "$REPO7" tag -a v1.2.3 -m "stable" || fail "tag v1.2.3"
# commit 2 + beta1
printf 'b\n' > "$REPO7/b.txt"
git -C "$REPO7" add -A && git -C "$REPO7" commit -q -m "c2" || fail "commit2"
git -C "$REPO7" tag -a v1.2.4-beta.1 -m "beta1" || fail "tag beta1"
# commit 3 + beta2
printf 'c\n' > "$REPO7/c.txt"
git -C "$REPO7" add -A && git -C "$REPO7" commit -q -m "c3" || fail "commit3"
git -C "$REPO7" tag -a v1.2.4-beta.2 -m "beta2" || fail "tag beta2"

cd "$REPO7" || fail "cd repo7"

OUT_PT1="$(python3 "$RELEASE_TOOL" previous-tag v1.2.4-beta.2 2>&1)" || fail "AC-005: previous-tag v1.2.4-beta.2 failed"
[ "$OUT_PT1" = "v1.2.4-beta.1" ] || fail "AC-005: expected v1.2.4-beta.1, got: $OUT_PT1"

OUT_PT2="$(python3 "$RELEASE_TOOL" previous-tag v1.2.4 2>&1)" || fail "AC-005: previous-tag v1.2.4 failed"
[ "$OUT_PT2" = "v1.2.3" ] || fail "AC-005: expected v1.2.3, got: $OUT_PT2"

OUT_IP1="$(python3 "$RELEASE_TOOL" is-prerelease v1.2.4-beta.2 2>&1)" || fail "AC-005: is-prerelease beta failed"
[ "$OUT_IP1" = "true" ] || fail "AC-005: expected true, got: $OUT_IP1"

OUT_IP2="$(python3 "$RELEASE_TOOL" is-prerelease v1.2.4 2>&1)" || fail "AC-005: is-prerelease stable failed"
[ "$OUT_IP2" = "false" ] || fail "AC-005: expected false, got: $OUT_IP2"
ok "AC-005 previous-tag and is-prerelease behave correctly"

# ---------------------------------------------------------------------------
# Annotated prerelease tag: tag 1.2.4-beta.1 --dry-run exits 0 after beta write
# ---------------------------------------------------------------------------
REPO8="$WORK_DIR/repo8"
make_repo "$REPO8" "1.2.3" || fail "could not create repo8"
cd "$REPO8" || fail "cd repo8"
python3 "$RELEASE_TOOL" beta 1.2.4 >/tmp/rt8.out 2>&1 || fail "beta write for tag test failed"
git -C "$REPO8" add -A && git -C "$REPO8" commit -q -m "beta1" || fail "commit beta1"
if ! python3 "$RELEASE_TOOL" tag 1.2.4-beta.1 --dry-run >/tmp/rt8t.out 2>&1; then
  fail "tag 1.2.4-beta.1 --dry-run must exit 0 after beta write"
fi
ok "annotated prerelease tag dry-run exits 0"

printf 'PASS - release_tool.test.sh: all behavior checks passed\n'
