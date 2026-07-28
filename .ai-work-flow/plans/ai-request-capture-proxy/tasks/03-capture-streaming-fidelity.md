# 03 - 流式转发与抓取保真

## Goal

扩展基础代理，使 chunked 请求、普通/chunked/SSE 响应、常规 HTTP method、大正文和传输错误在不被持久化阻塞的情况下完整转发，并准确记录截断、字节统计和终态。

## Dependencies

02 - 基础代理纵向闭环

## Status

ready-for-agent

## Acceptance Criteria

- [ ] 请求与响应均使用流式 tee；SSE 首个 chunk 在上游结束和数据库收尾前到达客户端。
- [ ] 每方向只捕获前 2 MiB，但超过上限的网络内容仍完整转发，并准确记录 `captured_bytes`、`total_bytes` 和 `truncated`。
- [ ] 支持除 `CONNECT` 外的常规 method、无正文、Content-Length、chunked、HEAD 和上游 4xx/5xx。
- [ ] 转发副本移除标准及 Connection 声明的 hop-by-hop headers、Host 和旧 Content-Length，并设置 `Accept-Encoding: identity`；清理前的原始 headers 明文入库。
- [ ] 明确拒绝 CONNECT 和 WebSocket Upgrade；上游连接失败、请求/响应传输失败、客户端断开和响应已开始后的失败映射到约定状态。
- [ ] 所有端到端测试只使用本地受控 mock upstream，不访问公网或使用真实凭证。

## Verification

```bash
cargo test --manifest-path src-tauri/Cargo.toml ai_request_capture::tests::streaming_fidelity -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml ai_request_capture::tests::basic_proxy -- --nocapture
git diff --check
```
