#!/usr/bin/env bash
#
# OneSpace local development code-signing setup.
#
# Creates a self-signed code-signing certificate so that local macOS debug
# builds get a stable signing identity. A stable identity lets keychain access
# and privacy grants survive rebuilds; regenerating the certificate makes macOS
# treat the app as a different signer and invalidates the saved grant.
#
# Usage:
#   setup-dev-codesign.sh [--name NAME] [--keychain PATH]

set -euo pipefail

DEFAULT_NAME="OneSpace Dev Signing"

name="$DEFAULT_NAME"
keychain=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Create the OneSpace local development code-signing certificate if it is absent.

Options:
  --name NAME       Certificate common name (default: "$DEFAULT_NAME")
  --keychain PATH   Target keychain (default: the login keychain)
  -h, --help        Show this help
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --name)
      shift
      name="${1:-}"
      [ -n "$name" ] || { echo "error: --name requires a value" >&2; exit 2; }
      shift
      ;;
    --keychain)
      shift
      keychain="${1:-}"
      [ -n "$keychain" ] || { echo "error: --keychain requires a value" >&2; exit 2; }
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [ -z "$keychain" ]; then
  keychain="$(security default-keychain -d user 2>/dev/null \
    | sed -E 's/^[[:space:]]*//; s/[[:space:]]*$//; s/^"//; s/"$//')"
  if [ -z "$keychain" ]; then
    keychain="$HOME/Library/Keychains/login.keychain-db"
  fi
fi

if [ ! -e "$keychain" ]; then
  echo "error: target keychain does not exist: $keychain" >&2
  exit 1
fi

if security find-certificate -c "$name" "$keychain" >/dev/null 2>&1; then
  echo "The code-signing certificate \"$name\" is already present in $keychain; nothing to do."
  exit 0
fi

# Prefer the system LibreSSL: its PKCS#12 default encryption is readable by
# `security import`, whereas OpenSSL 3+ defaults to algorithms macOS rejects.
OPENSSL_BIN="/usr/bin/openssl"
if [ ! -x "$OPENSSL_BIN" ]; then
  OPENSSL_BIN="openssl"
fi

umask 077
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/onespace-dev-codesign.XXXXXX")"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

cat > "$tmp_dir/openssl.cnf" <<EOF
[ req ]
distinguished_name = onespace_dev_dn
prompt = no
x509_extensions = onespace_dev_signing_ext

[ onespace_dev_dn ]
CN = $name

[ onespace_dev_signing_ext ]
keyUsage = critical, digitalSignature
extendedKeyUsage = codeSigning
basicConstraints = critical, CA:FALSE
EOF

"$OPENSSL_BIN" req -x509 -newkey rsa:2048 -nodes \
  -keyout "$tmp_dir/key.pem" -out "$tmp_dir/cert.pem" \
  -days 3650 -config "$tmp_dir/openssl.cnf" -extensions onespace_dev_signing_ext \
  >/dev/null 2>&1

p12_password="$("$OPENSSL_BIN" rand -base64 27 | tr -d '\n')"

"$OPENSSL_BIN" pkcs12 -export \
  -inkey "$tmp_dir/key.pem" -in "$tmp_dir/cert.pem" \
  -out "$tmp_dir/identity.p12" \
  -passout "pass:$p12_password" >/dev/null 2>&1

# -A grants every application access to the imported private key, which is what
# lets a non-interactive `codesign` resolve the identity without a GUI prompt.
if ! security import "$tmp_dir/identity.p12" \
    -k "$keychain" -P "$p12_password" -A >/dev/null 2>&1; then
  echo "error: could not import the generated certificate into $keychain" >&2
  exit 1
fi

cat <<EOF
Created the code-signing certificate "$name" in $keychain.

Next steps:
  1. Run: npm run tauri dev
  2. When macOS asks for keychain access, choose "Always Allow" once. The
     saved grant lets later dev builds sign without prompting.

Do not regenerate this certificate to "fix" a prompt: a new certificate is a
different signer, so macOS invalidates the saved grant and you must allow it
again.
EOF
