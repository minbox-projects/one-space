# 01 - 配置、状态与本地存储契约

## Goal

交付可独立测试的 AI 请求抓包后端基础：本机配置持久化与校验、运行状态和十个 Tauri 命令契约、独立 SQLite schema/CRUD/筛选分页/清理，以及不会阻塞应用启动的自动恢复入口；本任务不启动真实代理监听。

## Dependencies

None - can start immediately

## Status

ready-for-agent

## Acceptance Criteria

- [ ] 默认配置为 `enabled = false`、`port = 17688`、空上游，监听地址固定为 `127.0.0.1` 且不可配置。
- [ ] 配置校验覆盖端口、HTTP/HTTPS、host、path、query、fragment 和指向当前监听端口的 loopback 循环目标；有效配置原子写入本机应用目录。
- [ ] 抓包数据库位于本机应用目录，使用 `PRAGMA user_version`、WAL/busy timeout、主表和计划要求的索引，不接入共享存储、`app_store` 或 Protocol Router。
- [ ] SQLite 支持开始/完成记录、明文多值 headers、BLOB body、详情、稳定筛选分页、清空、遗留 `in_progress` 中断恢复和 7 天清理。
- [ ] 十个命令具有可编译的类型和薄命令入口；自动恢复失败只更新 `last_error` 和状态，不阻止应用启动。
- [ ] 配置与存储测试使用临时目录或内存数据库，不绑定真实端口。

## Verification

```bash
cargo test --manifest-path src-tauri/Cargo.toml ai_request_capture::tests::config_storage -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git diff --check
```
