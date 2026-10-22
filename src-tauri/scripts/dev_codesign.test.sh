#!/usr/bin/env bash
# dev_codesign.test.sh — behavior test for the macOS-only dev code-signing setup.
# Public boundary covered:
#   1. scripts/setup-dev-codesign.sh  (--name/--keychain, self-signed RSA code-signing
#      certificate with codeSigning extended key usage and ten-year validity, idempotent,
#      never touching the login keychain when --keychain is given)
#   2. scripts/dev-codesign-runner.sh (cargo runner: exec bypass for non-OneSpace
#      binaries, certificate check + failed message when missing, otherwise re-sign
#      with identifier com.onespace.desktop.dev into a stable, certificate-anchored
#      designated requirement and exec the binary)
#   3. ../.cargo/config.toml (runner wired for aarch64-apple-darwin and x86_64-apple-darwin)
set -u
set -o pipefail

fail() {
  printf 'FAIL - dev_codesign.test.sh: %s\n' "$*" >&2
  exit 1
}

ok() {
  printf 'ok - %s\n' "$*"
}

case "$(uname)" in
  Darwin) ;;
  *) fail "this behavior test is macOS-only (uname=$(uname))" ;;
esac

WT="$(cd "$(dirname "$0")/../.." && pwd)"
SETUP="$WT/src-tauri/scripts/setup-dev-codesign.sh"
RUNNER="$WT/src-tauri/scripts/dev-codesign-runner.sh"
CARGO_CFG="$WT/src-tauri/.cargo/config.toml"

# Existence checks come first so an absent implementation fails with a precise reason.
[ -x "$SETUP" ] || fail "missing executable: $SETUP"
[ -f "$RUNNER" ] || fail "missing runner: $RUNNER (cargo invokes bash scripts/dev-codesign-runner.sh)"
[ -f "$CARGO_CFG" ] || fail "missing cargo config: $CARGO_CFG"

WORK="$(mktemp -d)" || fail "mktemp -d failed"
KC="${WORK}/test-dev-codesign.keychain"
KC_PASS="$(openssl rand -hex 16)"
cleanup() {
  if [ -f "$KC" ]; then
    security delete-keychain "$KC" >/dev/null 2>&1
  fi
  rm -rf "$WORK" 2>/dev/null
  return 0
}
trap cleanup EXIT

RUN_ID="$(date +%Y%m%d%H%M%S)-$$"
CERT_NAME="OneSpace TestDev Signing $RUN_ID"

security create-keychain -p "$KC_PASS" "$KC" || fail "security create-keychain failed"
security unlock-keychain -p "$KC_PASS" "$KC" || fail "security unlock-keychain failed"

sec_certs() {
  security find-certificate -a -Z -c "$CERT_NAME" "$KC" >"$1" 2>/dev/null || fail "security find-certificate failed"
  grep -c 'SHA-1 hash' "$1"
}

cert_fp() {
  grep 'SHA-1 hash' "$1" | grep -Eo '[0-9A-Fa-f]{40}'
}

# --- setup-dev-codesign.sh run 1: creates the certificate in the target keychain ---
bash "$SETUP" --name "$CERT_NAME" --keychain "$KC" >"$WORK/setup1.out" 2>&1 || \
  fail "setup run 1 exited nonzero: $(tail -n 3 "$WORK/setup1.out")"
ok "setup run 1 exit 0"

