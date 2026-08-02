#!/usr/bin/env bash
set -euo pipefail

root=$(git rev-parse --show-toplevel)
cd "$root"

files=0
private_key_matches=0
header_value_matches=0
credential_literal_matches=0

while IFS= read -r -d '' path; do
  case "$path" in
    node_modules/*|dist/*|src-tauri/target/*)
      continue
      ;;
  esac

  read -r private_keys header_values credential_literals < <(
    perl -0777 -e '
      local $/;
      my $data = <>;
      if (!defined($data) || index($data, "\0") >= 0) {
        print "0 0 0\n";
        exit;
      }

      my $private_keys = () = $data =~ /-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----/g;
      my $header_values = 0;
      while ($data =~ /["\x27]?(?:Authorization|Cookie)["\x27]?\s*:\s*["\x27]?\s*(?:Bearer\s+)?([A-Za-z0-9._~+\/-]{16,})/ig) {
        $header_values++ unless $1 =~ /SAFE_FIXTURE_|REDACTED|EXAMPLE|PLACEHOLDER|TEST|FIXTURE|SAMPLE|DUMMY|MOCK|FAKE|LOCALHOST/i;
      }
      my $credential_literals = 0;
      while ($data =~ /(?:api[_-]?key|access[_-]?token|refresh[_-]?token|client[_-]?secret|password)\s*[:=]\s*["\x27]([A-Za-z0-9._~+\/-]{16,})["\x27]/ig) {
        $credential_literals++ unless $1 =~ /SAFE_FIXTURE_|REDACTED|EXAMPLE|PLACEHOLDER|TEST|FIXTURE|SAMPLE|DUMMY|MOCK|FAKE|LOCALHOST|SYSTEM-GEMINI-KEY|\$\{/i;
      }
      print "$private_keys $header_values $credential_literals\n";
    ' "$path"
  )

  files=$((files + 1))
  private_key_matches=$((private_key_matches + private_keys))
  header_value_matches=$((header_value_matches + header_values))
  credential_literal_matches=$((credential_literal_matches + credential_literals))
done < <(git ls-files -co --exclude-standard -z -- .)

total=$((private_key_matches + header_value_matches + credential_literal_matches))
printf '脱敏扫描范围=当前 worktree 全部已跟踪及未被忽略的文本文件；排除构建产物 node_modules、dist、src-tauri/target\n'
printf '敏感模式=私钥 PEM；Authorization/Cookie 字面量值；api_key/access_token/refresh_token/client_secret/password 字面量值\n'
printf '边界=静态扫描不读取运行时正文；日志、错误、fixture、SQLite 边界由 Rust 脱敏测试覆盖；允许 SAFE_FIXTURE_* 与既有 system-gemini-key 测试 fixture\n'
printf '结果=%s；文件数=%d；私钥=%d；头部值=%d；凭据字面量=%d\n' \
  "$([[ "$total" -eq 0 ]] && printf PASS || printf FAIL)" \
  "$files" "$private_key_matches" "$header_value_matches" "$credential_literal_matches"

if [[ "$total" -ne 0 ]]; then
  exit 1
fi
