# Agent Note: API Gateway Cache Hit Rate Normalizes Provider Usage Semantics

Status: implemented

[English](2026-09-21-api-gateway-cache-hit-accounting.md) | 中文

## Problem

用量面板此前在前端按 `cache_read / (input + cache_read)` 自行推算 Token 缓存命中率，但各服务商报告的输入语义互不兼容。OpenAI Chat 与 Responses 报告的输入总量（`prompt_tokens` / `input_tokens`）已包含缓存子集，而 Anthropic 风格上游报告的顶层输入本身已是普通输入，另有独立的 `cache_read_input_tokens` 与 `cache_creation_input_tokens` 档位。把这些原始输入相加会高估分母，因此同一个本地模型由不同服务商服务时得到互不一致、无法解释的命中率，两个各自报告 `80%` 的服务商汇总后约为 `57%`。历史数据库行、上游未返回 `usage` 对象的请求以及分母刻意为零的请求都与真实的 `0%` 无法区分，覆盖率缺口因此伪装成了真实的未命中率。

## Decision

每个新解析的上游 `usage` 对象都由 `src-tauri/src/api_gateway/usage_log.rs` 的 `canonical_usage_from_value` 归一化为四个互斥档位：`input_tokens`（普通输入）、`cache_read_tokens`、`cache_write_tokens` 与 `output_tokens`。`input_tokens` 优先于 `prompt_tokens`，`output_tokens` 优先于 `completion_tokens`；OpenAI 的包含语义用 checked subtraction 从报告总量中减去其嵌套缓存分量，因此任何缓存 token 都不会被重复计算。

| Shape | Detection and source fields | Normalization |
| --- | --- | --- |
| OpenAI Chat | `prompt_tokens` 或 `input_tokens` 搭配嵌套 `prompt_tokens_details.cached_tokens`；可选嵌套 `prompt_tokens_details.cache_write_tokens` | `ordinary = reported − cache_read − cache_write`，使用 checked subtraction |
| OpenAI Responses | `input_tokens` 搭配嵌套 `input_tokens_details.cached_tokens`；可选嵌套 `input_tokens_details.cache_write_tokens` | 与 OpenAI Chat 相同的 checked subtraction |
| Anthropic-style split | 顶层 `input_tokens` 或 `prompt_tokens` 搭配 `cache_read_input_tokens` 与 `cache_creation_input_tokens`，没有嵌套缓存明细 | 报告的输入本身已是普通输入；缓存读写保持为独立档位 |
| Mixed compatible write | 有嵌套缓存读、没有嵌套缓存写，另有顶层 `cache_creation_input_tokens` | 按 OpenAI 包含语义，把顶层 creation 值作为缓存写回退扣除；嵌套缓存写优先于该回退 |

两种形态属于非法且绝不用饱和减法修复：嵌套缓存读与顶层 `cache_read_input_tokens` 同时存在，以及嵌套缓存分量超过报告的输入。非法行保留报告输入且不含缓存档位，使计费保持保守、绝不为负或重复计算，其缓存档位绝不参与缓存分档计费，该请求也保持缓存统计不合格。历史金额永不重算。

存储以幂等、只追加的方式迁移。`PRAGMA table_info` 追加 `usage_semantics`（既有行默认为 `legacy`，新行为 `canonical_v1`）、`usage_present` 与 `cache_accounting_valid`；原有 token、total、amount 与日志字段绝不改写，legacy 行被排除在新分子与分母之外。

