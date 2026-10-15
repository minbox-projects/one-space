# Agent Note: Native DeepSeek Cache Accounting, a Content-Free Observer and a Bounded Comparison

Status: implemented

[English](2026-10-15-deepseek-cache-diagnostics.md) | 中文

## Problem

DeepSeek 原生 Chat Completions 响应报告的 `prompt_tokens` 是包含式总量，并同时携带原生缓存子集 `prompt_cache_hit_tokens` 与可选的 `prompt_cache_miss_tokens`。`canonical_usage_from_value` 此前识别嵌套 OpenAI 缓存明细与 Anthropic 拆分档位，却不读取这些原生命名字段，因此原生形态的包含式输入 `100`、命中 `98`、未命中 `2` 被存成 `100` 个普通输入 token、`0` 个缓存读 token、`0%` 缓存命中率与按输入档位计费，尽管等价的嵌套形态早已被理解。该缺口使网关既无法解释上报的缓存命中，也无法在确认转发路径保留会话载荷且自身没有引入缓存丢失之前给出结论。已上报的症状——某会话上报的缓存读计数骤降而其输入持续增长——仍需要一种无内容泄露的方式来判断更早的消息、工具或隔离字段是否发生变化，以及在归因之前把网关与直连上游做有界对比的方式。

## Decision

`src-tauri/src/ai_gateway/usage_log.rs` 的 `canonical_usage_from_value` 现同时把原生 DeepSeek 缓存字段归一化进既有的四个互斥档位，复用既有的 checked 规范分解，而不是新增一条并行代码路径。present 的原生值必须是非负整数或既已支持的整数串形态；`null` 与缺失键都表示不存在，显式 `0` 表示 present。合法形态及其归一化为：

| Native and nested shape | Detection | Normalization |
| --- | --- | --- |
| Native-only | present `prompt_cache_hit_tokens`，且无嵌套缓存明细 | 原生命中成为缓存读档位 |
| Consistent dual | 原生命中与嵌套 `cached_tokens` 同时 present 且相等 | 命中只计一次 |
| Nested-only | 仅有嵌套 `cached_tokens` | 既有嵌套归一化保持不变 |

present 的原生 miss 必须等于报告的包含式输入减去已选缓存命中，因此仅有 miss 的响应绝不臆造缓存数量。为负、小数或畸形的原生值、原生与嵌套命中不一致、与 `input - hit` 矛盾的 miss，以及原生或嵌套命中与独立 Anthropic `cache_read_input_tokens` 档位并存，都会产生既有的保守回退：报告输入只按普通输入计费一次且不含任何缓存档位，该请求保持缓存统计不合格，且任何失败形态都不以饱和减法修复。包含式总量仍以 `input_tokens` 优先于 `prompt_tokens`，输出数量校验仍独立于缓存有效性。缓冲式 JSON 响应与增量观察的碎片化 SSE 流得到相同结果，缺失 usage 仍记 `usage_present = false` 而不是伪造的 `0%`，且不改变、不回填任何配置 schema、已存行或价格快照。核算未新增公开符号；原生 helper 保持 `usage_log.rs` 私有。

同一交付切片在不改动行为的前提下刻画了既有 Chat 转发契约：仅模型改写、原生 messages、有序工具定义、包含空串与显式 null 的 `reasoning_content`、工具调用与结果、提供的 `user_id`/`metadata` 与已识别会话头都能在网关映射后保留，上游鉴权保持既有处理。`forwarding.rs` 与 `selection.rs` 无需修复；若发现新缺陷，应提出有界后续请求，而不是未经授权的修改。

