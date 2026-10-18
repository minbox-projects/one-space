#!/usr/bin/env bash
#
# Behavior test for the OneSpace macOS release signing tooling.
#
# It drives the two sibling scripts end to end against temporary paths only:
#
#   * create_signing_certificate.sh  -- one-time bootstrap of the code-signing
#     certificate, its p12 export and the GitHub secret material.
#   * verify_release_artifact.sh     -- pre-release/post-build verification of a
#     signed application bundle.
#
# Every runtime artifact lives under a single `mktemp -d` directory that is
# removed on exit, and the test creates its own throwaway keychain. It never
# touches the login keychain, never writes `~/onespace-release-signing` and
# never uses a pre-existing certificate.
#
# Usage:
#   bash skills/onespace-release/scripts/signing_tools.test.sh

set -u
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

CREATE_SCRIPT="$SCRIPT_DIR/create_signing_certificate.sh"
VERIFY_SCRIPT="$SCRIPT_DIR/verify_release_artifact.sh"
ENTITLEMENTS_PLIST="$REPO_ROOT/src-tauri/Entitlements.plist"

# Interface under test: the bootstrap defaults to the login keychain and to
# `~/onespace-release-signing`, so the test always passes an explicit throwaway
# keychain and an explicit output directory.
CERT_NAME="OneSpace Release Signing"
BUNDLE_IDENTIFIER="com.onespace.desktop"

WORK_DIR=""
PRIMARY_KEYCHAIN=""
SECONDARY_KEYCHAIN=""
KEYCHAIN_PASSWORD="onespace-signing-tools-test-keychain"

ok() {
  printf 'ok - %s\n' "$1"
}

fail() {
  printf 'FAIL - %s\n' "$1" >&2
  exit 1
}

cleanup() {
  if [ -n "$PRIMARY_KEYCHAIN" ]; then
    security delete-keychain "$PRIMARY_KEYCHAIN" </dev/null >/dev/null 2>&1 || true
  fi
  if [ -n "$SECONDARY_KEYCHAIN" ]; then
    security delete-keychain "$SECONDARY_KEYCHAIN" </dev/null >/dev/null 2>&1 || true
  fi
  if [ -n "$WORK_DIR" ] && [ -d "$WORK_DIR" ]; then
    rm -rf "$WORK_DIR"
  fi
}

trap cleanup EXIT

stat_mode() {
  stat -f '%Lp' "$1" 2>/dev/null
}

require_mode_0600() {
  local path="$1" mode
  mode="$(stat_mode "$path")"
  [ "$mode" = "600" ] || fail "expected mode 0600 for $path, got '${mode:-<unreadable>}'"
}

# Prints the first certificate-anchored designated-requirement line of the
# captured verification output, or nothing when the output has none.
requirement_line() {
  printf '%s\n' "$1" | grep -E 'certificate (root|leaf) = H"[0-9A-Fa-f]{40}"' | head -n 1
}

# Prints the lowercased certificate hash embedded in a requirement line.
requirement_hash() {
  printf '%s' "$1" | grep -E -o '[0-9A-Fa-f]{40}' | head -n 1 | tr 'A-Z' 'a-z'
}

# Prints the lowercase SHA-1 of the first certificate named CERT_NAME in the
# throwaway keychain, or nothing when the keychain holds no such certificate.
keychain_certificate_sha1() {
  local dump line
  dump="$(security find-certificate -a -c "$CERT_NAME" -Z "$PRIMARY_KEYCHAIN" 2>/dev/null)"
  line="$(printf '%s\n' "$dump" | grep -m 1 '^SHA-1 hash:')"
  line="${line#SHA-1 hash: }"
  printf '%s' "$line" | tr 'A-Z' 'a-z'
}

keychain_certificate_count() {
  local dump
  dump="$(security find-certificate -a -c "$CERT_NAME" -Z "$PRIMARY_KEYCHAIN" 2>/dev/null)"
  printf '%s\n' "$dump" | grep -c '^SHA-1 hash:'
}

