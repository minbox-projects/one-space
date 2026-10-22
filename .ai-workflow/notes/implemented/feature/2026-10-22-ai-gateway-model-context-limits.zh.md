# Agent Note: AI Gateway Model Context Limits Are Declared Metadata

Status: implemented

[English](2026-10-22-ai-gateway-model-context-limits.md) | 中文

## Problem

网关此前没有自有的事实来源来保存每个模型的总上下文窗口。终端集成需要这份元数据，但从模型名、价格、完成上限或第二个目录推导会发布猜测值，可能超过服务商实际能提供的容量。同一个公开模型可以由多个已配置来源服务，因此只有当每个参与来源都声明了已知上限时，单一对外值才是安全的；继承副本也不能让陈旧编辑器或健康状态迁移凭空造出更大的值。

## Decision

`ProviderTemplateModel` 与 `ModelMapping` 现在携带可选的 `context_window: Option<u32>`，以总上下文 token 计量。两个字段都使用 serde 默认且在缺省时省略序列化，因此旧配置文件读取为未指定而非 0，配置 schema 保持版本 4 且无需迁移。由于持久化类型是 `u32`，负数、小数与超过 `u32` 的声明在解码时被拒绝；提交的 0 在任何写入之前被拒绝。

模板绑定服务商对该字段为只读。映射跟随最新的有效模板，按 trim 后大小写敏感的上游模型 id 与既有首个匹配规则匹配，覆盖别名、用户禁用与自动禁用行。手动模板保存与 `ai_gateway_upsert_provider` 都以最新模板规范化提交的绑定值，因此陈旧对话框无法建立覆盖。创建、恢复与重新绑定继承当前模板值；解绑保留最后一次继承值以供后续手动维护；移除模板中的某个模型只清除该孤立的继承值，不改变既有的映射退役行为；模板不含该模型时为未指定。

远程模型清单只为声明解码。顶层 `context_length` 与嵌套 `limit.context` 各接受正整数或显式 `null`；两者同时声明时必须一致，包括同为 `null`。缺失或为 `null` 的 `limit` 父级，或缺少 `context` 的对象，不声明嵌套字段；其他任何非对象 `limit` 均非法。声明只在既有的隐藏条目、协议不支持与重复首项过滤之后校验，因此被拒绝条目的畸形元数据不会让同步失败。非法声明类型、非正值、小数或溢出会让整次同步失败，不发布任何模板、映射或时间戳。无声明保留同一模型 id 的旧值（新模型保持未指定），显式 `null` 清除该值，合法声明替换此前的值。上下文绝不从价格、输入或输出上限、最大完成 token 或模型名推导。

`src-tauri/src/ai_gateway/context_limits.rs` 以 `pub(in crate::ai_gateway)` 可见性提供 `resolve_public_context_window(&GatewayConfig, &str, UpstreamProtocol) -> Option<u32>`。它聚合跨服务商的普通别名、每个池候选以及展开的有序后备来源，并按服务商、本地模型、上游模型与生效协议组成的精确来源身份去重。只有当每个参与来源都声明已知正值时，聚合值才是它们的最小值；任何未知或无法解析的已配置来源都会让聚合变为未知。已配置但禁用与自动禁用的来源可以保守贡献，而 Key 标记、冷却、运行时健康与恢复绝不抬高对外上限。池别名在原生 Anthropic Messages 入站下不解析，`default_model` 后备路由只从同一服务商的精确上游声明解析。该解析器只读取声明式配置，不触碰路由或网络。

