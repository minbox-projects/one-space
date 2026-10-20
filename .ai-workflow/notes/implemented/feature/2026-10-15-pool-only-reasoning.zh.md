# Agent Note: Model Pools Own a Bounded Pool-Local Reasoning Policy

Status: implemented

[English](2026-10-15-pool-only-reasoning.md) | 中文

## Problem

模型池公共别名的调用方无法请求推理偏好。模型池便携契约按字段拒绝每个无法识别的消息或请求控制，其中包含每个 `reasoning_effort`、`reasoningEffort` 与 `reasoning` 取值，因此想要更多或更少推理的调用方没有任何稳定、与服务商无关的方式来表达。每条被选中的候选都是一条真实的来源服务商映射，其原生 effort 标识各不相同且其来源配置必须保持不变，因此无论是一个共享的服务商字段还是全局策略，都无法表达“这个公共偏好映射到该候选的原生控制”。操作者也无处声明候选的原生能力、没有默认偏好，也没有任何逐次尝试中实际发送控制的证据。用户明确要求该特性只应用于模型池，并让既有上游服务商不受影响。

## Decision

- 公共偏好是规范、与服务商无关的 `low`、`medium`、`high`、`xhigh` 与显式 `none` 集合。Chat 接受 `reasoning_effort` 与兼容拼写 `reasoningEffort`；Responses 接受 `reasoning.effort`。取值先 trim 再与小写集合精确匹配，因此大小写变体不构成公共档位。缺失、null 与空白表示未指定；两个非空且相等的 Chat 拼写被接受，而冲突拼写、非字符串取值、未知标识与字面量 `auto` 在任何调度器、key、探测或健康变更之前返回点名该字段的 `400 invalid_request`。`none` 是显式禁用，`auto` 绝不是请求取值。
- 池默认是五个偏好之一或 `auto`，其中 `auto` 省略全部本特性控制并使用服务商默认；缺失的持久化默认读为 `auto`，且 `auto` 绝不作为 effort 字符串发送。显式请求偏好覆盖已存默认。新打开的池对话框初始选择 `medium`，操作者也可以改选 `auto`；绝不为了让这个初始默认有效而臆造任何映射。
- 每个新声明都位于 `model_pools[*].candidates[*]` 或可选的 `model_pools[*].fallback` 之下，默认位于 `model_pools[*]` 之下。`GatewayUpstreamProvider` 与 `ModelMapping` 都不新增持久字段，上游服务商编辑器也不新增控制。声明作用域限定在其池与完整候选身份：服务商 id、trim 后的原始 local/upstream 对与生效协议。引用同一来源的两个池保持彼此独立的策略。既有来源 `reasoning_efforts` 可以作为只读参考展示，但来源元数据取值绝不自动成为池映射，缺失声明保持未知，后续来源标签改动也绝不改写池自有声明。
- 映射把一个公共偏好关联到一个类型化原生动作，且只支持五个有界原生档位。多个正向偏好可以共享同一个正向原生动作；缺失映射意味着该候选不支持；正向偏好绝不可能指向禁用推理或省略；`none` 只能映射到显式声明且受支持的禁用；原生枚举目标必须属于该候选声明的集合。manual budget 是至少 `MIN_MANUAL_REASONING_BUDGET_TOKENS = 1024` 且落在候选声明、内部一致的范围内；manual budget 与 adaptive thinking 不能组合；仅开关档位的正向映射不携带强度梯度。不存在任意请求 JSON 注入机制。
- 全部档位、映射与默认都在既有串行配置事务内校验，因此非法提交保留原始加密字节。此前存储、后来缺失或改变协议的引用可以带可见原因保留，但绝不通过猜测或替换同级来源来服务它。
- 新建或改动的具体默认要求至少一个当前有效映射，而一次删除保留具体默认最后一个映射声明的保存必须在同一次保存中把该默认改为 `auto` 或改为一个拥有有效映射的其他默认；否则原子拒绝该保存。未改动的默认与声明可以在其来源变得不可用后保留，而对那个已声明但不可用偏好的请求返回 `502`。

