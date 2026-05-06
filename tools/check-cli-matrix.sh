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
  tag="$(curl -fsSL --max-time 10 -H 'Accept: application/vnd.github+json' -H 'User-Agent: OneSpace CLI Matrix' "https://api.github.com/repos/${repo}/releases/latest" 2>/dev/null \
    | node -e 'const d=require("fs").readFileSync(0,"utf8");try{const j=JSON.parse(d);const t=j.tag_name||"";const m=t.match(/[0-9]+\.[0-9]+\.[0-9]+/);console.log(m?m[0]:"")}catch(e){console.log("")}' 2>/dev/null || true)"
  echo "${tag#v}"
}

latest_opencode() {
  local ver
  ver="$(latest_github "anomalyco/opencode")"
  if [[ -n "$ver" ]]; then
    echo "${ver}|github_release"
    return 0
  fi
  ver="$(latest_npm "opencode-ai")"
  if [[ -n "$ver" ]]; then
    echo "${ver}|npm_registry"
    return 0
  fi
  echo "|github_release/npm_registry"
  return 0
}

version_gt() {
  local l_major l_minor l_patch
  IFS='.' read -r l_major l_minor l_patch <<< "$1"
  local r_major r_minor r_patch
  IFS='.' read -r r_major r_minor r_patch <<< "$2"
  [[ $((l_major)) -gt $((r_major)) ]] || \
    { [[ $((l_major)) -eq $((r_major)) ]] && [[ $((l_minor)) -gt $((r_minor)) ]]; } || \
    { [[ $((l_major)) -eq $((r_major)) ]] && [[ $((l_minor)) -eq $((r_minor)) ]] && [[ $((l_patch)) -gt $((r_patch)) ]]; }
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
      latest_pair="$(latest_opencode)"
      latest_ver="${latest_pair%%|*}"
      source="${latest_pair#*|}"
      ;;
  esac

  if [[ -z "$latest_ver" ]]; then
    echo "[ERROR] $tool latest version fetch failed (source=$source)"
    failures=$((failures + 1))
    continue
  fi

  echo "[$tool] local=$local_ver latest=$latest_ver source=$source"

  if [[ "$local_ver" != "not_installed" && "$local_ver" != "unknown" ]]; then
    if version_gt "$local_ver" "$latest_ver"; then
      echo "[WARN] $tool local ($local_ver) is ahead of latest ($latest_ver)"
    fi
  fi
done

if [[ "$failures" -gt 0 ]]; then
  echo "[cli-matrix] $failures error(s) encountered"
  exit 1
fi

echo ""
echo "[cli-matrix] checking full-access permission parameters..."

check_permission_flag() {
  local cmd="$1"
  local flag="$2"
  local env_var="$3"  # optional

  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "[$cmd] not installed — skip permission flag check"
    return 0
  fi

  local help_output
  help_output="$(run_with_timeout "$cmd" --help 2>&1 || true)"

  if [[ -n "$flag" ]] && echo "$help_output" | grep -qF -- "$flag"; then
    echo "[$cmd] permission flag '$flag' detected"
  elif [[ -n "$flag" ]]; then
    echo "[WARN] [$cmd] permission flag '$flag' NOT found in --help output"
    failures=$((failures + 1))
  fi

  if [[ -n "$env_var" ]]; then
    echo "[$cmd] uses environment variable '$env_var' for permission (not CLI flag)"
  fi
}

check_permission_flag "claude" "--dangerously-skip-permissions" ""
check_permission_flag "gemini" "--approval-mode" ""
check_permission_flag "codex" "--dangerously-bypass-approvals-and-sandbox" ""
check_permission_flag "opencode" "" "OPENCODE_PERMISSION"

if [[ "$failures" -gt 0 ]]; then
  echo "[cli-matrix] $failures permission check error(s) encountered"
  exit 1
fi

echo "[cli-matrix] all checks passed"
