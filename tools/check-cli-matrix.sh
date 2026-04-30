#!/usr/bin/env bash
set -euo pipefail

TOOLS=(
  "claude"
  "codex"
  "gemini"
  "opencode"
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

latest_claude() {
  local ver
  ver="$(curl -fsSL --max-time 10 'https://downloads.claude.ai/claude-code-releases/latest' 2>/dev/null | grep -Eo '[0-9]+\.[0-9]+\.[0-9]+' | head -n1 || true)"
  if [[ -z "$ver" ]]; then
    ver="$(curl -fsSL --max-time 10 'https://registry.npmjs.org/@anthropic-ai%2Fclaude-code/latest' 2>/dev/null | node -e 'const d=require("fs").readFileSync(0,"utf8");const j=JSON.parse(d);console.log(j.version||"")' 2>/dev/null || true)"
  fi
  echo "${ver:-}"
}

latest_npm() {
  local pkg="$1"
  curl -fsSL --max-time 10 "https://registry.npmjs.org/${pkg}/latest" 2>/dev/null \
    | node -e 'const d=require("fs").readFileSync(0,"utf8");const j=JSON.parse(d);console.log(j.version||"")' 2>/dev/null || true
}

latest_github() {
  local repo="$1"
  local tag
  tag="$(curl -fsSL --max-time 10 -H 'Accept: application/vnd.github+json' "https://api.github.com/repos/${repo}/releases/latest" 2>/dev/null \
    | node -e 'const d=require("fs").readFileSync(0,"utf8");const j=JSON.parse(d);console.log(j.tag_name||"")' 2>/dev/null || true)"
  echo "${tag#v}"
}

echo "[cli-matrix] checking CLI tools..."

failures=0
for tool in "${TOOLS[@]}"; do
  local_ver="$(local_version "$tool")"

  latest_ver=""
  source=""
  case "$tool" in
    claude)
      latest_ver="$(latest_claude)"
      source="claude_release/npm_registry"
      ;;
    codex)
      latest_ver="$(latest_npm "@openai%2Fcodex")"
      source="npm_registry"
      ;;
    gemini)
      latest_ver="$(latest_npm "@google%2Fgemini-cli")"
      source="npm_registry"
      ;;
    opencode)
      latest_ver="$(latest_github "anomalyco/opencode")"
      source="github_release"
      ;;
  esac

  if [[ -z "$latest_ver" ]]; then
    echo "[ERROR] $tool latest version fetch failed (source=$source)"
    failures=$((failures + 1))
    continue
  fi

  echo "[$tool] local=$local_ver latest=$latest_ver source=$source"

  if [[ "$local_ver" != "not_installed" && "$local_ver" != "unknown" ]]; then
    # Simple comparison: if local > latest, warn (should not happen unless pre-release)
    local l_major l_minor l_patch
    IFS='.' read -r l_major l_minor l_patch <<< "$local_ver"
    local r_major r_minor r_patch
    IFS='.' read -r r_major r_minor r_patch <<< "$latest_ver"
    if [[ $((l_major)) -gt $((r_major)) ]] || \
       { [[ $((l_major)) -eq $((r_major)) ]] && [[ $((l_minor)) -gt $((r_minor)) ]]; } || \
       { [[ $((l_major)) -eq $((r_major)) ]] && [[ $((l_minor)) -eq $((r_minor)) ]] && [[ $((l_patch)) -gt $((r_patch)) ]]; }; then
      echo "[WARN] $tool local ($local_ver) is ahead of latest ($latest_ver)"
    fi
  fi
done

if [[ "$failures" -gt 0 ]]; then
  echo "[cli-matrix] $failures error(s) encountered"
  exit 1
fi

echo "[cli-matrix] all checks passed"