# Creates a minimal signed-bundle fixture carrying the production bundle
# identifier.
make_bundle() {
  local app="$1" marker="$2"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" || return 1
  printf '#!/bin/sh\necho "%s"\n' "$marker" > "$app/Contents/MacOS/Test" || return 1
  chmod +x "$app/Contents/MacOS/Test" || return 1
  printf '%s\n' "$marker" > "$app/Contents/Resources/payload.txt" || return 1
  plutil -create xml1 "$app/Contents/Info.plist" || return 1
  plutil -insert CFBundleIdentifier -string "$BUNDLE_IDENTIFIER" "$app/Contents/Info.plist" || return 1
  plutil -insert CFBundleExecutable -string Test "$app/Contents/Info.plist" || return 1
  plutil -insert CFBundleName -string Test "$app/Contents/Info.plist" || return 1
  plutil -insert CFBundlePackageType -string APPL "$app/Contents/Info.plist" || return 1
}

sign_bundle_with_certificate() {
  local app="$1" label="$2" output
  if ! output="$(codesign --force --sign "$CERT_NAME" --keychain "$PRIMARY_KEYCHAIN" \
    --options runtime --entitlements "$ENTITLEMENTS_PLIST" "$app" </dev/null 2>&1)"; then
    fail "signing the $label with '$CERT_NAME' must succeed once the bootstrap imported the certificate: $output"
  fi
}

adhoc_sign_bundle() {
  local app="$1" label="$2" output
  if ! output="$(codesign --force --sign - "$app" </dev/null 2>&1)"; then
    fail "could not ad-hoc sign the $label fixture: $output"
  fi
}

signed_cdhash() {
  codesign -dvvv "$1" 2>&1 | grep '^CandidateCDHash ' | head -n 1
}

# Runs the verification script and exports VERIFY_OUTPUT / VERIFY_STATUS.
run_verification() {
  VERIFY_OUTPUT="$(bash "$VERIFY_SCRIPT" "$1" </dev/null 2>&1)"
  VERIFY_STATUS=$?
}

# ---------------------------------------------------------------------------
# Preconditions
# ---------------------------------------------------------------------------

[ -f "$CREATE_SCRIPT" ] \
  || fail "missing sibling bootstrap script $CREATE_SCRIPT"
[ -f "$VERIFY_SCRIPT" ] \
  || fail "missing sibling verification script $VERIFY_SCRIPT"
[ -f "$ENTITLEMENTS_PLIST" ] \
  || fail "missing entitlements plist $ENTITLEMENTS_PLIST"
ok "both signing scripts and src-tauri/Entitlements.plist are present"

TMP_BASE="${TMPDIR:-/tmp}"
WORK_DIR="$(mktemp -d "${TMP_BASE%/}/onespace-signing-tools-test.XXXXXX")" \
  || fail "could not create a temporary workspace with mktemp -d"
ok "temporary workspace: $WORK_DIR"

PRIMARY_KEYCHAIN="$WORK_DIR/bootstrap.keychain-db"
security create-keychain -p "$KEYCHAIN_PASSWORD" "$PRIMARY_KEYCHAIN" </dev/null >/dev/null 2>&1 \
  || fail "could not create the throwaway keychain $PRIMARY_KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$PRIMARY_KEYCHAIN" </dev/null >/dev/null 2>&1 \
  || fail "could not unlock the throwaway keychain $PRIMARY_KEYCHAIN"
ok "throwaway keychain created and unlocked"

# ---------------------------------------------------------------------------
# 1. Bootstrap, first run: creates the certificate and exports it
# ---------------------------------------------------------------------------

OUTPUT_FIRST="$WORK_DIR/output-first"
if ! BOOTSTRAP_FIRST_OUTPUT="$(bash "$CREATE_SCRIPT" \
    --keychain "$PRIMARY_KEYCHAIN" --output-dir "$OUTPUT_FIRST" </dev/null 2>&1)"; then
  fail "the first bootstrap run must succeed, but it exited non-zero: $BOOTSTRAP_FIRST_OUTPUT"
