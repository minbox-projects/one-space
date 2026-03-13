#!/usr/bin/env bash
set -euo pipefail

BASELINE_DATE="2026-03-13"
TOOLS=(
  "claude|@anthropic-ai/claude-code|2.1.74"
  "gemini|@google/gemini-cli|0.33.1"
  "codex|@openai/codex|0.114.0"
  "opencode|opencode-ai|1.2.25"
)

extract_semver() {
  local text="$1"
  echo "$text" | grep -Eo '[0-9]+\.[0-9]+\.[0-9]+' | head -n1 || true
}

run_with_timeout() {
  node -e '
const {spawnSync} = require("child_process");
const args = process.argv.slice(1);
const cmd = args.shift();
const result = spawnSync(cmd, args, {encoding: "utf8", timeout: 8000});
process.stdout.write((result.stdout || "") + (result.stderr || ""));
if (result.error && result.error.code === "ETIMEDOUT") process.exit(124);
process.exit(result.status == null ? 1 : result.status);
' "$@"
}

local_version() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "not_installed"
    return 0
  fi
  local output
  output="$(run_with_timeout "$cmd" --version 2>&1 || true)"
  local semver
  semver="$(extract_semver "$output")"
  if [[ -z "$semver" ]]; then
    echo "unknown"
  else
    echo "$semver"
  fi
}

registry_version() {
  local pkg="$1"
  npm view "$pkg" version --fetch-timeout=7000 --fetch-retries=0 2>/dev/null | tr -d '[:space:]'
}

echo "[cli-matrix] baseline date: $BASELINE_DATE"

failures=0
for entry in "${TOOLS[@]}"; do
  IFS='|' read -r tool package expected <<<"$entry"

  local_ver="$(local_version "$tool")"
  latest_ver="$(registry_version "$package")"

  if [[ -z "$latest_ver" ]]; then
    echo "[ERROR] $tool registry version fetch failed for $package"
    failures=$((failures + 1))
    continue
  fi

  echo "[$tool] expected=$expected latest=$latest_ver local=$local_ver"

  if [[ "$latest_ver" != "$expected" ]]; then
    echo "[ERROR] $tool latest drift: expected=$expected latest=$latest_ver"
    failures=$((failures + 1))
  fi

  if [[ "$local_ver" != "not_installed" && "$local_ver" != "unknown" && "$local_ver" != "$expected" ]]; then
    echo "[ERROR] $tool local drift: expected=$expected local=$local_ver"
    failures=$((failures + 1))
  fi
done

if [[ "$failures" -gt 0 ]]; then
  echo "[cli-matrix] failed with $failures mismatch(es)"
  exit 1
fi

echo "[cli-matrix] all checks passed"
