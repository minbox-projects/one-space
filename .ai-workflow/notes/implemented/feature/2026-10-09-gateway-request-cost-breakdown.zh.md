# Agent Note: Gateway Request Cost Breakdown Snapshot

Status: implemented

[English](2026-10-09-gateway-request-cost-breakdown.md) | 中文

## Problem

用量日志此前为每个已转发尝试行只固化一个总额 `amount`，因此运维者能看到一次请求花了多少，却看不到该总额如何分解为输入、缓存读、缓存写与输出四档。固化的总额在记录时按本次实际转发服务商自身的精确价格行计算，之后无法重建，因为生效的时段、单价与各档费用从未保存；若从当前价格表回读，就会改写历史，并在费率或峰谷时段变化时明知与固化的 `amount` 不一致。请求日志行因此缺少运维者可检视的分档明细。

## Decision

`UsageCostBreakdown` 是 `src-tauri/src/ai_gateway/usage_log.rs` 新增的公开 struct，含八个数值字段：解析后的生效单价 `input_price`、`output_price`、`cache_read_price` 与 `cache_write_price`（美元/百万 tokens），以及固化的费用 `input_cost`、`output_cost`、`cache_read_cost` 与 `cache_write_cost`（美元）。总额刻意不在该 struct 内重复：`UsageLogRecord.amount` 仍是唯一权威总额。

`UsageLogRecord` 新增 `cost_breakdown: Option<UsageCostBreakdown>` 并带 `#[serde(default)]`，因此缺失或 `null` 的快照表示分档明细不可用，而不是零；显式零费用是真实的零。存储将其持久化为可空 `cost_breakdown TEXT` JSON 列，该列已存在于全新 schema，并由既有 `PRAGMA table_info` 驱动的幂等加法迁移 `migrate_usage_logs` 追加；新列绝不回填，`migration.rs`、配置 schema、`user_version` 与任何版本策略均不变。

公开 `compute_cost_at_time_with_breakdown(price, tokens, timestamp_ms) -> (f64, UsageCostBreakdown)` 只解析一次首个命中的 UTC+8 峰谷时段，并同时返回总额与快照，保持既有总额「先按档求和再除以一百万」的算术与档位顺序不变。`compute_cost_at_time` 委托它并返回 `.0`，因此既有调用方与语义得以保留。

`runtime_http::build_usage_log_row` 执行既有的、精确且大小写敏感的服务商加上游模型价格匹配，命中价格行时由那一次解析同时冻结快照与总额；未命中则两者都存 `None`。保存的 token 来自同一次真实尝试 usage，因此已定价的尝试、终止失败与成功都携带快照，命中价格行且费率为零或用量为零时仍记录实际费率与零费用。共享插入与投影 helper（`INSERT_SQL`、`insert_record`、`RECORD_COLUMNS` 与 `record_from_row`）在单行与批量写入以及所有会产出记录的查询中一并携带该列。

`src/lib/aiGateway.ts` 镜像 `UsageCostBreakdown` 并新增可选 `UsageLogRecord.cost_breakdown`。在 `src/components/AiGateway/UsageLogsPanel.tsx` 中，每个不分组行的花费单元格在金额旁渲染原生、可键盘聚焦的信息按钮，其本地化 `aria-label` 与 `useId` 生成的 `aria-describedby` 将其关联到 `role=tooltip` 明细面板；面板在鼠标进入或聚焦时打开，在鼠标离开、失焦或 Escape 时关闭。面板只读取该行存储快照与权威 `amount`，绝不查询当前配置：`amount` 存在时展示输入、输出与缓存读三档费用与单价加总额共七项，且仅当 `cache_write_tokens > 0` 时追加缓存写费用与单价。缺失快照或任一未知值渲染 `—` 并附本地化不可用说明，而显式零与实际费率保持记录原值；费用沿用既有四位小数展示，单价保留配置的数值并标注为美元/百万 tokens。面板以 `fixed` 定位，测量触发器矩形以保持右对齐并 clamp 在视口内，并在窗口 resize 与文档 scroll 时重新定位。五个新的本地化键（`aiGatewayLogsCostDetail`、`aiGatewayLogsCostFees`、`aiGatewayLogsCostUnitRates`、`aiGatewayLogsCostDetailTotal`、`aiGatewayLogsCostDetailUnavailable`）在两种语言中承载文案。分组、金额与既有查询均不变。

## Alternatives considered

- 在读取时按当前价格表从 `amount` 重算各档费用：未采纳，因为改价或改峰谷时段会改写历史请求的展示，并可能与固化的 `amount` 不一致。
- 只持久化四档单价、读取时再推导费用：未采纳，因为费用必须与产出 `amount` 的那一次解析完全一致，重新推导可能与其漂移。
- 在快照内再放一个冗余总额：未采纳，因为 `UsageLogRecord.amount` 已是唯一权威，第二个总额会引发分歧。
- 用当前价格表回填历史行：未采纳，因为历史的生效时段与费率不可知，回填会伪造精度；不可用就应保持 `null`。
- 为新列提升用量数据库或配置 schema 版本：未采纳，因为可空加法列不需要版本门控，且冻结契约禁止配置 schema 与版本变更。

## Consequences

- 行明细无需重新计价即可检视：保存的快照与产出 `amount` 的是同一次时段解析，含生效的峰谷时段。
- 旧行或更旧 payload 的缺失快照读回为 `null`，表示不可用，绝不表示零；显式零费率或零费用保留为零。
- 存储变更是加法且可空的，因此更旧的显式列客户端仍可查询与追加，回滚时该列无害且旧行为 `null`。
- 既有 `amount`/指标 SQL 与 usage 解析不变，未引入当前配置价格回读、回填或取整修复。
- 本记录由计划 `20261009-gateway-request-cost-breakdown` 的 Step 1（后端快照与存储）与 Step 2（前端检视）交付：不分组请求日志列表通过关联的花费明细控件展示存储快照与权威 `amount`。
- 部分取代：[API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) 被保留并交叉链接；本记录在存储行字段上新增可选的固化 `cost_breakdown` 快照及其可空列，而该记录的 SQLite 日志、记录时价格冻结、保留、`group_by` 契约与 `local_model` 面决策仍然有效。
- `MEMORY.md`、`navigation.json` 的 `ai-gateway` 条目（镜像类型与展示行为）与 `ai-gateway-backend` 条目（类型、helper、列与快照行为）描述同一组事实，`navigation.md` 已按权威 JSON 重新生成。