n1="$(sec_certs "$WORK/certs1.txt" || true)"
[ "$n1" -eq 1 ] || fail "expected exactly 1 certificate named '$CERT_NAME' in target keychain, found $n1"
FP1="$(cert_fp "$WORK/certs1.txt" || true)"
if [ -z "$FP1" ] || [ ${#FP1} -ne 40 ]; then
  fail "could not read a 40-hex SHA-1 for the created certificate"
fi
ok "exactly one certificate after run 1 (SHA-1 $FP1)"

security find-certificate -c "$CERT_NAME" -p "$KC" >"$WORK/cert.pem" 2>/dev/null || \
  fail "could not export PEM of the created certificate"
# Nine years in seconds: a certificate passing this is valid for roughly ten years.
openssl x509 -in "$WORK/cert.pem" -noout -checkend 283968000 || \
  fail "certificate fails the ~ten-year validity check (checkend 9y)"
openssl x509 -in "$WORK/cert.pem" -noout -text | grep -Fq 'Code Signing' || \
  fail "certificate lacks the codeSigning extended key usage"
ok "certificate is codeSigning EKU and ~ten-year valid"

# --- setup-dev-codesign.sh run 2: idempotent, no regeneration ---
bash "$SETUP" --name "$CERT_NAME" --keychain "$KC" >"$WORK/setup2.out" 2>&1 || \
  fail "setup run 2 exited nonzero: $(tail -n 3 "$WORK/setup2.out")"
n2="$(sec_certs "$WORK/certs2.txt" || true)"
[ "$n2" -eq 1 ] || fail "run 2 changed the certificate count in target keychain (found $n2)"
FP2="$(cert_fp "$WORK/certs2.txt" || true)"
if [ -z "$FP2" ] || [ "$FP1" != "$FP2" ]; then
  fail "run 2 regenerated the certificate (SHA-1 $FP1 -> $FP2)"
fi
ok "setup run 2 exit 0 and certificate unchanged"

# --- --keychain must not touch the login keychain (read-only absence probe) ---
if security find-certificate -c "$CERT_NAME" login >/dev/null 2>&1; then
  fail "setup leaked the test certificate into the login keychain despite --keychain"
fi
ok "login keychain has no trace of the test certificate"

# --- executable fixtures named OneSpace ---
[ -x /bin/echo ] || fail "/bin/echo not available"
[ -x /usr/bin/true ] || fail "/usr/bin/true not available"
mkdir -p "$WORK/app1" "$WORK/app2" || fail "mkdir -p failed"
APP_ECHO="${WORK}/app1/OneSpace"
APP_TRUE="${WORK}/app2/OneSpace"
cp /bin/echo "$APP_ECHO" || fail "cp /bin/echo failed"
cp /usr/bin/true "$APP_TRUE" || fail "cp /usr/bin/true failed"

# --- runner: re-sign and exec the echo app with its argument ---
M1="ONESPACE_DEV_RUNNER_ECHO_MARKER_${RUN_ID}"
rc=0
env ONESPACE_DEV_SIGNING_IDENTITY="$CERT_NAME" ONESPACE_DEV_SIGNING_KEYCHAIN="$KC" \
  bash "$RUNNER" "$APP_ECHO" "$M1" >"$WORK/run_echo.out" 2>&1 || rc=$?
if [ "$rc" -ne 0 ]; then
  fail "runner run on echo app exited $rc: $(tail -n 5 "$WORK/run_echo.out")"
fi
grep -Fq "$M1" "$WORK/run_echo.out" || \
  fail "runner did not execute the app with its arguments (marker missing from output)"
ok "runner executed the echo app with its argument"

# --- runner: executes the true app, exit 0 ---
rc=0
env ONESPACE_DEV_SIGNING_IDENTITY="$CERT_NAME" ONESPACE_DEV_SIGNING_KEYCHAIN="$KC" \
  bash "$RUNNER" "$APP_TRUE" >"$WORK/run_true.out" 2>&1 || rc=$?
[ "$rc" -eq 0 ] || fail "runner run on true app exited $rc: $(tail -n 5 "$WORK/run_true.out")"
ok "runner executed the true app with exit 0"

# --- designated requirement: certificate-anchored, stable across binaries ---
req_line() {
  codesign -d -r- "$1" 2>&1 | grep '^designated =>'
}
R1="$(req_line "$APP_ECHO" || true)"
if [ -z "$R1" ]; then
  fail "no designated requirement line from codesign -d -r- for the echo app"
fi
R2="$(req_line "$APP_TRUE" || true)"
if [ -z "$R2" ]; then
  fail "no designated requirement line from codesign -d -r- for the true app"
fi
printf '%s\n' "$R1" | grep -Eq 'certificate leaf = H"[0-9A-Fa-f]{40}"' || \
  fail "designated requirement is not certificate-leaf anchored: $R1"
printf '%s\n' "$R1" | grep -Fq 'com.onespace.desktop.dev' || \
  fail "designated requirement does not carry identifier com.onespace.desktop.dev: $R1"
printf '%s\n' "$R1" | grep -Fqi 'cdhash' && \
  fail "designated requirement is cdhash-anchored: $R1"
printf '%s\n' "$R2" | grep -Fqi 'cdhash' && \
  fail "designated requirement of the second app is cdhash-anchored: $R2"
if [ "$R1" != "$R2" ]; then
  fail "designated requirements differ across two binaries signed by the same certificate: [$R1] vs [$R2]"
fi
ok "designated requirement is certificate-anchored and byte-identical across both fixtures"

codesign --verify --strict "$APP_TRUE" || \
  fail "codesign --verify --strict failed for the signed app fixture"
ok "codesign --verify --strict passes for the signed app fixture"

# --- missing identity: runner exits nonzero, mentions the setup script, no exec ---
MISSING_ID="Onespace Test Missing Identity $RUN_ID"
M2="ONESPACE_DEV_RUNNER_MISSING_PROBE_MARKER_${RUN_ID}"
rc=0
env ONESPACE_DEV_SIGNING_IDENTITY="$MISSING_ID" ONESPACE_DEV_SIGNING_KEYCHAIN="$KC" \
  bash "$RUNNER" "$APP_ECHO" "$M2" >"$WORK/run_missing.out" 2>&1 || rc=$?
[ "$rc" -ne 0 ] || fail "runner exited 0 despite a missing identity"
grep -Fq 'setup-dev-codesign.sh' "$WORK/run_missing.out" || \
  fail "missing-identity message does not mention setup-dev-codesign.sh"
if grep -Fq "$M2" "$WORK/run_missing.out"; then
  fail "runner executed the app despite a missing identity (marker present in output)"
fi
ok "missing identity fails with a setup-dev-codesign.sh hint and no execution"

# --- non-OneSpace binary: exec unchanged without any certificate ---
M3="ONESPACE_DEV_RUNNER_BYPASS_MARKER_${RUN_ID}"
cp /bin/echo "$WORK/OtherTool" || fail "cp /bin/echo for bypass fixture failed"
env -u ONESPACE_DEV_SIGNING_IDENTITY -u ONESPACE_DEV_SIGNING_KEYCHAIN \
  bash "$RUNNER" "$WORK/OtherTool" "$M3" >"$WORK/run_other.out" 2>&1 || \
  fail "non-OneSpace binary did not run unchanged (exited $?)"
grep -Fq "$M3" "$WORK/run_other.out" || \
  fail "non-OneSpace bypass did not pass the marker argument through"
ok "non-OneSpace binary runs unchanged with its argument, no certificate required"

# --- cargo config wires the runner for both darwin targets ---
grep -qF '[target.aarch64-apple-darwin]' "$CARGO_CFG" || \
  fail "cargo config missing [target.aarch64-apple-darwin] runner section"
grep -qF '[target.x86_64-apple-darwin]' "$CARGO_CFG" || \
  fail "cargo config missing [target.x86_64-apple-darwin] runner section"
grep -qF 'bash scripts/dev-codesign-runner.sh' "$CARGO_CFG" || \
  fail "cargo config missing exact runner command bash scripts/dev-codesign-runner.sh"
ok "cargo config declares the runner for both darwin targets"

printf 'PASS - dev_codesign.test.sh\n'