| Profile | Target | Positive control | `none` control, only when declared supported |
| --- | --- | --- | --- |
| OpenAI effort | Chat Completions | `reasoning_effort: <native value>` | `reasoning_effort: "none"` |
| OpenAI effort | Responses | `reasoning: { effort: <native value> }` | `reasoning: { effort: "none" }` |
| Anthropic effort, inherit thinking | Anthropic Messages | `output_config: { effort: <native value> }`; omit only `thinking` | `thinking: { type: "disabled" }`; omit positive controls |
| Anthropic effort, adaptive thinking | Anthropic Messages | `output_config: { effort: <native value> }` and `thinking: { type: "adaptive" }` | `thinking: { type: "disabled" }`; omit positive controls |
| Anthropic manual budget | Anthropic Messages | `thinking: { type: "enabled", budget_tokens: N }` | `thinking: { type: "disabled" }`; omit the budget |
| Chat thinking switch | Chat Completions | `thinking: { type: "enabled" }` | `thinking: { type: "disabled" }` |

- 解析器、资格策略与原生注入器只在精确匹配当前池别名之后运行，绝不在普通路由上运行；禁用与退役别名、以及对池别名的原生 Messages 请求保持既有 fenced 行为。请求形状与推理在候选选择之前完成校验，一个具体、已解析的偏好按固定顺序分类：没有任何已配置常规候选或后备来源声明该偏好返回 `400 unsupported_reasoning_effort`；存在声明但没有一个相对其档位与当前来源身份/协议有效返回 `502 all_providers_unavailable` 并附有界非机密原因；全部有效映射都与请求的有效输出上限不兼容返回 `400 reasoning_output_limit_incompatible` 并点名输出上限字段与预算；否则余下候选按既有健康、key、协议与冷却规则过滤并至多追加一个兼容探测，若无可用的则结果为 `502 all_providers_unavailable`。Anthropic 预算必须严格低于有效上限，Chat `max_completion_tokens` 优先于 `max_tokens`，Responses 使用 `max_output_tokens`，省略的 Anthropic 上限沿用既有 4096 默认，网关绝不抬高调用方上限也绝不压低配置预算。`auto` 保留既有合格候选集且不注入任何东西。
- 每次尝试从原始请求剥离被识别的入口控制开始，用既有适配器转换剩余便携请求，然后只在既有池候选守卫之后注入恰好该候选自己的原生控制；绝不复用其他候选已改动的请求体。`anthropic.rs` 保持不变，因此普通 Anthropic 适配器保留其拒绝规则；既有 strict 工具约束、助手 `reasoning_content` 轨迹、首字节、成功标记与下游取消/不重放保证继续有效。
- 每次尝试都从与其上游请求体同源的构造取值记录持久、池专属的证据：既有 `reasoning_effort` 字段保留调用方的请求值（未指定时为 null），并追加两个新的可空字段 `resolved_reasoning_effort` 与原生控制快照 `{ protocol, mode, parameters }`。显式 `high` 解析为 `high`，省略且默认为 `medium` 解析为 `medium`，省略且为 `auto` 或遗留池解析为 `auto`。被尝试的 `auto` 记 `mode: "provider_default"` 且 parameters 为空，无尝试的合成行没有快照，普通与历史行保持 null/省略。快照只使用允许清单内的控制，绝不含量体、请求头或凭据。可空 SQLite 列经既有 `PRAGMA table_info` 幂等路径新增且不回填，token 计数、成本快照、金额、保留策略与单终止行规则不变。
- 静态目录只从当前存储配置派生：规范偏好顺序、逐候选静态可服务性（精确来源身份可解析、档位与存储生效协议匹配、映射结构合法）、池默认，以及每个偏好已映射候选数占常规候选加可选后备来源这一已配置并集总数的覆盖。冷却、key 可用性、health、来源启用状态与请求输出上限绝不进入这个配置计数；运行时就绪单独展示，常规候选状态数组与就绪计数仍只含常规候选，后备就绪是独立可选字段。发现 `/v1/models` 行、模型列表、状态载荷与 OpenCode 池行都发布同一目录。OpenCode 池变体把规范偏好同时用作变体名与 `reasoningEffort` 取值，绝不暴露原生映射值、借用来源 npm/服务商覆盖、臆造价格/余额/凭据，也不产生 `auto` 变体；无有效映射的遗留池保留其先前无变体形态；禁用与退役池仍被排除；手动同步、载荷比较、激活与投影所有权规则不变。普通模型行及其既有原生 effort 标识保持精确行为，Codex 默认选择与原生 Claude 池排除不变。
- 池编辑器拥有默认、每个候选的原生档位与取值/范围/预算/禁用声明以及五偏好映射矩阵。它提供显式未映射选项、标识未知能力与仅开关行为，并就地校验非法映射而不清空草稿；每条 manual budget 映射保留自己的原生 token 数，草稿取值被保留。被拒绝的保存保留整份草稿，取消不写，运行时刷新绝不覆盖未保存编辑。独立于就绪计算的静态覆盖与运行时就绪计数分开呈现；服务商编辑器不新增控制。日志 UI 为池行解释请求、解析与原生取值，而普通行保持其既有呈现，未知历史证据显示为不可用而非推断。标签、错误与控制支持中英文且可键盘访问。
- 兼容与隔离：只新增加性 serde 默认池字段，配置 schema 保持 3、无迁移，因此读取更旧配置既不重写它也不推断映射。池的创建、更新、启用/禁用与删除保留 `providers` 与服务商专属价格的语义取值，包括来源映射、efforts、keys、默认与运行时标记，并让另一个池不受影响。选择之前发生的映射或预算排除不创建尝试、也不变更健康。普通 Chat、Responses 与原生 Anthropic 路由、校验、发现与终端条目绝不调用池解析器或注入器，服务商 upsert、模板同步与普通 reasoning-effort 并集保持其契约。

