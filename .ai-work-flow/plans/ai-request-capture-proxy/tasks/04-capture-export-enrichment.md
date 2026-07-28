# 04 - AI 元数据、HAR 与 cURL

## Goal

在已完成的抓包记录上交付最佳努力的供应商/model/token 解析、文本/Base64 正文表示、过滤后的 HAR 1.2 导出和可安全执行的 POSIX cURL 生成，同时保持敏感值和不完整性提示。

## Dependencies

03 - 流式转发与抓取保真

## Status

ready-for-agent

## Acceptance Criteria

- [ ] OpenAI、Anthropic、Gemini 的普通 JSON 与流式样本可提取 provider、model 和 input/output/total token；unknown、无效 JSON 和截断样本不影响记录完成。
- [ ] UTF-8 文本按文本返回，非文本或无效 UTF-8 按 Base64 返回并携带编码标记；SQLite 继续保存原始 BLOB。
- [ ] HAR 1.2 仅导出当前过滤条件命中的已结束记录，包含标准字段、真实敏感 headers/query/body 和 `_onespace`/comment 完整性元数据。
- [ ] cURL 使用该次实际上游 URL、真实端到端 headers 和 method，排除 hop-by-hop/Host/Content-Length，并正确转义 POSIX 单引号。
- [ ] 二进制正文通过 `printf '%b'` 八进制字节流传给 `curl --data-binary @-`；无正文不添加 data 参数。
- [ ] 截断或传输失败的 cURL 返回 `complete = false`、明确 warning，且复制文本首行可加入警告注释。

## Verification

```bash
cargo test --manifest-path src-tauri/Cargo.toml ai_request_capture::tests::export_enrichment -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml ai_request_capture::tests::streaming_fidelity -- --nocapture
git diff --check
```
