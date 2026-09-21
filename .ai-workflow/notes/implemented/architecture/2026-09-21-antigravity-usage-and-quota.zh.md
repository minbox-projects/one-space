# Agent Note: Antigravity Usage and Quota: Legacy Token Scan, Transcript Call Counts and Quota Command

Status: implemented

[English](2026-09-21-antigravity-usage-and-quota.md) | 中文

## Problem

基线曾认为 Antigravity 不在磁盘上持久化任何 token 用量，因此 `src-tauri/src/ai_sessions/usage.rs` 带有永不解析注释，并由守卫测试强制永久 `unavailable`。该决策在两个方向上都已错误：旧格式 `~/.gemini/tmp` 文件确实包含按消息记录的 token 数据，且用户在其中已积累历史；新格式 brain transcript（`transcript_full.jsonl`）虽有会话行，却完全不含 token 用量。双轴评审 blocker N-F1 与 major 意见 N-F3 要求把这次反转留痕：哪个来源产出 token 记录、哪个来源只产出调用计数、`empty` 如何判定、额度从何而来。

## Decision

- 递归扫描旧格式 `~/.gemini/tmp` 下 `session-` 前缀的 `.json` 与 `.jsonl` 文件（`src-tauri/src/ai_sessions/usage.rs` 的 `collect_antigravity_usage_records`），取代以往恒为 `unavailable` 的旧决策：旧文件确实含有 token，且用户有存量数据。Token 解析保持迁移前的 gemini 口径：`.json` 消息把 `tokens.cached` 与 `tokens.cache` 求和计入缓存 token，模型按 `message.model`、`message.modelName`、`message.metadata.model`、最后文件级模型的顺序回退，`.jsonl` 行读取 `tokens.cached`；输入、输出、缓存与总量全为零的消息和行一律跳过。只有这条旧格式路径产出 `UsageRecord`。
- 新格式 brain transcript 只做调用计数。两个 brain 根（`~/.gemini/antigravity-cli/brain` 与 `~/.gemini/antigravity/brain`，经 `src-tauri/src/ai_sessions/history.rs` 的 `antigravity_brain_roots` 与 `find_antigravity_transcript` 定位）下每个会话的 `transcript_full.jsonl`，把窗口内的 `USER_INPUT` 行——以 `eq_ignore_ascii_case` 匹配、复用 `antigravity_entry_timestamp_ms` 时间戳 helper、按 `[start_ms, end_ms)` 做窗口过滤——计入 `scanned_sessions` 与 `scanned_calls`（`transcript_calls`），绝不进入 `records`。因此 summary、日分桶与模型统计不受 token 污染：新格式没有 token，调用计数是唯一诚实的口径。
- `empty` 判定保持基线不变：`aggregate_tool_usage` 仅在扫描结果为 `available` 且 `scanned_sessions` 为零时报告 `empty`。有源但无 token 时，前端显示 `available` 加 Antigravity 专属的 token 不可用说明（`src/components/AiUsageStats.tsx` 的 `aiUsageTokenUnavailableLocally`，即 `Token usage locally unavailable`），仅在 `tool === "antigravity"` 且 `source_status === "empty"`、`scanned_sessions > 0`、summary 调用为零时生效；不影响其他任何工具的文案。
- 额度来自专属的 `sessions_antigravity_quota` 命令（`src-tauri/src/ai_sessions/usage.rs` 的 `#[tauri::command(async)]`）：执行 `agy -p /usage --output-format json --print-timeout 30s` 并设 35 秒硬超时，解析 `status` / `command.data.groups` 信封（`parse_antigravity_quota_envelope` 在非 `SUCCESS` 状态或缺 groups 时报错，分桶缺数字 `remaining_fraction` 则整个信封失败），仅缓存成功的快照（5 分钟 TTL），失败永不缓存，报错为中英双语，且直读而不经过 30 秒用量扫描缓存（`USAGE_SCAN_CACHE_TTL`）。

## Alternatives considered

- 维持恒为 `unavailable` 的基线：未采纳，因为旧文件确实含有 token 数据且用户已有存量，拒绝解析会丢弃真实用量。
- 把 transcript `USER_INPUT` 行按估算 token 提升为 token 记录：未采纳，因为新格式不含 token 用量，任何估算都会污染 token 聚合；调用计数是唯一诚实的口径。
- 把有源无 token 的情况判为 `unavailable` 而非 `empty`：未采纳，因为基线契约（`available` 且零扫描会话即 `empty`）仍然成立，且前端已用 Antigravity 专属说明区分该情形。
- 让额度命令走用量扫描缓存：未采纳，因为额度是实时数据，有自己独立的 5 分钟成功缓存 TTL，必须按需直读。

## Consequences

- 旧格式 `~/.gemini/tmp` 历史重新按迁移前 gemini 口径产出 token 记录，而新格式 brain transcript 只贡献 `scanned_sessions` 与 `scanned_calls`。
- 由于 transcript 不含 token，9 月后的 agy 交互会话仍然没有 token 明细；对它们只报告调用数与会话数。
- 额度卡片读取实时 `agy` 输出并带 5 分钟成功缓存；失败时展示双语错误并按需重试。
- 已知覆盖缺口，如实记录：额度进程路径与零 quota 成本行为无测试证明；仅信封解析（`parse_antigravity_quota_envelope`）有测试覆盖。
- print 模式的增量记录作为后续项，暂不实现。
- 取代评估：无取代。没有任何现存记录描述旧的永不解析基线或任何 Antigravity 用量语义，因此无需保留或交叉链接；提及 Antigravity 的终端同步记录（[API Fusion Terminal Independent Provider](../feature/2026-09-17-api-fusion-terminal-independent-provider.md)）仅说明网关同步不触碰 `claude`/`antigravity` 记录，与用量扫描无关。