## Alternatives considered

- 采用全局推理策略或在服务商或 `ModelMapping` 上新增持久推理字段：拒绝，因为用户要求池专属行为且既有上游服务商不受影响，来源类型、模板行为与服务商编辑器必须保持其契约，因此策略存储在每个池候选内，来源则保持只读参考。
- 在公共请求或 API 中直接暴露候选的原生 effort 取值：拒绝，因为调用方需要一个稳定、与服务商无关的偏好，而每个候选的原生标识各不相同；原生取值保持池私有，规范集合保持有界。
- 从来源 `reasoning_efforts`、模型名或自动探测推断候选的档位或能力：拒绝，因为来源元数据只是只读参考，猜测会误述真实上游能力；操作者必须显式声明档位与原生取值集合，缺失声明保持未知。
- 在没有池专属解析器与注入器的情况下复用普通 reasoning-effort 并集与普通适配器：拒绝，因为普通路径绝不能接受或发出这些控制，且每个候选都需要独立构造自己的原生请求体；池解析器、分类与注入器留在池别名之后，普通 Anthropic 适配器保持不变。
- 为池字段引入新的配置 schema 版本或迁移：拒绝，因为加性 `#[serde(default)]` 字段已使旧文件可读，所以 schema 保持 3 且无需迁移。
- 为获得响应而静默降级 `none` 或丢弃显式控制：拒绝，因为禁用请求必须禁用或失败，正向偏好绝不能悄悄变为省略；已声明但未映射的输入返回专属 `400`，断连声明返回 `502` 而非猜测。
- 抬高调用方输出上限或压低配置预算使候选变得兼容：拒绝，因为网关绝不能改写调用方请求或操作者配置的策略；不兼容候选被排除，全部不兼容的请求返回专属 `400`。
- 把 `minimal`、`max`、`ultra` 或字面量 `auto` 新增为额外公共档位：拒绝，因为公共集合是有界的规范五项，而 `auto` 只是默认/省略策略、不是请求取值；服务商专属原生标识仍可在声明的取值集合内使用其他拼写。
- 把明文 `reasoning_content` 轨迹表示为一个新的推理控制：拒绝，因为该轨迹是 provider 输出而非调用方控制，所以池策略保持既有同协议保留与转换省略行为不变，而不是把它变成可配置 effort。

## Consequences

