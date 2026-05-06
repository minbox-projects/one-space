#!/usr/bin/env bash
set -euo pipefail

# Regression test for check-cli-matrix.sh
# Verifies OpenCode npm fallback when GitHub latest release fails.

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

# Write fake curl that simulates different responses based on URL
cat > "$tmpdir/curl" << 'CURL_EOF'
#!/usr/bin/env bash
# Find the URL argument (first arg starting with http)
url=""
for arg in "$@"; do
  if [[ "$arg" == http* ]]; then
    url="$arg"
    break
  fi
done

case "$url" in
  *api.github.com/repos/anomalyco/opencode/releases/latest*)
    # Simulate GitHub failure: curl non-zero exit
    exit 22
    ;;
  *registry.npmjs.org/opencode-ai/latest*)
    # Simulate npm success for OpenCode
    echo '{"version":"1.3.0"}'
    exit 0
    ;;
  *downloads.claude.ai/claude-code-releases/latest*)
    echo 'Claude CLI 1.0.0'
    exit 0
    ;;
  *registry.npmjs.org/@openai%2Fcodex/latest*)
    echo '{"version":"1.0.0"}'
    exit 0
    ;;
  *registry.npmjs.org/@google%2Fgemini-cli/latest*)
    echo '{"version":"1.0.0"}'
    exit 0
    ;;
  *registry.npmjs.org/@anthropic-ai%2Fclaude-code/latest*)
    echo '{"version":"1.0.0"}'
    exit 0
    ;;
  *)
    echo ""
    exit 0
    ;;
esac
CURL_EOF
chmod +x "$tmpdir/curl"

# Write fake CLI tools with help output for permission checks
cat > "$tmpdir/claude" << 'FAKE_EOF'
#!/usr/bin/env bash
if [[ "${1:-}" == "--help" ]]; then
  echo "Usage: claude [options]"
  echo "  --version                 Show version"
  echo "  --dangerously-skip-permissions  Skip permission checks"
fi
echo "claude 1.0.0"
FAKE_EOF
chmod +x "$tmpdir/claude"

cat > "$tmpdir/codex" << 'FAKE_EOF'
#!/usr/bin/env bash
if [[ "${1:-}" == "--help" ]]; then
  echo "Usage: codex [options]"
  echo "  --version                          Show version"
  echo "  --dangerously-bypass-approvals-and-sandbox  Bypass approvals"
fi
echo "codex 1.0.0"
FAKE_EOF
chmod +x "$tmpdir/codex"

cat > "$tmpdir/gemini" << 'FAKE_EOF'
#!/usr/bin/env bash
if [[ "${1:-}" == "--help" ]]; then
  echo "Usage: gemini [options]"
  echo "  --version           Show version"
  echo "  --approval-mode     Set approval mode"
fi
echo "gemini 1.0.0"
FAKE_EOF
chmod +x "$tmpdir/gemini"

cat > "$tmpdir/opencode" << 'FAKE_EOF'
#!/usr/bin/env bash
if [[ "\${1:-}" == "--help" ]]; then
  echo "Usage: opencode [options]"
  echo "  --version    Show version"
fi
echo "opencode 1.2.0"
FAKE_EOF
chmod +x "$tmpdir/opencode"

# Run the script and capture output + exit code
output="$(PATH="$tmpdir:$PATH" bash tools/check-cli-matrix.sh 2>&1)" && status=0 || status=$?

echo "=== Test Output ==="
echo "$output"
echo "==================="
echo "Exit code: $status"

# Assertions
pass=true

if echo "$output" | grep -qF '[opencode] local=1.2.0 latest=1.3.0 source=npm_registry'; then
  echo "PASS: OpenCode npm fallback version detected"
else
  echo "FAIL: Expected '[opencode] local=1.2.0 latest=1.3.0 source=npm_registry' in output"
  pass=false
fi

if echo "$output" | grep -qF '[ERROR] opencode latest version fetch failed'; then
  echo "FAIL: Should not report opencode latest version fetch failed"
  pass=false
else
  echo "PASS: No opencode latest fetch error"
fi

if [[ $status -eq 0 ]]; then
  echo "PASS: Script exit code is 0"
else
  echo "FAIL: Expected exit code 0, got $status"
  pass=false
fi

if $pass; then
  echo ""
  echo "=== All assertions passed ==="
  exit 0
else
  echo ""
  echo "=== Some assertions failed ==="
  exit 1
fi