本次交付还新增独立的按需 Node 工具 `tools/diagnose-deepseek-cache.mjs`，仅使用内置模块，且不安装进常态网关运行时。其 `observe --listen <port> --gateway <loopback-url> --model <id>` 入口在 `127.0.0.1` 绑定并把请求与响应字节原样转发到命令行显式给定的回环网关；非回环的 `--gateway` 会被拒绝。它只针对所请求模型观察原生 `/v1/chat/completions` 与 `/chat/completions` 两条 POST 路由，按与亲和解析器相同的头名优先级取首个非空已识别会话标识，只保留恰好一个前序完成样本，并为每个选中的完成请求输出一行 JSONL `observation` 事件。事件报告 `append-only`、带首个变化 JSON 路径的 `changed`，或 `unavailable` 分类（`overlap`、`no-predecessor`、`oversized-metadata` 或解析/编码失败），并附带上游 `http_status` 与数值 `usage` 结果；`usage.availability` 及其 `reason` 与 `usage.fields.native` / `usage.fields.nested` 记录哪些来源存在。输出是元数据白名单：正文、消息与工具值、头值、密钥、会话标识、错误文本与内容哈希绝不打印或持久化，且 JSON/字符比较绝不声称度量分词器实际的缓存单元边界。该工具只是可选临时路径；操作者把一台选定客户端连接指向它，随后恢复原始选择。

观察器经本轮 review 的聚合修复加固，但其 CLI 契约与输出白名单不变。`--gateway` 同时接受 `http:` 与 `https:` URL，上游传输按配置的协议选择，因此同步的传输构造失败或异步 TLS 握手失败都会变成有界的 `HTTP 502`，同时进程与其 listener 存活、客户端得到应答而不是被丢弃。SSE 解析在累积 buffer 上把 `\r\n` 与 `\r` 规范化为 `\n` 并保留悬空的尾 `\r`，因此跨 chunk 切分的事件边界仍会折叠成真正的空行、被切分事件的 `usage` 仍被解析，`SSE_EVENT_LIMIT` 与转发字节均不变。overlap 从同会话 chat 请求开始传输的那一刻起跟踪，而不是等其 body 结束；诊断判决延迟到每个并发 peer 的 selected/unselected 状态都已知，而转发、管道与背压绝不延迟；overlap 是对称的，因此双方同时 selected 的请求都报告 `unavailable`/`overlap`、都不被保留为 predecessor、其后的顺序请求报告 `no-predecessor`，且不同 session、模型或路由绝不贡献 peer。保留的 body 样本仍以至多一个前序请求加上进行中的 selected 请求为界；nonselected、aborted 与 overlapped 的捕获会被释放。

同一工具的 `compare` 入口不带参数，地址与凭据只从六个进程环境变量 `CACHE_DIAG_GATEWAY_BASE_URL`、`CACHE_DIAG_GATEWAY_MODEL`、`CACHE_DIAG_GATEWAY_KEY`、`CACHE_DIAG_UPSTREAM_BASE_URL`、`CACHE_DIAG_UPSTREAM_MODEL` 与 `CACHE_DIAG_UPSTREAM_KEY` 读取；凭据绝不进入参数或输出，且不发现、不修改任何配置。它从配置的基础 URL 组装每个端点且不重复版本段，对固定的公开 256 KiB 前缀执行每条系列十条追加式请求体、`thinking: {"type":"disabled"}` 与 `max_tokens: 32`、每条路径两轮、每个完成系列两次不计入的 warmup 与八次测量，且最多 40 次模型 POST 尝试、无重试。它输出一行 `comparison` JSON：`attempts`、`status`（`completed` / `incomplete`）、恒为 `false` 的 `comparable`、恒为 `unverified` 的 `effectiveness`、`target_met`、为 `unknown` 的 `cost.availability`、时间戳，以及每条系列的 `warmup` / `measurement` 覆盖：`requests`（实际尝试次数）、`valid_usage`、token 求和、`cache_hit_rate`（包含式求和为 0 时为 `null`）与 `fields.native` / `fields.nested` 计数。完成的运行只表示一条有效测量序列，绝不表示两条路径服务同一账户或模型。配置缺失时以 `attempts: 0`、`status` 为 `incomplete`、`reason` 为 `missing-config` 结束。每次 `compare` POST 尝试都以逐请求超时为界：默认 `600000` ms，可由进程环境变量 `CACHE_DIAG_COMPARE_REQUEST_TIMEOUT_MS` 以正整数毫秒覆盖（缺失、非法、0 或负值回退默认且不使运行失败），到期时该尝试按既有 transport 结果结束、与任何其他传输失败完全相同并销毁 socket，该尝试仍计入其 phase 覆盖、序列停止、运行报告 `status` 为 `incomplete` 与 `reason` 为 `transport-failure`、后续轮次或路径不再尝试且不重试，同时该界限仅作用于 `compare`，`observe`、其流式与转发、六个 `CACHE_DIAG_*` 变量、fixture、40 次尝试上限、输出字段与元数据白名单均不变。

