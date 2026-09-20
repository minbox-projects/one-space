# Agent Note: OpenCode 会话存储兼容性

Status: implemented

[English](2026-09-20-opencode-session-storage-compatibility.md) | 中文

## Problem

OpenCode 在 v1.2.0 将会话存储从 legacy JSON 迁移到 SQLite，正式 v2 格式又重命名了 SQLite 表。迁移会保留 session ID，也可能保留旧存储，因此只读取一种格式会丢失历史，而独立聚合所有格式会重复计算同一个逻辑 session。

## Decision

- 以完整 SemVer 解析 OpenCode CLI 版本：提取包含 prerelease 与 build metadata 的 `1.x`、`2.x` 版本，并按 SemVer 优先级比较安装副本——prerelease 的纯数字标识优先级低于字母数字标识、stable 高于 prerelease，build metadata 保留在报告文本中但不参与优先级比较。
- 从 legacy JSON、SQLite v1 的 `session` 与 `message` 表，以及 SQLite v2 的 `session_v2` 与 `session_message` 表读取历史和用量。
- 以 trim 后的 ID 规范化 session 身份。相同 ID 出现在多个存储时，按固定优先级 v2 > v1 > JSON 只选择一个来源；低优先级来源只补充高优先级来源中不存在的 session ID。
- 用量保留选中来源内的全部 message，并从 message 级数据取得 token。这样既兼容早期 v1 记录，也匹配 v2，同时不会跨来源重复累计同一 session。
- 保持 Tauri schema、前端与 resolver 不变。本决策不增加 `OPENCODE_DB` 或 channel database 探测。

## Alternatives considered

- 只读取当前检测到的 CLI 版本所对应的存储格式。拒绝原因是迁移可能保留旧存储和旧 session，已安装版本本身不能标识全部可读历史。
- 将所有来源的 token 相加。拒绝原因是迁移保留的相同 session ID 表示同一个逻辑 session，这会重复计算用量。
- 仅支持 SQLite。拒绝原因是 legacy JSON 安装以及未出现在 SQLite 中的 session 会消失。

## Consequences

- 历史与用量在 JSON 到 SQLite 以及 v1 到 v2 的迁移中保持可用。
- 每个 trim 后的 session ID 都有确定的来源选择，同时旧来源中的独有 session 仍然可见。
- 用量包含选中来源内的全部 message，但不会仅因迁移遗留另一份副本而重复计算 session。
- 版本探测报告工具实际输出的版本（含 prerelease 与 build metadata），更新检查容忍 build metadata 且不改变比较结果。
- 兼容性限定在 OpenCode CLI 探测与会话存储读取器内；公开 Tauri 数据形态、前端行为、resolver 行为与数据库探测范围均不扩展。
