# Agent Note: API Gateway Usage Queries Replace Day Counts with a Named Range Selector

Status: implemented

[English](2026-09-22-gateway-usage-yesterday-range.md) | 中文

## Problem

两个网关查询命令此前把快捷范围编码为可选的天数：`api_gateway_usage_stats(days: Option<i64>)` 与 `api_gateway_request_logs(days: Option<i64>, group_by, status, model, page)` 用 `days = null` 表示全部时间，用 `Some(1)`、`Some(7)`、`Some(15)`、`Some(30)` 表示今日与包含今日在内的近 N 个自然日。操作者要求新增「昨天」范围，而该编码无法表达它：每个 `Some(n)` 窗口都从今日 00:00（UTC+8）开始、到明日 00:00 结束，因此半开窗口「昨天」`[昨天 00:00, 今天 00:00)` 不是任何受支持窗口的后缀；而 `Some(0)`、缺省与负值在此前的归一化下已经表示全部时间，没有任何数值可用于承载昨天。小时与自然日的分桶选择也必须跟随窗口：单日窗口按小时分布展示，多日窗口按自然日展示。

## Decision

两个命令现在改用去除首尾空白后的字符串选择器，而不再是天数：`api_gateway_usage_stats(range: Option<String>)` 与 `api_gateway_request_logs(range: Option<String>, group_by, status, model, page)`。取值范围为 `today`、`yesterday`、`7d`、`15d`、`30d` 与 `all`；`None`、空串与 `all` 表示无界的全部时间，未知取值会返回列出受支持取值集合的可操作错误，绝不回退到全部时间或今日。`yesterday` 是 UTC+8 半开窗口 `[昨天 00:00, 今天 00:00)`，`7d` / `15d` / `30d` 保持原有窗口不变（包含今日在内共 N 个自然日，结束于明日 00:00（UTC+8））。时间分桶粒度由解析后的窗口决定：恰为一个 UTC+8 自然日（`today` 与 `yesterday`）序列化 `granularity = "hour"`，N 个自然日与全部时间序列化 `granularity = "day"`。前端在用量统计与请求日志两个面板之间共享同一份六个条目的范围列表——今日、昨天、近 7 天、近 15 天、近 30 天、全部——默认今日；`UsageRangeKey` 新增 `yesterday`，不再被使用的 `usageRangeToDays` 辅助函数已删除，新的 i18n 键 `apiGatewayRangeYesterday` 为 Yesterday / 昨天，运行时状态卡片的今日统计调用继续发送 `range: "today"`。数据库 schema、持久化与聚合语义均不变；命令名与响应结构保持原样。

## Alternatives considered

- 保留 `days` 并新增一个布尔或偏移参数表示昨天：未采纳，因为那会让同一概念在同一个签名里出现两种编码，需要维护第二条分支，且参数之间谁优先并不明确；单一命名选择器足以承载界面提供的全部范围。
- 在前端对今日或全部时间的结果做本地过滤，或取两个窗口再相减：未采纳，因为范围解析、聚合与固定分页都由后端持有并锚定 UTC+8；客户端窗口需要重新实现日边界、破坏后端合计，并让一个选择器值付出双倍查询成本。
- 用 `days = 0` 或负数天数编码昨天：未采纳，因为 `0`、缺省与负值在此前归一化下已表示全部时间，重新定义会静默改变既有调用方，并让选择器取值变得含混。
- 所有范围都返回小时分桶，或新增独立的粒度参数：未采纳，因为分桶粒度是解析后窗口的纯粹推论；由调用方请求的粒度可能与窗口矛盾，产生空白或误导性的分布。

## Consequences

- 命令签名发生变化：`api_gateway_usage_stats` 现在接收 `range`，`api_gateway_request_logs` 以 `range` 取代 `days`；`MEMORY.md`、`docs/USAGE.md` 与 `api-gateway` / `api-gateway-backend` 导航条目描述六个选择器与由窗口决定的粒度，`navigation.md` 按权威 JSON 重新生成。
- 前端两个面板共用同一份 `UsageRangeKey` / `USAGE_RANGE_KEYS` 列表，因此两个页签不会各自漂移；被删除的 `usageRangeToDays` 辅助函数已无调用方。
- 数据库 schema、持久化、聚合与金额语义都不变：`api_gateway_usage.db`、保留行为与两个命令的响应结构均未改动，差异只在解析出的窗口与分桶标签。
- 回滚就是恢复签名与其调用点：旧构建无法读取 `range`，必须同时恢复 `days` 参数与构造数值的调用方；没有任何已存数据需要迁移或重写。
- Supersession（取代评估）：部分取代。[API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) 曾记录 `days = null` 表示全部时间、`Some(1)` 表示今日；该范围编码表述由本记录取代，其存储、记录、计价与保留决策仍然有效。
