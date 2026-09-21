# Agent Note: API Gateway Usage Stats and Request Logs

Status: implemented

[English](2026-09-17-ai-gateway-usage-logs.md) | 中文

## Problem

API 网关此前只负责转发流量，却不保留任何服务记录。用户无法查看每个请求的 tokens、花费、成功或失败结果，也无法知道某个本地模型实际由哪个上游服务商处理；唯一持久化的状态是加密的 `api_fusion.json` 服务商、Key 与终端同步台账。因此不借助外部工具就无法回答「这个网关花了我多少钱、哪些请求失败了」，本地中继也没有价格模型与保留控制。

## Decision

请求日志存于 `get_app_dir()` 下独立、不加密的 SQLite 文件 `api_gateway_usage.db`，由 `src-tauri/src/api_gateway/usage_log.rs` 的 `UsageLogStore` 实现，schema 为 `usage_logs`。一条记录保存 UTC+8 时间戳、本地模型、本次实际转发的上游模型、服务商 id 与名称、结果（`success` | `failure` | `cancelled`）、HTTP 状态、输入、缓存读、缓存写与输出四档 token、总 token、金额与耗时；不保存请求或响应正文、不保存请求头、不保存任何凭据。只有通过本地鉴权并进入 `/chat/completions` 或 `/responses` 转发流程的请求才会记录，本地鉴权失败（401）、`GET /v1/models` 与未知路径或方法不记录。非流式响应从已缓冲的响应体解析，流式响应由 `SseUsageAccumulator` 从 SSE `usage` 分片只读累积，不改写转发给调用方的字节；缺失的用量字段记 0，没有 usage 的请求仍会记录一行，只有写入存储失败时记录日志并吞掉错误，绝不影响响应。保留天数在新增的独立设置分区 `ai-gateway` 配置（`src/components/SettingsView.tsx` 的 `SettingsTab`，默认 90，范围 1–365，存储为带 `#[serde(default)]` 的 `usage_retention_days`），每次追加日志都会永久删除超过保留窗口的行。

金额在记录时按本次实际转发的服务商自身价格行与上游模型名精确匹配计算（`src-tauri/src/api_gateway/usage_log.rs` 的 `match_price_for_provider` 与 `compute_cost_at_time`，只精确、大小写敏感地命中该服务商自己的行，全局行或其他服务商的行绝不匹配；四档单价格为输入 / 缓存读 / 缓存写 / 输出，单位为美元/百万 tokens），并把金额固化进该行，因此之后修改价格永不重算历史。未定价模型存 `None`、界面显示 `—` 且不计入合计；`unpriced_count` 只对会产生用量成本的请求驱动「请求未配置价格」提示（成功，或记录的 `total_tokens > 0`），零用量失败按 0 成本排除在外。价格维护已改为在新增/编辑上游服务商对话框内按每条映射行维护（`MappingPriceEditor`）并随服务商保存，因此用量统计页签不再提供价格入口，旧 `ModelPriceDialog` 已删除（[Gateway Model Prices Move Into Provider Mappings](2026-09-19-gateway-model-prices-in-provider-mappings.md) 部分取代本记录）。`GatewayConfig` 新增 `usage_retention_days`（默认 90）与 `model_prices`，两者都带 `#[serde(default)]`，因此旧 `api_fusion.json` 无需迁移仍可反序列化。当前命令面为 `api_gateway_usage_stats`、`api_gateway_request_logs`、`api_gateway_usage_retention_get` 与 `api_gateway_usage_retention_save` 四个命令；独立的 `api_gateway_model_prices_get` 与 `api_gateway_model_prices_save` 命令已由 2026-09-19 变更移除。聚合、分组、筛选与分页均在后端完成，页大小固定 50，范围按 UTC+8 解析（`resolve_range`）；`days = null` 表示全部时间，`Some(1)` 表示今日。价格与保留天数保存会读取现有配置、只替换自身字段再写回，因此绝不会清空服务商、本地 Key 与 `terminal_syncs`，且 1–365 之外的非法保留天数会被拒绝并返回可操作错误、绝不落盘。前端在既有 `api-gateway` 页面内新增两个页签（`usage`、`logs`）与一个设置分区（`ai-gateway`）；未新增导航 id。

`api_gateway_request_logs` 的不分组规范取值为 `group_by: "none"`，前端即发送该值；`null`、缺省与空串作为向后兼容的不分组别名被容忍，`"model"` 与 `"day"` 选择分组行，其他取值会被拒绝并返回可操作错误。响应在 `group_by` 中回显分组值，不分组页返回 `null`。不分组页还带有范围内有界的 `models: string[]` 面，即范围内去重且非空的 `local_model`，它受已解析范围与状态筛选约束，但不受分页与模型筛选影响，因此前端模型选择器可以提供当前页未出现的范围内模型。非流式响应只从其 2xx 响应体计入 tokens 与花费；流式尝试在首个转发字节之后失败时记为状态 502 的 `failure`，但保留已累积的 usage，其 tokens 与固化金额同样计入；未捕获用量的失败四档 token 全为 0。`unpriced_count` 与 `—` 显示只适用于已到达上游模型、没有匹配价格行且会产生用量成本的请求（成功，或记录的 `total_tokens > 0`），因此零用量失败、取消与无上游失败按 0 成本处理，不计入未定价。

## Alternatives considered