fi
ok "first bootstrap run succeeded"

P12_FILE="$OUTPUT_FIRST/onespace-release-signing.p12"
BASE64_FILE="$OUTPUT_FIRST/certificate-base64.txt"
PASSWORD_FILE="$OUTPUT_FIRST/password.txt"

for path in "$P12_FILE" "$BASE64_FILE" "$PASSWORD_FILE"; do
  [ -f "$path" ] || fail "the first bootstrap run must write $(basename "$path") in the output directory"
  [ -s "$path" ] || fail "$(basename "$path") must not be empty"
done
ok "p12, base64 export and password file written"

require_mode_0600 "$P12_FILE"
require_mode_0600 "$BASE64_FILE"
require_mode_0600 "$PASSWORD_FILE"
ok "signing material files are mode 0600"

CERT_SHA1="$(keychain_certificate_sha1)"
[ -n "$CERT_SHA1" ] \
  || fail "the first bootstrap run must create the '$CERT_NAME' certificate in the target keychain $PRIMARY_KEYCHAIN"
ok "certificate '$CERT_NAME' created in the target keychain (SHA-1 $CERT_SHA1)"

# `certificate-base64.txt` is the value of the APPLE_CERTIFICATE secret, i.e.
# the base64 encoding of the p12 that CI decodes into its keychain.
base64 -D -i "$BASE64_FILE" -o "$WORK_DIR/decoded.p12" </dev/null >/dev/null 2>&1 \
  || fail "certificate-base64.txt must be valid base64"
cmp -s "$WORK_DIR/decoded.p12" "$P12_FILE" \
  || fail "certificate-base64.txt must be the base64 encoding of onespace-release-signing.p12"
ok "certificate-base64.txt is the base64 encoding of the p12"

# `password.txt` is the value of the APPLE_CERTIFICATE_PASSWORD secret, i.e.
# the export password `security import -P` needs for that p12.
P12_PASSWORD="$(cat "$PASSWORD_FILE")"
[ -n "$P12_PASSWORD" ] || fail "password.txt must contain the p12 export password"
SECONDARY_KEYCHAIN="$WORK_DIR/import-check.keychain-db"
security create-keychain -p "$KEYCHAIN_PASSWORD" "$SECONDARY_KEYCHAIN" </dev/null >/dev/null 2>&1 \
  || fail "could not create the second throwaway keychain"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$SECONDARY_KEYCHAIN" </dev/null >/dev/null 2>&1 \
  || fail "could not unlock the second throwaway keychain"
if ! IMPORT_OUTPUT="$(security import "$P12_FILE" -k "$SECONDARY_KEYCHAIN" -P "$P12_PASSWORD" \
    -A </dev/null 2>&1)"; then
  fail "password.txt must be the p12 export password, but importing the p12 with it failed: $IMPORT_OUTPUT"
fi
ok "the p12 imports with the password from password.txt"

# ---------------------------------------------------------------------------
# 2. Bootstrap, second run without --force: refuses and creates nothing new
# ---------------------------------------------------------------------------

cp "$P12_FILE" "$WORK_DIR/first-p12.snapshot" \
  || fail "could not snapshot the first bootstrap export"
cp "$PASSWORD_FILE" "$WORK_DIR/first-password.snapshot" \
  || fail "could not snapshot the first bootstrap password file"

OUTPUT_SECOND="$WORK_DIR/output-second"
if BOOTSTRAP_SECOND_OUTPUT="$(bash "$CREATE_SCRIPT" \
    --keychain "$PRIMARY_KEYCHAIN" --output-dir "$OUTPUT_SECOND" </dev/null 2>&1)"; then
  fail "the second bootstrap run without --force must refuse while '$CERT_NAME' exists, but it exited zero: $BOOTSTRAP_SECOND_OUTPUT"
fi
ok "second bootstrap run refused with a non-zero exit status"