- 池别名的调用方现在可以请求一个稳定的规范偏好，每条被选中的候选都收到自己显式配置的原生推理控制（含 fallback 与探测），而省略偏好时遵循池默认。该偏好表达相对的推理投入；它不承诺各选中模型具有同等智能、推理 token、延迟或成本。
- 普通上游服务商与普通路由不变：服务商配置、普通 Chat/Responses/原生 Anthropic 请求与错误、普通发现与普通终端条目保持其基线，池 CRUD 让服务商、价格与同级池在语义上完全一致。
- 配置与回退：schema 保持 3 且带加性 serde 默认池字段，旧池在被显式配置前使用 `auto`。新对话框初始的 `medium` 必须变得有效或改为 `auto`。因为旧二进制可能在保存时丢弃未知池字段且无法服务显式池 effort 请求，回到旧二进制需要保留同一 profile 的加密网关配置及匹配的本地 key 与对应的终端投影。新增的可空日志列与旧读取者兼容，历史用量绝不重算，不引入新的备份子系统或迁移框架。
- 限制与已知覆盖缺口：捕获的模拟请求与尝试到日志的比较证明的是确定性适配，而不是真实服务商会遵守声明的推理偏好，因此真实服务商合规仍未验证，且 adaptive thinking 的接受取决于服务商支持。真实原生 Tauri IPC、实时服务商推理、真实终端投影与交互式截图均未被演练，按未执行如实报告而非宣称完成；可选的视觉检查属可选，不作为验收标准的证据。
- 验证（后端）：前端工作之前的最终后端运行 `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::` 通过，910 通过、2 个既有被忽略的性能测试，退出 0。尝试适配的每个范围套件（两条 OpenAI 入站与三个目标的原生请求体捕获、预算/上限用例、缺失/变更来源、重试/探测兼容、普通路由与服务商取值相等、SQLite 重开）与捕获的尝试到映射传输守卫一同全绿。
- 验证（前端）：九个分步 vitest 文件通过，544 通过；对精确改动产品文件的 eslint 运行 0 错误；对精确改动测试文件的 eslint 运行 0 错误、34 个既有警告；`npm run build`（tsc 加 Vite）退出 0，带两条既有 Notes/Snippets 导入提示。构建与交互测试检测跨组件 DTO/导入/i18n 集成错误。
- 部分取代：本记录只在三处部分取代 [Model Pools Route One Public Alias Across Explicit Source Candidates](2026-10-13-free-model-pool.md)——该记录的池候选 schema 现在携带可选的池自有 `reasoning` 声明，其此前对每个推理控制的一律拒绝被有界的池自有解析器与映射取代，其“无池推理变体”的表述变为从有效映射发布的规范池变体；其来源身份、key 与健康结算、协议处理、刻意排除池会话亲和以及无真实价格来源的保证，以及其余每项决策，继续有效。本记录还在“该记录此前对每个入口控制的一律拒绝被新识别的推理控制收窄”这一点上部分取代 [Model Pool Portable Contract Recognizes the Assistant reasoning_content Trace](../bug-fix/2026-10-14-model-pool-reasoning-content.md)；其对助手 `reasoning_content` 轨迹的同协议 Chat 精确 JSON 保留，以及其在 Responses 与原生 Messages 转换上的省略且不臆造——全部四个轨迹行为——予以保留。既有普通终端推理决策未被改变。本记录还在池编辑器的 UI 路径及其默认适配器显式选择行为上被 [Model Pools Bind Supported Native Reasoning Levels Directly in the Candidate Editor](../simplification/2026-10-16-model-pool-reasoning-bindings.md) 部分取代：候选编辑器现在在其主面板中绑定规范偏好，而 profile、取值、范围与禁用字段移入默认收起的高级披露，本记录所述的全部路由、解析、隔离与默认守卫继续有效。
- Supersession: partial. [OpenAI 推理控制在 Anthropic Messages 转换中被翻译](../bug-fix/2026-10-17-openai-reasoning-anthropic-conversion.md) 仅在「`anthropic.rs` 保持不改，普通 Anthropic 适配器保留其拒绝规则」这一表述上部分取代本记录：普通适配器现在把被识别的 Chat 与 Responses 推理控制翻译为原生 `output_config` 与 `thinking`，而池路径仍在转换前剥离其被识别的入口控制并优先注入操作者声明的原生控制；本记录被保留并交叉链接，此处所有其他池决策继续有效。本记录还被 [Model Pools Use an Explicit Last-Stage Fallback Source](2026-10-20-model-pool-default-fallback.md) 部分取代，仅在于其已配置并集现在把可选最后阶段后备来源纳入请求分类、静态目录与 strict 工具校验；其解析器、原生档位、默认守卫、终端证据与普通服务商隔离继续有效。