原生终端适配只在各工具既有的写入路径上消费该解析器。OpenCode 生成模型把保守聚合值写成数值 `limit.context` 并固定 `output: 0`，聚合未知时整行不写 `limit` 对象；既有自由形式 `tool_config.models` 渲染整个模型对象，因此不新增伴随字段或渲染器改动。Codex 同步在既有手动激活/投影路径把当前选中模型的聚合值写成顶层整数 `model_context_window`，聚合未知时记录显式 `null`，使经合并的 canonical 记录清除陈旧标量。Claude 同步对默认选择与三个 family 选择去重后取最小值：已知最小值不低于 100000 时写成十进制字符串 `CLAUDE_CODE_AUTO_COMPACT_WINDOW`，上限 1000000；低于下限的最小值、未知值或不完整选择会清除该受管键并报告原因，压缩本身保持启用，全局渲染器与承载创建和恢复的隔离 profile materializer 写同一值。每个终端目标暴露带 `mode`、`scope`、`TerminalContextCoverage` 计数与稳定原因码的 `TerminalContextSummary`；它绝不携带凭据，也绝不声称真实运行中的 CLI 已消费该值。Codex 汇总复用生成时同样保留的已启用池别名来解析选中模型，因此 `selected_model` 与 `applied_context` 与生成的选择一致。

操作者界面复用声明值辅助函数而不是另行推导，但保留一处有意的显示差异。`src/lib/aiGateway.ts` 导出声明值辅助函数 `knownContextWindow`、`parseContextWindowInput`、`formatContextWindow`、`resolveTemplateContextWindow`、`resolveOrdinaryContextWindow`、`resolvePoolContextWindow` 与 `resolveMappingContextWindow`，以及 `1`–`4294967295` 值域、`100000` 下限与 `1000000` 上限，和 `GatewayTerminalContextCoverage`、`GatewayTerminalContextSummary` DTO。`resolveOrdinaryContextWindow` 与 `resolvePoolContextWindow` 有意跨全部已配置映射、池候选与展开后备取来源级保守最小值，与入站无关；逐终端投影与汇总则按各自入站的协议可用性求值，例如 OpenCode 行在 Chat 与 Responses 之间的选择，或池别名在原生 Anthropic Messages 入站下不解析。由于展示值是超集上的最小值，它不会高估单入站值，但可能在入站专属投影已知时报告未知。`ProviderTemplateEditDialog` 为每个模板模型提供带标签的 token 输入，并说明合法来源声明覆盖手写值、缺失声明保留手写值、显式 null 或空白清除该值。`ProviderDetailDialog` 为未绑定映射提供同样的标签输入，对绑定映射只读展示继承值及其来源，并在模板或模板模型缺失时明确说明；非法草稿保持可见并阻止保存，取消不写入任何内容。`ModelListPanel` 为普通行与池行展示逐来源值以及保守聚合值或「不完整—未知」状态，池没有独立的可编辑覆盖。`TerminalSyncPanel` 为 OpenCode、Codex 与 Claude 每个目标展示 `mode` 与 `scope`、已知与总数覆盖、选中模型、原始与应用 token、未知/低于下限/封顶/选择不完整的稳定原因，以及仅新会话或重新物化会话生效的范围，且绝不声称真实运行中的 CLI 已消费该值。`ClaudeModelSelectionDialog` 在同步前展示共享阈值、封顶与不可用原因。成功的手动模板保存会重载服务商配置与终端目标。两侧语言都提供标签与可访问名称，隐藏面板行为与轮询节奏保持不变。

## Alternatives considered

- 从模型名、价格、输出上限或最大完成 token 推导窗口：未采纳，因为这种值是猜测，可能高估服务商的真实容量。
- 允许绑定服务商保留手动覆盖：未采纳，因为陈旧对话框或较旧模板可能建立与权威模板不一致的值。
- 在另一来源未知时公布最大或首个已知来源值：未采纳，因为公开模型不得声称超过每个已配置贡献者所能支持的范围。
- 把远程声明的缺失当作 0、无限或清除：未采纳，因为缺失保留既有值，只有显式 `null` 才清除。
- 增加第二个模型目录或配置迁移：未采纳，因为模板与映射已经拥有该元数据，而加性可选字段让 schema 版本 4 继续可读。

## Consequences

