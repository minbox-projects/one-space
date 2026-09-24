# Agent Note: Gateway Cancelled Requests Are Not Logs

Status: implemented

[English](2026-09-21-gateway-cancelled-requests-are-not-logs.md) | 中文

## Problem

网关此前把下游工具连接关闭或已完成的上游响应变得不可交付视为需要存储的业务结果。它会写入一条合成 `cancelled` 终止行，并保留此前已经完成的上游 attempts。这使内部 transport 生命周期事件以请求日志状态可见，尽管该入站 OpenCode 或 Codex 请求并未向调用方完成。历史 `cancelled` 行还会影响请求日志页、筛选面、分组与时间戳，因此只移除可见筛选项无法一致地移除该状态。

## Decision

下游 TCP 关闭或响应交付失败不是业务请求日志结果。连接处理器会丢弃该 inbound request 的整个缓冲 usage-log 集合，包括取消前已经完成的每次上游尝试，并且不写合成 `cancelled` 终止行。它仍会及时取消未完成的转发、重试等待与后续尝试；服务商健康、自动禁用、session affinity 与调用方可见响应行为保持不变。正常完成的 success、failure、重试恢复多尝试与无候选请求维持既有的按尝试行和恰好一个终止行，包括无候选时合成的 HTTP 502 终止行。

存储结果为 `cancelled` 的既有行不会在升级中幸存：旧用量数据库首次打开时在数据库级版本门控下（`PRAGMA user_version` 推进到 1）以事务一次性删除所有 `cancelled` 行，之后的打开不再删除任何行。所有用户可见请求日志查询此前即把那些行排除在不分组记录、`total`、`total_pages`、页码收敛、`models` 面、按模型/按天分组、请求数、错误数和最后请求时间之外，因此删除不会改变任何可见页、总数或分组。此后后端既不定义、不解析也不过滤 `cancelled`，前端既不声明、过滤、样式化也不标注它，兼容 `status="cancelled"` 别名也不再保留（[Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md)）。

## Alternatives considered

- 保留合成终止行，只隐藏其前端徽标：未采纳，因为后端总数、分页、筛选面、分组与时间戳仍会暴露取消的 transport 事件，stale 客户端也仍可能渲染它们。
- 只丢弃合成终止行，但保留已完成 attempts：未采纳，因为这些尝试属于同一个未以业务结果完成的入站请求；保留它们会让取消请求部分可见，也会以违反所选请求日志边界为代价保留上游成本归属。
- 永久保留历史 `cancelled` 行及其兼容面：未采纳，因为该状态对所有用户可见查询本就不可见，保留它只会保留永久的排除条件与死掉的 parser/filter 面；在数据库版本门控下一次性删除这些行即可移除该状态而不改变任何可见结果。
- 在每次打开时删除 `cancelled` 行，或把它们改写成可见状态：未采纳，因为删除必须可证明只发生一次，且绝不能把 transport 事件重新归类为业务结果；版本标记记录清理已完成，而重归类会伪造该请求从未产生的日志行。

## Consequences

- 取消或不可交付的入站请求写入零行，即使一个或多个尝试（包括一次上游成功）已在断连前完成。已批准的取舍是失去这些已完成上游工作及其可能产生费用的请求日志可见性。
- 历史 `cancelled` 行在旧数据库首次打开时由数据库版本门控恰好一次地从 `api_gateway_usage.db` 物理删除；由于所有用户可见请求日志页、总数、页数、模型面、按模型/按天分组与时间戳此前即排除它们，任何可见结果都不变，且该状态无法恢复。
- Rust `UsageResult` 成员及其 parser 与 SQL 排除、TypeScript 兼容成员、翻译 helper、前端防御性过滤与兼容 `status="cancelled"` 输入均已移除；清理改为推进数据库版本，不改写其余行，也不改变 payload 形状。
- 正常 success、failure、重试恢复多尝试与无候选日志语义保持不变，错误文本、token 与费用处理、保留和 attempt 标签也保持不变。
- 部分取代：[Gateway Per-Attempt Request Logging and Stored Error Text](../architecture/2026-09-20-gateway-per-attempt-logging-and-error-text.md) 对正常完成请求的按尝试日志、终止行、已存错误文本、迁移缺省值与隐私边界仍然具有权威性。本记录只替换其取消决策，即保留已完成尝试、写入合成 `cancelled` 终止行并在请求日志视图中暴露历史 cancelled 行的部分。
- 部分取代：[Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md) 只取代本记录让历史 `cancelled` 行物理保留并保留兼容表示的决定；"取消或不可交付的入站请求写零行"的规则及本记录其余全部决定仍然成立。