if [ -e "$OUTPUT_SECOND" ] && [ -n "$(ls -A "$OUTPUT_SECOND" 2>/dev/null)" ]; then
  fail "the refused bootstrap run must create nothing new, but $OUTPUT_SECOND is not empty"
fi
cmp -s "$P12_FILE" "$WORK_DIR/first-p12.snapshot" \
  || fail "the refused bootstrap run must not replace onespace-release-signing.p12"
cmp -s "$PASSWORD_FILE" "$WORK_DIR/first-password.snapshot" \
  || fail "the refused bootstrap run must not replace password.txt"
[ "$(keychain_certificate_count)" = "1" ] \
  || fail "the refused bootstrap run must not add another '$CERT_NAME' certificate to the target keychain"
ok "the refused run created nothing new and changed no existing material"

if ! printf '%s\n' "$BOOTSTRAP_SECOND_OUTPUT" | grep -E -i -q 'invalidat'; then
  fail "the refusal must explain that regeneration invalidates existing user state, got: $BOOTSTRAP_SECOND_OUTPUT"
fi
if ! printf '%s\n' "$BOOTSTRAP_SECOND_OUTPUT" | grep -E -i -q 'grant|permission'; then
  fail "the refusal must mention the existing permission grants it would invalidate, got: $BOOTSTRAP_SECOND_OUTPUT"
fi
ok "the refusal explains that regeneration would invalidate every user's existing grants"

# ---------------------------------------------------------------------------
# 3. Bootstrap, third run with --force: succeeds
# ---------------------------------------------------------------------------

OUTPUT_FORCED="$WORK_DIR/output-forced"
if ! BOOTSTRAP_FORCED_OUTPUT="$(bash "$CREATE_SCRIPT" \
    --keychain "$PRIMARY_KEYCHAIN" --output-dir "$OUTPUT_FORCED" --force </dev/null 2>&1)"; then
  fail "the bootstrap run with --force must succeed, but it exited non-zero: $BOOTSTRAP_FORCED_OUTPUT"
fi
for name in onespace-release-signing.p12 certificate-base64.txt password.txt; do
  [ -s "$OUTPUT_FORCED/$name" ] || fail "--force must rewrite $name in the output directory"
  require_mode_0600 "$OUTPUT_FORCED/$name"
done
ok "bootstrap run with --force rewrote the signing material"

[ "$(keychain_certificate_count)" = "1" ] \
  || fail "after --force the target keychain must still hold exactly one '$CERT_NAME' certificate, found $(keychain_certificate_count)"
CERT_SHA1="$(keychain_certificate_sha1)"
[ -n "$CERT_SHA1" ] || fail "after --force the '$CERT_NAME' certificate must exist in the target keychain"
ok "the target keychain holds exactly one '$CERT_NAME' certificate (SHA-1 $CERT_SHA1)"

# ---------------------------------------------------------------------------
# 4. A signed bundle verifies and reports a certificate-anchored requirement
# ---------------------------------------------------------------------------

APP_ONE="$WORK_DIR/Test.app"
make_bundle "$APP_ONE" "first-bundle-payload" || fail "could not synthesize $APP_ONE"
sign_bundle_with_certificate "$APP_ONE" "Test.app fixture"
ok "Test.app signed with '$CERT_NAME', --options runtime and src-tauri/Entitlements.plist"

run_verification "$APP_ONE"
[ "$VERIFY_STATUS" -eq 0 ] \
  || fail "verify_release_artifact.sh must accept the certificate-signed Bundle (exit $VERIFY_STATUS): $VERIFY_OUTPUT"

REQUIREMENT_ONE="$(requirement_line "$VERIFY_OUTPUT")"
[ -n "$REQUIREMENT_ONE" ] \
  || fail "verify_release_artifact.sh must print a certificate-anchored designated requirement line: $VERIFY_OUTPUT"
if printf '%s' "$REQUIREMENT_ONE" | grep -E -i -q 'cdhash'; then
  fail "the designated requirement must not be cdhash-anchored: $REQUIREMENT_ONE"
