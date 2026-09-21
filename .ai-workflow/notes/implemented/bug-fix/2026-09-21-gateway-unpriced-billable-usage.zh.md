# Agent Note: Gateway Unpriced Hint Counts Only Billable Usage

Status: implemented

[English](2026-09-21-gateway-unpriced-billable-usage.md) | 中文

## Problem

用量统计的未定价提示统计了每一个到达上游模型但没有匹配价格行的终止请求，包括完全没有记录用量的终止失败。因此不消耗任何用量、也不产生任何花费的请求仍会要求配置价格行，`unpriced_count` 与 `unpriced_items` 夸大了真正缺少价格的请求数，请求全部为零用量失败的用量行也一直显示 `—`。网关确实会记录带有真实用量的失败——流式尝试在首个转发字节之后失败时存为状态 502 的 `failure` 并保留已累积 usage——所以仅凭「到达上游模型」并不代表请求会产生用量成本。

## Decision

仅当以下三条同时成立时请求才算未定价：存储的 `amount` 为 `NULL`（没有匹配价格行）、`upstream_model` 非空（请求到达了上游模型），且请求会产生用量成本——`result = 'success'`，或记录的 `total_tokens > 0`。相同的谓词 `amount IS NULL AND upstream_model <> '' AND (result = 'success' OR total_tokens > 0)` 在 `src-tauri/src/api_gateway/usage_log.rs` 的两处 SQL 中一致应用：为 totals、时间分桶、按模型行与按服务商行提供数据的 `METRIC_COLUMNS` 未定价聚合，以及列出受影响模型与服务商行的 `unpriced_items` 查询。

口径分层是有意为之。请求数仍覆盖全部终止行，tokens 与金额仍累加各行实际记录的值，包括保留部分用量的失败；只有未定价提示收窄为真正会产生花费的行。零用量失败与无上游失败仍保留未定价行的 `None` 金额，但按 0 成本处理、不再计入未定价。行为覆盖：`usage_stats_unpriced_eligibility_requires_success_or_usage` 断言零用量失败被排除、部分用量失败被计入、零用量成功被计入；`all_candidates_failed_request_writes_one_row_per_completed_attempt` 期望未定价数为 0，因为该请求的每次尝试都是零用量失败。

## Alternatives considered

- 只统计 `result = 'success'`：未采纳，因为流式尝试在首个转发字节之后失败时记为状态 502 的 `failure`，同时保留已累积 usage，其 tokens 与固化金额是真实可计费的，排除它会掩盖一个需要价格行的模型。
- 只统计 `total_tokens > 0`：未采纳，因为零用量成功同样证明该模型正在被使用，操作者仍需价格行才能为后续用量计价。
- 保留原谓词（任何到达上游模型但没有匹配价格行的终止行）：未采纳，因为它把零成本请求报告为未定价并产生错误的提示噪音，零用量失败尤为明显。

## Consequences

- 未定价提示数现在与会产生用量成本的请求一致：`unpriced_count`、`unpriced_items` 与 `—` 显示排除零用量失败与无上游失败，同时保留零用量成功与带用量的失败。
- 请求全部为零用量失败的用量行不再显示 `—`；其聚合金额为 0，在 `Cost ($)` 列显示为 `0.0000`。
- 请求数、tokens、金额与终止行/取消行规则保持不变：`COUNT(*)`、token 与 amount 聚合、保留策略、按尝试行与取消行排除均维持既有行为。
- 部分取代：[API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) 被保留并交叉链接；本记录只替换其未定价资格，包括「只有 2xx 上游响应计入 tokens 与花费」的旧表述——现已纠正为非流式只从 2xx 响应体解析，中途流失败保留已累积 usage——其 SQLite 日志、记录时价格冻结、保留、`group_by` 契约与 `local_model` 面决策仍然有效。
