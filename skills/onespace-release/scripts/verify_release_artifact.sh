#!/usr/bin/env bash
#
# Verify a signed OneSpace macOS application bundle.
#
# It fails unless the bundle at the given path is a validly signed macOS
# application that carries the production bundle identifier and a designated
# requirement anchored to a certificate. An ad-hoc signature, a cdhash-pinned
# requirement, a wrong identifier or a missing path all fail, so a release
# pipeline can use it both locally and after publishing to reject an artifact
# that would break users' keychain, Bluetooth and Accessibility grants.
#
# Usage:
#   verify_release_artifact.sh <path-to-OneSpace.app>

set -euo pipefail

EXPECTED_IDENTIFIER="com.onespace.desktop"

if [ "$#" -ne 1 ]; then
  echo "usage: $(basename "$0") <path-to-OneSpace.app>" >&2
  exit 2
fi

bundle="$1"

if [ ! -e "$bundle" ]; then
  echo "error: bundle not found: $bundle" >&2
  exit 1
fi

verify_output="$(codesign --verify --strict "$bundle" 2>&1)" || {
  echo "error: codesign --verify --strict failed for $bundle" >&2
  printf '%s\n' "$verify_output" >&2
  exit 1
}

display="$(codesign -dvvv "$bundle" 2>&1 || true)"
requirement_output="$(codesign -d -r- "$bundle" 2>&1 || true)"

identifier="$(printf '%s\n' "$display" | sed -n 's/^Identifier=//p' | head -n 1)"
if [ "$identifier" != "$EXPECTED_IDENTIFIER" ]; then
  echo "error: expected identifier '$EXPECTED_IDENTIFIER', got '${identifier:-<none>}'" >&2
  exit 1
fi

if printf '%s\n' "$display" | grep -q '^Signature=adhoc'; then
  echo "error: bundle is ad-hoc signed (Signature=adhoc): $bundle" >&2
  exit 1
fi

requirement_line="$(printf '%s\n' "$requirement_output" | grep -E 'designated =>' | head -n 1)"
if [ -z "$requirement_line" ]; then
  echo "error: could not read the designated requirement of $bundle" >&2
  printf '%s\n' "$requirement_output" >&2
  exit 1
fi

if ! printf '%s\n' "$requirement_line" | grep -E -q 'certificate (root|leaf) = H"[0-9A-Fa-f]{40}"'; then
  echo "error: designated requirement is not certificate-anchored: $requirement_line" >&2
  exit 1
fi

echo "OK: $bundle is validly signed as '$EXPECTED_IDENTIFIER'."
echo "designated requirement: $requirement_line"
printf '%s\n' "$display" | grep -E '^Authority=' || true