fi
[ "$(requirement_hash "$REQUIREMENT_ONE")" = "$CERT_SHA1" ] \
  || fail "the designated requirement must be anchored to the signing certificate $CERT_SHA1, got: $REQUIREMENT_ONE"
ok "verification passed with requirement '$REQUIREMENT_ONE'"

# ---------------------------------------------------------------------------
# 5. Different content, same certificate: byte-identical requirement
# ---------------------------------------------------------------------------

APP_TWO="$WORK_DIR/Test2.app"
make_bundle "$APP_TWO" "second-bundle-payload-different" || fail "could not synthesize $APP_TWO"
cmp -s "$APP_ONE/Contents/MacOS/Test" "$APP_TWO/Contents/MacOS/Test" \
  && fail "the second bundle fixture must have different content from the first"
sign_bundle_with_certificate "$APP_TWO" "Test2.app fixture"

CDHASH_ONE="$(signed_cdhash "$APP_ONE")"
CDHASH_TWO="$(signed_cdhash "$APP_TWO")"
[ -n "$CDHASH_ONE" ] && [ -n "$CDHASH_TWO" ] \
  || fail "could not read the signed code directory hashes of the two bundles"
[ "$CDHASH_ONE" != "$CDHASH_TWO" ] \
  || fail "the two bundles must really carry different content, but their signed hashes are identical: $CDHASH_ONE"
ok "the two bundles carry different content ($CDHASH_ONE vs $CDHASH_TWO)"

run_verification "$APP_TWO"
[ "$VERIFY_STATUS" -eq 0 ] \
  || fail "verify_release_artifact.sh must accept the second certificate-signed bundle (exit $VERIFY_STATUS): $VERIFY_OUTPUT"

REQUIREMENT_TWO="$(requirement_line "$VERIFY_OUTPUT")"
[ -n "$REQUIREMENT_TWO" ] \
  || fail "verify_release_artifact.sh must print a certificate-anchored designated requirement line for the second bundle: $VERIFY_OUTPUT"
[ "$(requirement_hash "$REQUIREMENT_TWO")" = "$CERT_SHA1" ] \
  || fail "the second bundle must be anchored to the same certificate $CERT_SHA1, got: $REQUIREMENT_TWO"
[ "$REQUIREMENT_ONE" = "$REQUIREMENT_TWO" ] \
  || fail "the designated requirement must be byte-identical across builds: '$REQUIREMENT_ONE' vs '$REQUIREMENT_TWO'"
ok "both builds report the same byte-identical requirement line"

# ---------------------------------------------------------------------------
# 6. Error boundary: an ad-hoc signed bundle must fail verification
# ---------------------------------------------------------------------------

APP_ADHOC="$WORK_DIR/TestAdhoc.app"
make_bundle "$APP_ADHOC" "adhoc-bundle-payload" || fail "could not synthesize $APP_ADHOC"
adhoc_sign_bundle "$APP_ADHOC" "ad-hoc"
ok "ad-hoc bundle fixture signed"

run_verification "$APP_ADHOC"
[ "$VERIFY_STATUS" -ne 0 ] \
  || fail "verify_release_artifact.sh must reject an ad-hoc signed bundle: $VERIFY_OUTPUT"
ok "an ad-hoc signed bundle is rejected (exit $VERIFY_STATUS)"

# ---------------------------------------------------------------------------
# 7. Error boundary: a nonexistent target must fail verification
# ---------------------------------------------------------------------------

MISSING_APP="$WORK_DIR/definitely-missing.app"
run_verification "$MISSING_APP"
[ "$VERIFY_STATUS" -ne 0 ] \
  || fail "verify_release_artifact.sh must fail for the nonexistent path $MISSING_APP instead of passing silently: $VERIFY_OUTPUT"
ok "a nonexistent target path is rejected (exit $VERIFY_STATUS)"

printf 'PASS - signing_tools.test.sh: all behavior checks passed\n'
