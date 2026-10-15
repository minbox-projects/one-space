# Agent Note: Native DeepSeek Cache Usage Is Normalized and the Chat Forwarding Contract Is Verified

Status: implemented

[English](2026-10-15-deepseek-cache-diagnostics.md) | 中文

## Problem

DeepSeek 原生 Chat Completions 响应报告的 `prompt_tokens` 是包含式总量，并同时携带原生缓存子集 `prompt_cache_hit_tokens` 与可选的 `prompt_cache_miss_tokens`。`canonical_usage_from_value` 此前识别嵌套 OpenAI 缓存明细与 Anthropic 拆分档位，却不读取这些原生命名字段，因此原生形态的包含式输入 `100`、命中 `98`、未命中 `2` 被存成 `100` 个普通输入 token、`0` 个缓存读 token、`0%` 缓存命中率与按输入档位计费，尽管等价的嵌套形态早已被理解。该缺口使网关既无法解释上报的缓存命中，也无法在确认转发路径保留会话载荷且自身没有引入缓存丢失之前给出结论。

## Decision

`src-tauri/src/ai_gateway/usage_log.rs` 的 `canonical_usage_from_value` 现同时把原生 DeepSeek 缓存字段归一化进既有的四个互斥档位，复用既有的 checked 规范分解，而不是新增一条并行代码路径。present 的原生值必须是非负整数或既已支持的整数串形态；`null` 与缺失键都表示不存在，显式 `0` 表示 present。合法形态及其归一化为：

| Native and nested shape | Detection | Normalization |
| --- | --- | --- |
| Native-only | present `prompt_cache_hit_tokens`，且无嵌套缓存明细 | 原生命中成为缓存读档位 |
| Consistent dual | 原生命中与嵌套 `cached_tokens` 同时 present 且相等 | 命中只计一次 |
| Nested-only | 仅有嵌套 `cached_tokens` | 既有嵌套归一化保持不变 |

present 的原生 miss 必须等于报告的包含式输入减去已选缓存命中，因此仅有 miss 的响应绝不臆造缓存数量。为负、小数或畸形的原生值、原生与嵌套命中不一致、与 `input - hit` 矛盾的 miss，以及原生或嵌套命中与独立 Anthropic `cache_read_input_tokens` 档位并存，都会产生既有的保守回退：报告输入只按普通输入计费一次且不含任何缓存档位，该请求保持缓存统计不合格，且任何失败形态都不以饱和减法修复。包含式总量仍以 `input_tokens` 优先于 `prompt_tokens`，输出数量校验仍独立于缓存有效性。

缓冲式 JSON 响应与增量观察的碎片化 SSE 流得到相同结果，缺失 usage 仍记 `usage_present = false` 而不是伪造的 `0%`，且不改变、不回填任何配置 schema、已存行或价格快照。没有新增公开符号；原生 helper 保持 `usage_log.rs` 私有。

同一交付切片在不改动行为的前提下刻画了既有 Chat 转发契约：仅模型改写、原生 messages、有序工具定义、包含空串与显式 null 的 `reasoning_content`、工具调用与结果、提供的 `user_id`/`metadata` 与已识别会话头都能在网关映射后保留，上游鉴权保持既有处理。`forwarding.rs` 与 `selection.rs` 无需修复；若发现新缺陷，应提出有界后续请求，而不是未经授权的修改。

行为测试位于 `src-tauri/src/ai_gateway/tests/deepseek_cache.rs`，并在 `src-tauri/src/ai_gateway/tests.rs` 中注册为 `mod deepseek_cache;`，覆盖原生-only、嵌套-only 与一致双形态、冲突、不可能与 miss 用例、显式零与 null 的区分、整数串形态、`input_tokens` 优先、输出数量独立性、碎片化 SSE、缓冲与流式持久化、不重复计费与响应字节保留。

## Alternatives considered

- 把原生字段当作独立 Anthropic `cache_read_input_tokens` 档位之后的第二回退：未采纳，因为原生命中与该独立读描述同一份缓存量，合并会重复计算，二者是冲突形态而非互补档位。
- 对原生字段复用通用的 `token_number` 强制转换：未采纳，因为它会把为负、小数或畸形的声称静默转换成看似合理的数量，而不是把核算标记为非法。
- 仅在出现 miss 时用 `input - prompt_cache_miss_tokens` 推导缓存读档位：未采纳，因为那会臆造上游并未上报的命中；仅有 miss 的响应绝不应产生缓存数量。
- 用饱和减法或钳制修复不一致的 miss 或超大的命中：未采纳，因为它会伪造出不可能的分解，可能重复计算或产生负的 token 计数；该形态应保持非法并只做保守计费回退。
- 修改 `forwarding.rs` 或 `selection.rs` 去追查疑似缓存丢失：未采纳，因为刻画结果表明既有契约已保留载荷，任何改动都会是交付证据之外未经授权的修改。

## Consequences

- 新追加的请求日志行采用修正后的原生核算；既有行、历史 token、金额与价格快照保持不变且绝不回填。
- 四个互斥档位、已存的 `usage_semantics`/`usage_present`/`cache_accounting_valid` 列、共享合格谓词与 token 加权缓存命中率均不变；原生字段只喂给既有的缓存读档位。
- 只新增了私有 helper，因此导航条目因路径与语义而更新，而不是因为新增公开符号。
- 部分取代：[API Gateway Cache Hit Rate Normalizes Provider Usage Semantics](../bug-fix/2026-09-21-api-gateway-cache-hit-accounting.md) 被保留并交叉链接；本记录在其 `canonical_usage_from_value` 形态表中扩展原生 DeepSeek 命中/未命中字段并新增转发刻画，而它的四档归一化、存储迁移、合格谓词与覆盖契约继续有效。
- 路由无取代：[Gateway Session Affinity Pins a Session and Model to One Upstream](../architecture/2026-09-20-gateway-session-affinity-routing.md) 被保留并交叉链接；本变更扩展原生字段核算与诊断，不改变亲和策略、其头名优先级、其绑定规则或其仅进程内存状态。
- 本记录不交付 Step 2 与 Step 3：独立观察/对比工具尚不存在，计划中的 `CACHE_DIAG_*` 环境变量均未定义，且没有任何实时对比运行，因此 98% 的 token 加权有效性目标仍未测量，也不声称已达标。随着这些步骤落地，本记录将继续扩展。