`UsageMetrics` 在 totals、时间分桶、模型与服务商四个层级新增五个始终序列化的附加字段：`cache_hit_tokens`、`cache_eligible_tokens`、`cache_hit_rate_percent`（`0..=100` 的百分数或 `null`）、`cache_rate_eligible_count` 与 `successful_request_count`。命令名 `api_gateway_usage_stats` 及其既有字段不变。分子为 `cache_read` 求和，分母为 `ordinary + cache_read + cache_write` 求和；所有层级共用同一合格谓词（终止、`result='success'`、`usage_semantics='canonical_v1'`、`usage_present=1`、`cache_accounting_valid=1`、分母大于 0），因此同一本地模型跨服务商时先分别求和分子与分母、再计算一次 token 加权命中率，而服务商明细仍按 `local_model + provider_id + provider_name` 分离。`successful_request_count` 统计范围内成功终止行，`cache_rate_eligible_count` 只统计合格行：legacy、缺失、零分母与非法成功计入成功但不合格，失败、取消与无候选行两者都不计。

`SseUsageAccumulator` 同时接受 Chat Completions 顶层 `usage` 与 Responses 的 `response.completed.response.usage`，保留最后一个有效对象，并只以有界透传读取转发字节；网关绝不注入 `stream_options.include_usage`，缺失的 usage 对象记 `usage_present = false`。不修改任何请求或响应字节，`forwarding.rs` 保持不变。用量面板直接渲染后端 `cache_hit_rate_percent`（`null` 显示 `—`、真实的 `0.0` 显示 `0%`），在总览、模型与服务商层级把覆盖显示为合格数/成功数，并把指标名固定为中文“Token 缓存命中率”、英文“Token Cache Hit Rate”。

## Alternatives considered

- 保留前端基于原始 token 字段的公式：未采纳，因为服务商输入语义互不兼容，任何客户端算术都无法还原正确命中率，同一模型仍会按服务商得到不同的聚合结果。
- 用启发式把 legacy 行重新归类为 canonical：未采纳，因为存储中已丢失原始字段形态，启发式可能编造出看似合理却错误的命中率，而不是暴露覆盖率缺口。
- 用饱和减法修复非法或超大的嵌套缓存分量：未采纳，因为它会伪造出不可能的分解，可能重复计算缓存 token 并产生负的普通输入；非法形态应保持非法，只做保守计费回退。
- 为 Chat 流式请求注入 `stream_options.include_usage` 以提高覆盖率：未采纳，因为这会改动客户端请求体并可能破坏兼容的上游；缺失的 usage 应报告为不存在。
- 把缺失或非法 usage 渲染为 `0%`：未采纳，因为普通输入为正的真实未缓存请求本身合格且确实为 `0%`，而没有有效正分母时必须显示 `—` / `null`。
- 对模型或总览取各服务商预计算百分数的算术平均：未采纳，因为百分数不可相加；按层级求和分子与分母才能得到 token 加权命中率，两个 `80%` 的服务商汇总后仍是 `80%`。

## Consequences

- totals、时间分桶、模型行与服务商行共用同一个后端持有的 token 加权缓存命中率，因此同一本地模型由 OpenAI 风格与 Anthropic 风格服务商服务时也能正确聚合。
- `api_gateway_usage_stats` 保留命令名与既有字段；五个附加字段在每个层级都始终序列化。
- SQLite 迁移只新增列，legacy 行在日志、合计与历史花费中按存储原样可见，任何历史金额都不重算；回滚可还原解析器、聚合与界面，而附加列保持无害。
- 覆盖率是显式的：`cache_rate_eligible_count / successful_request_count` 把有效新数据与 legacy、缺失、零分母及非法请求区分开，`null` 是唯一无数据表示，`0%` 只保留给普通输入为正的合法未缓存请求。
- 流式转发字节与请求体保持不变；绝不注入 `stream_options.include_usage`，缺失的 usage 对象记作不存在。
- 部分取代：[API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) 保留并交叉链接；本记录只取代其记录的流式 usage 来源、缺失 usage 记 0 的行为与原始 token 档位语义，其中的 SQLite 日志、记录时价格冻结、保留、`group_by` 契约与 `local_model` 面决策仍然有效。
- `MEMORY.md` 与两个 `api-gateway` 导航职责描述这套归一化、schema、资格与覆盖契约，且 `navigation.md` 已按权威 JSON 重新生成。
