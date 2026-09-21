# Agent Note: Gateway Cancelled Requests Are Not Logs

Status: implemented

[English](2026-09-21-gateway-cancelled-requests-are-not-logs.md) | 中文

## Problem

网关此前把下游工具连接关闭或已完成的上游响应变得不可交付视为需要存储的业务结果。它会写入一条合成 `cancelled` 终止行，并保留此前已经完成的上游 attempts。这使内部 transport 生命周期事件以请求日志状态可见，尽管该入站 OpenCode 或 Codex 请求并未向调用方完成。历史 `cancelled` 行还会影响请求日志页、筛选面、分组与时间戳，因此只移除可见筛选项无法一致地移除该状态。

## Decision

下游 TCP 关闭或响应交付失败不是业务请求日志结果。连接处理器会丢弃该 inbound request 的整个缓冲 usage-log 集合，包括取消前已经完成的每次上游尝试，并且不写合成 `cancelled` 终止行。它仍会及时取消未完成的转发、重试等待与后续尝试；服务商健康、自动禁用、session affinity 与调用方可见响应行为保持不变。正常完成的 success、failure、重试恢复多尝试与无候选请求维持既有的按尝试行和恰好一个终止行，包括无候选时合成的 HTTP 502 终止行。

存储结果为 `cancelled` 的既有行在物理上保持存在，并可由 raw 或兼容代码读取。所有用户可见请求日志查询都一致地把这些行排除在不分组记录、`total`、`total_pages`、页码收敛、`models` 面、按模型/按天分组、请求数、错误数和最后请求时间之外。兼容 `status="cancelled"` 筛选仍合法并返回空页。前端可见筛选只提供 success 与 failure，并在渲染前防御性移除 stale cancelled 记录。Rust `UsageResult::Cancelled` 解析与序列化、TypeScript 兼容 union member 和翻译 helper 继续保留；不执行 schema migration、行改写或历史删除。

## Alternatives considered

- 保留合成终止行，只隐藏其前端徽标：未采纳，因为后端总数、分页、筛选面、分组与时间戳仍会暴露取消的 transport 事件，stale 客户端也仍可能渲染它们。
- 只丢弃合成终止行，但保留已完成 attempts：未采纳，因为这些尝试属于同一个未以业务结果完成的入站请求；保留它们会让取消请求部分可见，也会以违反所选请求日志边界为代价保留上游成本归属。
- 删除或迁移历史 cancelled 行：未采纳，因为兼容不需要破坏性数据改写，既有保留策略已负责物理清理，而保留 parser 与 type 可让旧数据库和 stale payload 继续可读。
- 移除 cancelled enum/type/filter 兼容面：未采纳，因为历史数据库与 stale 调用方仍可能提供该值；接受并返回空结果可以避免破坏性输入变更，同时不使它重新可见。

## Consequences

- 取消或不可交付的入站请求写入零行，即使一个或多个尝试（包括一次上游成功）已在断连前完成。已批准的取舍是失去这些已完成上游工作及其可能产生费用的请求日志可见性。
- 历史 `cancelled` 行保留在 `api_gateway_usage.db` 中，直到普通保留策略将其删除，但所有用户可见请求日志页、总数、页数、模型面、按模型/按天分组和时间戳都会排除它们。legacy cancelled 状态筛选成功返回空页，界面不提供也不渲染 cancelled 状态。
- Rust 与 TypeScript 中继续保留兼容表示，因此这项简化不需要 SQLite schema migration、数据删除或 payload shape 变更。
- 正常 success、failure、重试恢复多尝试与无候选日志语义保持不变，错误文本、token 与费用处理、保留和 attempt 标签也保持不变。
- 部分取代：[Gateway Per-Attempt Request Logging and Stored Error Text](../architecture/2026-09-20-gateway-per-attempt-logging-and-error-text.md) 对正常完成请求的按尝试日志、终止行、已存错误文本、迁移缺省值与隐私边界仍然具有权威性。本记录只替换其取消决策，即保留已完成尝试、写入合成 `cancelled` 终止行并在请求日志视图中暴露历史 cancelled 行的部分。