行为测试位于 `src-tauri/src/ai_gateway/tests/deepseek_cache.rs`（在 `src-tauri/src/ai_gateway/tests.rs` 中注册为 `mod deepseek_cache;`），以及 `tools/diagnose-deepseek-cache.test.mjs`。本轮修复在 Rust 文件中新增 fragmented nested-only 与 consistent-dual 两种流式刻画，在 Node 文件中新增三个 review 回归用例（https 构造/握手 `502`、跨 chunk 的 CRLF 事件边界、传输开始的 overlap）；修复后两套测试分别以 Rust 20 与 Node 41 通过，修复前的 18 与 38 记为基线。

## Alternatives considered

- 把原生字段当作独立 Anthropic `cache_read_input_tokens` 档位之后的第二回退：未采纳，因为原生命中与该独立读描述同一份缓存量，合并会重复计算，二者是冲突形态而非互补档位。
- 对原生字段复用通用的 `token_number` 强制转换：未采纳，因为它会把为负、小数或畸形的声称静默转换成看似合理的数量，而不是把核算标记为非法。
- 仅在出现 miss 时用 `input - prompt_cache_miss_tokens` 推导缓存读档位：未采纳，因为那会臆造上游并未上报的命中；仅有 miss 的响应绝不应产生缓存数量。
- 用饱和减法或钳制修复不一致的 miss 或超大的命中：未采纳，因为它会伪造出不可能的分解，可能重复计算或产生负的 token 计数；该形态应保持非法并只做保守计费回退。
- 修改 `forwarding.rs` 或 `selection.rs` 去追查疑似缓存丢失：未采纳，因为刻画结果表明既有契约已保留载荷，任何改动都会是交付证据之外未经授权的修改。
- 把观察器做成常态网关 feature flag 或安装式代理：未采纳，因为这是临时、可选、单会话的路径，绝不能改变生产运行时、持久配置或客户端安装。
- 为取得可比的前序样本而串行化重叠的选中请求：未采纳，因为它会改变被观察的传输并臆造客户端并未发送的序列；重叠应报告为 comparison-unavailable。
- 从请求内容或前缀哈希推断观察器的会话标识：未采纳，理由与亲和记录相同，因为标识只能来自客户端实际发送的请求头，且该工具报告差异路径而不是内容哈希。
- 把一次成功的 `compare` 响应或匹配的模型 id 当作账户等价的证明：未采纳，因为工具无法证明账户身份；等价性始终是由操作者提供、并经运行后日志核验的前提。
- 让 `compare` 自行报告成本或 token 估算：未采纳，因为该 runner 不计算成本、只记录上游上报的计数；成本显式记为 unknown，token 绝不从字符数推导。

## Consequences

