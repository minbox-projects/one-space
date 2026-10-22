#!/usr/bin/env bash
#
# Cargo target runner for local OneSpace debug builds.
#
# Cargo invokes this as: dev-codesign-runner.sh <binary> [app args...]
# Non-OneSpace binaries run unchanged. OneSpace is re-signed with the local
# development identity so its designated requirement stays certificate-anchored
# across rebuilds, then executed with its original arguments.

set -euo pipefail

usage() {
  echo "usage: $(basename "$0") <binary> [app args...]" >&2
}

if [ "$#" -lt 1 ]; then
  usage
  exit 2
fi

binary="$1"
shift

identity="${ONESPACE_DEV_SIGNING_IDENTITY:-OneSpace Dev Signing}"
keychain="${ONESPACE_DEV_SIGNING_KEYCHAIN:-}"

if [ "$(basename "$binary")" != "OneSpace" ]; then
  # macOS launch constraints kill copies of Apple platform binaries. Ad-hoc
  # signing needs no certificate and makes such a copy executable again;
  # ordinary build artifacts are executed unchanged.
  if binary_info="$(codesign -dvv "$binary" 2>&1)"; then
    case "$binary_info" in
      *"Platform identifier="*)
        codesign --force --sign - "$binary" >/dev/null
        ;;
    esac
  fi
  exec "$binary" "$@"
fi

find_cert_args=(-c "$identity")
if [ -n "$keychain" ]; then
  find_cert_args+=("$keychain")
fi

if ! security find-certificate "${find_cert_args[@]}" >/dev/null 2>&1; then
  cat >&2 <<EOF
error: the code-signing certificate "$identity" was not found.

Run the one-time setup first:
  bash src-tauri/scripts/setup-dev-codesign.sh
(from the repository root), or:
  bash scripts/setup-dev-codesign.sh
(from src-tauri). Then run "npm run tauri dev" again and choose "Always Allow"
once when macOS asks for keychain access.
EOF
  exit 1
fi

codesign_args=(--force --sign "$identity" --identifier com.onespace.desktop.dev)
if [ -n "$keychain" ]; then
  codesign_args+=(--keychain "$keychain")
fi

echo "Signing $(basename "$binary") with \"$identity\" for local development." >&2
codesign "${codesign_args[@]}" "$binary"

exec "$binary" "$@"