- 把用量数据放入加密的 `api_fusion.json` 台账：未采纳，因为该文件是整体重写的凭据、服务商与终端同步存储，而用量行是高写入、以追加为主的数据；独立 SQLite 文件让日志脱离凭据存储，并让写入与删除保持事务性。
- 持久化请求与响应正文以便日后重算：未采纳，因为数据边界禁止正文、请求头与凭据，且金额在记录时固化后并不需要重算。
- 在查询时按当前价格表重算金额：未采纳，因为那样改价就会改写历史、让既有合计不稳定，与记录时固化相矛盾。
- 把价格表放进服务商表单或设置页：未采纳，因为价格以跨服务商的上游模型名为键，而不是按服务商划分，入口应紧邻它所解释的用量分析。
- 把 HTTP 200 的流式响应当作成功：未采纳，因为在做出本决策时，全部上游不可用的流式请求同样返回 HTTP 200 的 SSE 错误事件，因此结果取自网关最终状态并记为 `failure`，且不计金额；该 200-SSE 前提对预写出失败已不再成立，现在改为返回 HTTP 502 加 JSON 错误信封。
- 让前端一次性拉取全部日志并在本地分页：未采纳，因为日志可无上限增长，因此聚合、分组、筛选与固定 50 行的分页都留在后端并按 UTC+8 解析范围。
- 不设保留天数、永久保留日志：未采纳，因为带永久删除的显式窗口是本次要求的数据边界，且清理在每次写入时执行，而非依赖后台调度。

## Consequences

- 日志存于 `get_app_dir()` 下独立、不加密的 SQLite 文件 `api_gateway_usage.db`；删除该文件即可清理全部日志数据，加密的 `api_gateway.json` 台账不受影响。
- 金额在记录时固化，因此之后修改价格只影响新请求；非流式响应只从其 2xx 响应体计入 usage，中途流失败以状态 502 的 `failure` 行保留已累积 usage，未捕获用量的失败记 0 tokens；未定价模型显示 `—`、不计入合计，并仅在请求已到达上游模型、没有匹配价格行且会产生用量成本（成功，或记录的 `total_tokens > 0`）时计入未定价请求提示。
- 请求日志契约稳定，并由 `MEMORY.md` 与 `docs/USAGE.md` 同步描述：`group_by: "none"` 是不分组规范值，`null`、缺省与空串为不分组别名，`"model"` 与 `"day"` 为分组值，不分组响应的 `group_by` 为 `null`，不分组页范围内有界的 `models` 面独立于分页与模型筛选，驱动前端模型筛选。
- `GatewayConfig` 新增 `usage_retention_days`（默认 90）与 `model_prices`，两者都带 `#[serde(default)]`；旧配置无需迁移仍可反序列化，而回滚版本重写该文件会把这些字段退回默认值。
- 价格与保留天数保存会合并进现有配置，服务商、本地 Key 与 `terminal_syncs` 得以保留；1–365 之外的非法保留天数会被拒绝并返回可操作错误、绝不落盘。
- 四个当前命令 `api_gateway_usage_stats`、`api_gateway_request_logs`、`api_gateway_usage_retention_get` 与 `api_gateway_usage_retention_save` 由 `lib.rs` 导出并在 `app_runtime/run_app.rs` 注册，聚合、分组、筛选与固定 50 行的分页均在后端完成；独立的 `api_gateway_model_prices_get` 与 `api_gateway_model_prices_save` 命令已由 2026-09-19 变更移除。
- 前端在既有 `api-gateway` 页面内提供 `usage` 与 `logs` 页签，并提供 `ai-gateway` 设置分区；未改动任何导航 id、启动器入口或其他页面。
- `docs/USAGE.md`、`MEMORY.md` 与 `navigation.json` 描述该存储、记录、计价、保留与价格入口行为，`navigation.md` 已按权威 JSON 重新生成。
- `notes list` 的 active 记录中没有相关的用量或请求日志决策，因此本记录对它们既非完全取代也非部分取代；这是一条新的决策记录，而不是对 `2026-09-17-api-fusion-terminal-independent-provider` 或 `2026-09-17-bilingual-note-triplets` 的重写。
- 部分取代：本记录由 [Gateway Single-Candidate Fast Fail and Standard Error Responses](2026-09-18-gateway-fast-fail-and-standard-errors.md) 保留并交叉链接，后者只取代上文的流式预写出失败前提；本文的 SQLite 日志、记录时计价、保留与 `unpriced_count` 决策仍然有效。
- 部分取代：本记录由 [Gateway Model Prices Move Into Provider Mappings](2026-09-19-gateway-model-prices-in-provider-mappings.md) 保留并交叉链接，后者只取代上文的定价入口与全局匹配决策——价格改为在服务商对话框内按映射行维护并随服务商保存，只有本次实际转发服务商自己的精确价格行参与计价，独立的 `api_gateway_model_prices_get` / `api_gateway_model_prices_save` 命令已移除；本文的 SQLite 日志、记录时价格冻结、保留与 `unpriced_count` 决策仍然有效。
- 部分取代：本记录由 [Gateway Per-Attempt Request Logging and Stored Error Text](2026-09-20-gateway-per-attempt-logging-and-error-text.md) 保留并交叉链接，后者只取代上文的「按请求记一行」记录粒度与不存储错误文本的决策；本文的 SQLite 存储边界、记录时价格冻结、保留、`unpriced_count`、`group_by` 契约与 `local_model` 面决策仍然有效。
- 部分取代：[Gateway Unpriced Hint Counts Only Billable Usage](../bug-fix/2026-09-21-gateway-unpriced-billable-usage.md) 只取代上文的未定价资格——只有已到达上游模型、没有匹配价格行且会产生用量成本（成功，或记录的 `total_tokens > 0`）的请求才算未定价——本文的 SQLite 日志、记录时价格冻结、保留、`group_by` 契约与 `local_model` 面决策仍然有效。