- 新追加的请求日志行采用修正后的原生核算；既有行、历史 token、金额与价格快照保持不变且绝不回填。
- 四个互斥档位、已存的 `usage_semantics`/`usage_present`/`cache_accounting_valid` 列、共享合格谓词与 token 加权缓存命中率均不变；原生字段只喂给既有的缓存读档位。
- 诊断工具是临时且可选的：它不新增持久配置、schema 或后台服务，退出它即恢复操作者的连接选择，回滚会一并还原工具、其测试与交付文档，同时保留全部用量行。
- 核算只新增了私有 helper，因此导航因 tooling/test 与文档路径及其语义而更新，而不是因为新增公开符号。
- 本轮 review 的聚合修复只改变观察器的传输处理、拆分 SSE 规范化与 overlap 跟踪；Step 1 的原生核算契约、四个互斥档位、输出白名单与 CLI 契约均不变。
- 交付的工具路径 `tools/diagnose-deepseek-cache.mjs` 与 `tools/diagnose-deepseek-cache.test.mjs` 登记在 `cli-matrix-check` 的 tooling 导航条目，`docs/deepseek-cache-diagnostics.md` 登记在 documentation 条目，工具标准记入 `MEMORY.md`；`navigation.md` 已按权威 JSON 重新生成。
- 部分取代：[API Gateway Cache Hit Rate Normalizes Provider Usage Semantics](../bug-fix/2026-09-21-api-gateway-cache-hit-accounting.md) 被保留并交叉链接；本记录在其 `canonical_usage_from_value` 形态表中扩展原生 DeepSeek 命中/未命中字段并新增无内容泄露的观察器与有界对比 runner，而它的四档归一化、存储迁移、合格谓词与覆盖契约继续有效。
- 路由无取代：[Gateway Session Affinity Pins a Session and Model to One Upstream](../architecture/2026-09-20-gateway-session-affinity-routing.md) 被保留并交叉链接；本变更扩展原生字段核算与诊断，不改变亲和策略、其头名优先级、其绑定规则或其仅进程内存状态。
- 在两次经操作者授权的实时尝试后，Step 3 仍阻塞且未验证。两次都使用计划内的准备 helper（计划产物、非产品特性），只读取 `onespace-dev` 的 `.local_key` 与 `ai_gateway.json`，并在内存中把六个 `CACHE_DIAG_*` 值交给子进程；交付的 `compare` 入口仍只读其进程环境，且未发现或修改任何配置。第一次尝试（`2026-10-15T09:15:18.078Z`–`2026-10-15T09:15:21.111Z`，即 `17:15:18.078`–`17:15:21.111` UTC+08:00）以非零退出：`attempts: 1`/40、`status: incomplete`、`reason: request-rejected`——网关路径一次 HTTP 429 后即结束，warmup 与 measurement 均无有效 usage，上游路径未发出请求。第二次尝试在本次后续跟进中于合并后的工具上运行（`2026-10-15T12:16:02.608Z`–`13:27:14.150Z`，即 `20:16:02.608`–`21:27:14.150` UTC+08:00），以非零退出：`attempts: 23`、`status: incomplete`、`reason: transport-failure`——网关路径获得 16/16 测量覆盖、token 加权费率 ≈93.684%（按测量低于 98% 目标），直连上游路径在其首个测量请求上停滞、约一小时后被上游关闭，因此没有测量覆盖、也没有双路径对比。对第一次尝试窗口与目标身份执行只读 `onespace-dev` 用量查询，恰好找到一行即该被拒尝试（`result: failure`、`status: 429`、`usage_present: 0`、`cache_accounting_valid: 0`、各 token 档位 `0`、费用 `0.0`）。对第二次尝试窗口的同类只读查询恰好找到 20 行成功存储行（普通输入 94364、缓存读 843136、输出 640），与网关序列相符。第一次尝试后重新加载对比状态因固定的缺少来源 key 原因失败，密钥未变复检因此未完成、也不声称密钥值发生变化；网关另行因该 429 在目标 key 上留下自己的 runtime 配额标记，清除它超出本计划授权。两次尝试均未重试、未切换服务商、未追加 warmup、也未运行 live `observe`，两个批次均已用尽，后续测量需要新的显式授权且 fixture 不得更改；本次后续跟进同一变更交付的 `compare` 逐请求超时为每次尝试设界（默认 `600000` ms，可由 `CACHE_DIAG_COMPARE_REQUEST_TIMEOUT_MS` 覆盖），它撰写于第二次批次之后、未影响该批次；98% token 加权有效性目标仍未测量且不声称达标，原始生产缓存问题也不声称已解决。
