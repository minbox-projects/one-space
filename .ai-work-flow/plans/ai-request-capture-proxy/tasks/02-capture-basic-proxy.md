# 02 - 基础代理纵向闭环

## Goal

交付第一条可用纵向链路：保存 Enabled 配置后可在 `127.0.0.1` 启动、停止和恢复代理，将一次普通敏感 JSON HTTP 请求转发到本地上游，并通过 list/detail 命令查询完整记录。

## Dependencies

01 - 配置、状态与本地存储契约

## Status

ready-for-agent

## Acceptance Criteria

- [ ] 运行时只绑定 `127.0.0.1`，支持幂等 start/stop/restart、oneshot shutdown、完整状态和状态事件。
- [ ] 保存有效 Enabled 配置会持久化期望状态并启动代理；运行时失败保留配置并暴露 `last_error`。
- [ ] 上游 URL 按 Base URL path 前缀与原始入站 path/query 拼接，只替换 origin，并在保存、启动和转发前拒绝当前监听循环。
- [ ] 普通 JSON 请求的方法、正文和敏感端到端 headers 到达本地 mock upstream，普通响应完整返回客户端。
- [ ] 记录从 `in_progress` 进入最终状态，list 不返回正文，detail 返回明文 headers 和正文样本。
- [ ] 数据创建、完成和失败发出 `ai-request-capture-updated`，启停、配置应用和运行错误发出状态事件。

## Verification

```bash
cargo test --manifest-path src-tauri/Cargo.toml ai_request_capture::tests::basic_proxy -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml ai_request_capture::tests::config_storage -- --nocapture
git diff --check
```
