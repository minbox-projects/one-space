#!/usr/bin/env bash
#
# OneSpace macOS release signing certificate bootstrap.
#
# Creates the dedicated self-signed code-signing certificate that gives every
# macOS OneSpace release a stable signing identity, so that the app's keychain
# entries and its Bluetooth and Accessibility privacy grants survive updates.
#
# It generates an RSA 2048 self-signed certificate (CN "OneSpace Release
# Signing", critical digitalSignature key usage, codeSigning extended key usage,
# ten-year validity), imports it into the target keychain so `codesign` can use
# it non-interactively, and exports the material needed for the GitHub Actions
# secrets into the output directory:
#
#   * onespace-release-signing.p12  -- APPLE_CERTIFICATE (base64-encoded)
#   * certificate-base64.txt        -- the base64 encoding of the p12
#   * password.txt                  -- APPLE_CERTIFICATE_PASSWORD
#
# The certificate identity is the app's permission anchor: replacing it makes
# macOS treat the app as a different signer, so it must be created exactly once
# and backed up. The script therefore refuses to run again while the certificate
# exists in the target keychain unless --force is given.
#
# It never reads stdin, never touches the network, and defaults to the user's
# default (login) keychain and $HOME/onespace-release-signing.
#
# Usage:
#   create_signing_certificate.sh [--name NAME] [--keychain PATH]
#                                 [--output-dir DIR] [--force]

set -euo pipefail

DEFAULT_NAME="OneSpace Release Signing"
DEFAULT_OUTPUT_DIR="$HOME/onespace-release-signing"

name="$DEFAULT_NAME"
keychain=""
output_dir="$DEFAULT_OUTPUT_DIR"
force=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Create the OneSpace macOS release signing certificate exactly once.

Options:
  --name NAME         Certificate common name (default: "$DEFAULT_NAME")
  --keychain PATH     Target keychain (default: the login keychain)
  --output-dir DIR    Where to write the p12/base64/password (default: "$DEFAULT_OUTPUT_DIR")
  --force             Replace an existing certificate with the same name.
                      This invalidates every existing user's permission grants.
  -h, --help          Show this help
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
    --output-dir)
      shift
      output_dir="${1:-}"
      [ -n "$output_dir" ] || { echo "error: --output-dir requires a value" >&2; exit 2; }
      shift
      ;;
    --force)
      force=1
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
  keychain="$(security default-keychain -d user 2>/dev/null | tr -d '"')"
  if [ -z "$keychain" ]; then
    keychain="$HOME/Library/Keychains/login.keychain-db"
  fi
fi

if [ ! -e "$keychain" ]; then
  echo "error: target keychain does not exist: $keychain" >&2
  exit 1
fi

certificate_exists() {
  security find-certificate -c "$name" "$keychain" >/dev/null 2>&1
}

if certificate_exists && [ "$force" -ne 1 ]; then
  cat >&2 <<EOF
Refusing to regenerate the code-signing certificate "$name": it already exists
in $keychain.

The certificate is the app's stable signing identity. Regenerating it makes
macOS treat OneSpace as a different signer and invalidates every user's existing
permission grants: after their next update, keychain access, Bluetooth and
Accessibility permissions must all be granted again, for every user.

If you truly intend to replace it, re-run with --force. That has the same
consequence for every existing user, so back up the current material first.
EOF
  exit 1
fi

# Prefer the system LibreSSL: its PKCS#12 default encryption is readable by
# `security import`, whereas OpenSSL 3+ defaults to algorithms macOS rejects.
OPENSSL_BIN="/usr/bin/openssl"
if [ ! -x "$OPENSSL_BIN" ]; then
  OPENSSL_BIN="openssl"
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/onespace-release-signing.XXXXXX")"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

cat > "$tmp_dir/openssl.cnf" <<EOF
[ req ]
distinguished_name = onespace_dn
prompt = no
x509_extensions = onespace_signing_ext

[ onespace_dn ]
CN = $name

[ onespace_signing_ext ]
keyUsage = critical, digitalSignature
extendedKeyUsage = codeSigning
basicConstraints = critical, CA:FALSE
EOF

"$OPENSSL_BIN" req -x509 -newkey rsa:2048 -nodes \
  -keyout "$tmp_dir/key.pem" -out "$tmp_dir/cert.pem" \
  -days 3650 -config "$tmp_dir/openssl.cnf" -extensions onespace_signing_ext \
  >/dev/null 2>&1

p12_password="$("$OPENSSL_BIN" rand -base64 27 | tr -d '\n')"

"$OPENSSL_BIN" pkcs12 -export \
  -inkey "$tmp_dir/key.pem" -in "$tmp_dir/cert.pem" \
  -out "$tmp_dir/onespace-release-signing.p12" \
  -passout "pass:$p12_password" >/dev/null 2>&1

# After an explicit --force, drop every prior certificate with this name first so
# the keychain ends up with exactly one identity for it.
if certificate_exists; then
  while read -r sha1; do
    [ -n "$sha1" ] && security delete-certificate -Z "$sha1" "$keychain" >/dev/null 2>&1 || true
  done < <(security find-certificate -a -c "$name" -Z "$keychain" 2>/dev/null \
    | sed -n 's/^SHA-1 hash: //p')
fi

# -A grants every application access to the imported private key, which is what
# lets a non-interactive `codesign` resolve the identity without a GUI prompt;
# a -T restriction to codesign alone was not sufficient.
if ! security import "$tmp_dir/onespace-release-signing.p12" \
    -k "$keychain" -P "$p12_password" -A >/dev/null 2>&1; then
  echo "error: could not import the generated certificate into $keychain" >&2
  exit 1
fi

umask 077
mkdir -p "$output_dir"

p12_output="$output_dir/onespace-release-signing.p12"
base64_output="$output_dir/certificate-base64.txt"
password_output="$output_dir/password.txt"

cp "$tmp_dir/onespace-release-signing.p12" "$p12_output"
chmod 600 "$p12_output"
base64 -i "$p12_output" -o "$base64_output"
chmod 600 "$base64_output"
printf '%s\n' "$p12_password" > "$password_output"
chmod 600 "$password_output"

cat <<EOF
Created the code-signing certificate "$name" in $keychain.

Signing material written to $output_dir (mode 0600):
  $(basename "$p12_output")       -> base64-encode it for APPLE_CERTIFICATE
  $(basename "$base64_output")    -> the base64 value for APPLE_CERTIFICATE
  $(basename "$password_output")  -> the value for APPLE_CERTIFICATE_PASSWORD

Next steps:
  1. Back up $output_dir off the repository; the certificate cannot be
     regenerated without invalidating every user's permission grants.
  2. Set the GitHub repository secrets from this directory:
       APPLE_CERTIFICATE          = contents of $(basename "$base64_output")
       APPLE_CERTIFICATE_PASSWORD = contents of $(basename "$password_output")
       APPLE_SIGNING_IDENTITY     = $name
  3. Publish the next release. Existing users grant keychain, Bluetooth and
     Accessibility once on the first signed release; later updates keep them.
EOF