- 既有加密配置保持可读并呈现未指定上下文；值仅在存在时往返，较旧构建在后续保存时可能丢弃它们。
- 远程刷新对该元数据是原子的：畸形或冲突声明保持先前的配置与同步时间戳不变，而合法、缺失或 `null` 声明分别遵循替换、保留或清除规则。
- 绑定行无法偏离其模板，解绑会保留最后一次继承值，直到操作者编辑已解绑的映射。
- 对外公布是保守的：完整聚合报告所有已配置贡献者的最小值，任何必需的未知都让聚合保持未知，而不是借用更大的值。
- [Provider Templates Drop Built-in Model Catalogs and Prices](../architecture/2026-09-19-provider-template-manual-model-sync.md) 的模板模型形状与远程同步合并事实被可选声明字段 `context_window` 及其归属规则部分取代；其精简目录、按模板手动同步、定价与退役决策继续有效。
- `MEMORY.md` 与权威导航索引在同一变更中登记后端模块、前端上下文辅助函数与终端汇总 DTO，以及对应的行为测试路径；生成的导航视图与 Note 配对记录同时刷新。[API Gateway Model List Tab](2026-09-18-api-gateway-model-list-tab.md) 现在也承载逐来源与保守上下文值，而其过滤、复制与聚合决策继续有效。
- 原生配置既可加性写入也可清除：OpenCode 未知值省略 `limit`，Codex 未知值写显式 `null` 使经合并的 canonical 记录清除陈旧标量，网关受管重置会随上下文键一起移除 `model_reasoning_effort` 与其他网关自有键、保留真正无关的原生键，只返回渲染内容并由既有投影应用路径持久化，而 Claude 的不可用、低于下限或封顶值只清除或收窄受管键且压缩保持启用；无关原生字段与既有归属、标记和激活规则不变。
- 仅上下文漂移现在也参与 `pending_sync`：OpenCode 模型清单缺少当前上限、Codex `model_context_window` 不一致，或 Claude 选择或压缩阈值不一致，都会把此前同步过的目标标记为待同步且不暴露 Key 材料。改变上下文的服务商保存或手动模板保存会在落盘后于锁外刷新此前同步过的工具一次，刷新失败绝不改变已保存的网关结果。
- 行为覆盖在原生投影侧位于 `src-tauri/src/ai_gateway/tests/context_terminal_sync.rs` 与 `src-tauri/src/app_store/tests/gateway_context_projection.rs`，在操作者界面侧位于 `src/components/AiGateway/ContextLimits.test.tsx` 以及既有 AiGateway 交互套件。它们使用隔离的临时 home、注入来源与 Testing Library，因此原生 GUI 交互与真实运行中的第三方 CLI 仍未执行。
- [Template Sync Best-Effort Refreshes Previously Synced Terminal Tools](2026-09-23-template-terminal-resync.md) 被部分取代：改变上下文的服务商保存与手动模板保存现在也会触发其一次性的 best-effort 刷新，而其模板同步触发、已同步工具谓词、顺序与吞错契约继续有效。[Gateway Terminal Sync Flags Model-Selection Drift as Pending](../bug-fix/2026-09-29-gateway-terminal-pending-model-drift.md) 被新增的 Codex 上下文字段与 Claude 选择/阈值比较部分取代，而其台账规则、Key 材料排除与只读计算继续有效。
- 操作者界面已交付：模板与未绑定映射的 token 输入、带缺失模板说明的只读继承值、模型列表的逐来源与保守值、带原因与会话范围的逐终端汇总、同步前的 Claude 阈值视图、双语与可访问标签，以及模板保存后重载服务商与终端目标。限定前端套件 300/300 通过，生产构建与限定 ESLint 退出 0 并有 3 个警告，有界网关套件 1151 例通过、0 失败、46 例既有忽略；生成配置与 Testing Library 断言不证明原生 GUI 交互，也不证明真实运行中的第三方 CLI 已消费该配置。
