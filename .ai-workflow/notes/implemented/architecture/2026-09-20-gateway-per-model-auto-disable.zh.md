# Agent Note: Gateway Per-Model Auto-Disable Settles Health on the Mapping Row

Status: implemented

[English](2026-09-20-gateway-per-model-auto-disable.md) | 中文

## Problem

网关此前只在服务商级表达自动可用性：失败后服务商的 `auto_disabled` 标志会一次性把该服务商的所有模型移出候选选择、`GET /v1/models`、聚合视图与终端同步模型清单，且唯一的恢复路径是整体清除该服务商的状态。上游服务商通常提供多个健康状态彼此独立的模型，因此一个模型故障会静默移除其健康的同类模型，而结算出的失败也无法归因到具体模型。此前交付的逐映射 `enabled` 标志表达的是用户排除某一行的意图，运行健康却没有行级状态可结算：操作者既看不到是哪个模型在失败，也无法只恢复该模型。

## Decision

`ModelMapping` 在用户的 `enabled` 之外新增五个 serde 默认的运行时字段——`auto_disabled`、`disabled_reason`、`disabled_at`、`consecutive_failures` 与 `last_error_at`；旧 `ai_gateway.json` 将每一行读取为健康。运行健康由 `selection::MappingTarget` 标识，即服务商 id 加 trim 后的 `(local_model, upstream_model)` 组合，并通过 `register_mapping_failure`、`register_mapping_success`、`clear_mapping_runtime_state` 与 `mapping_matches_key` 结算。`MappingTarget::for_request` 解析一次已服务尝试所属的行，并在请求模型为空、未命中任何行、或该尝试由服务商 `default_model` 提供时返回 `None`；此类尝试因此绝不记录任何健康结果。

结算在行级沿用既有失败分类与计数。`runtime_http::RequestHealth` 仍按每个入站请求、每行累计一个结果，并在请求正常结束时应用一次：连续 3 个失败请求（`FAILURE_THRESHOLD`）禁用该行；仅 404、临时 429 与其他 4xx 绝不作为健康失败计数；由该行完成的请求把其计数与最近错误值归零，且绝不触碰同服务商的其他行。自密钥池变更起，401/403 与额度耗尽 429 属 key 域：标记所尝试的上游 key 并在请求内轮换，而不在映射行上结算（[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](2026-09-24-gateway-key-pool-rotation.md)），因此计数始终只表示连续可重试失败。入站请求的本地模型名会被带到每个结算点，因此重试尝试的失败会记录在它实际使用的行上。

带 `auto_disabled` 的行在映射被选择或暴露的每一处都与 `enabled = false` 的行完全同等对待：`selection::resolve_model_for_protocol` 跳过它，因此它绝不作为健康候选参与服务、绝不产生 `ProtocolMismatch`、其 `upstream_model` 也绝不被转发；唯一例外是 [Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast](2026-09-23-gateway-auto-disable-recovery.md) 记录的冷却半开探测，它在全部健康候选与有限重试都失败之后恰好转发一次该行的 `upstream_model`，或在存在合格探测时作为零候选请求的唯一尝试；请求只命中被用户禁用、或没有合格探测的自动禁用行时返回无候选（`all_providers_unavailable`），而不是回退 `default_model`；`GET /v1/models` 所用的 `runtime_http::local_model_names`、终端同步所用的 `commands::build_gateway_provider`、以及前端的 `aggregateModels` 都跳过自动禁用行。类型中已不存在任何服务商级运行字段——服务商谓词仅为 `enabled`，而映射行仅在其 `enabled && !auto_disabled` 时贡献。

恢复以行为范围。`ai_gateway_reenable_provider_model(provider_id, local_model, upstream_model)` 清理与 trim 后键匹配的行的运行时状态，重复匹配键一并清理；`ai_gateway_reenable_provider_models(provider_id)` 清理某个服务商的全部自动禁用行。两者都返回更新后的配置、绝不改变任何行的 `enabled` 意图（被用户禁用的行保持禁用），并在服务商或键不匹配任何内容时返回可操作错误且不写盘。服务商级 `ai_gateway_reenable_provider` 已移除。自 [Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast](2026-09-23-gateway-auto-disable-recovery.md) 起，被阈值禁用的行也可无需操作者介入恢复服务：其 60 秒冷却结束后，若请求无法被任何健康候选服务，冷却半开探测会尝试它一次，成功即清除其运行状态；旧构建写入、被 401/403 即时禁用的行仍保持仅手动恢复，而新的鉴权失败属 key 域、绝不禁用映射行（[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](2026-09-24-gateway-key-pool-rotation.md)）。

类型中已不存在服务商级运行时状态：版本门控迁移在改写早于当前 schema 版本的配置时一次性清除遗留的服务商级键（[Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md)），此后没有任何路径能写入服务商级运行时状态。`ai_gateway_upsert_provider` 仅在映射行 trim 后的 `(local_model, upstream_model)` 键不变时保留其运行时状态，新键或变更键从健康开始，被删除行的状态随替换一起消失；模板创建与模板同步不写入任何运行时状态，因此它们产生的每一条映射都从健康开始。

`GatewayStatus.auto_disabled_count` 现在统计自动禁用的映射行数而非服务商数。`RuntimeStatusCard` 按 `enabled` 单独统计服务商数、自动禁用模型数取 `status.auto_disabled_count`；服务商卡片展示其自动禁用行的只读数量提示；`ProviderDetailDialog` 以 `data-auto-disabled` 标记自动禁用行（与用户禁用的 `data-disabled` 视觉可区分），提供逐行重新启用操作，并在该服务商存在自动禁用行时提供服务商级「重新启用全部」。文案以中英双语位于 `src/i18n.ts` 的 `aiGatewayProviderAutoDisabledModelsHint`、`aiGatewayReenableMapping` 与 `aiGatewayReenableAllMappings`。

## Alternatives considered

- 保持服务商级自动可用性、在某个模型失败时下线整个服务商：未采纳，因为该服务商的健康模型随后停止服务，而结算出的失败可归因到单个模型，映射行本已提供表达这一点的粒度。
- 任一单行跨过阈值就禁用整个服务商：未采纳，原因相同；服务商级禁用还会掩盖是哪个模型失败，且只能通过服务商级恢复路径清除。
- 删除失败行或把其 `enabled` 置为关闭：未采纳，因为运行健康必须与用户意图保持独立，暂时的上游故障绝不能改写用户并未要求的排除，且删除会丢弃该行的远端模型、协议与显示名。
- 同时保留服务商级与行级计数器作为两个并行事实来源：未采纳，因为同一种情况绝不能有两个归属；服务商级字段仅为反序列化兼容而保留，如今已被移除，遗留键由版本门控迁移清除。
- 把 `default_model` 尝试结算到服务商或任意一行：未采纳，因为此类尝试没有行身份；这样结算会禁用该请求从未使用过的行。
- 保留 `ai_gateway_reenable_provider` 作为唯一恢复入口：未采纳，因为恢复必须针对失败的那个模型；服务商级辅助命令仅以 `ai_gateway_reenable_provider_models` 保留，用于清理某个服务商的全部自动禁用行。

## Consequences

- 失败的模型现在单独消失：结算后只有自动禁用行退出候选选择、`GET /v1/models`、聚合视图与终端同步模型清单，而该服务商的其他行继续服务；该行保持用户启用状态，在其模型恢复且延迟的半开探测清除其运行状态、或操作者按行或按服务商清除后即可重新服务。
- `default_model` 尝试绝不记录健康结果，因此经未映射回退完成的请求既不能禁用任何行、也不能清零任何行。
- 旧配置由版本门控迁移一次性升级：所有新字段都有 serde 默认，服务商级遗留运行键从改写后的文件中清除，回退后的构建在 best-effort 降级下忽略行级字段。
- `auto_disabled_count` 的含义由服务商变为映射行；运行时状态卡、服务商卡片、服务商对话框与新增文案随之更新，前端以 `data-auto-disabled` 标记自动禁用行并调用 `ai_gateway_reenable_provider_model` 或 `ai_gateway_reenable_provider_models`，`aiGatewayReenableProvider` 封装已删除。
- Supersession（取代评估）：部分取代。[Gateway Per-Model Mapping Disable Is an Explicit Exclusion](2026-09-18-gateway-per-mapping-disable.md) 被保留并交叉链接：其「由上游健康推导可用性并自动禁用单个映射」的被否决备选如今在行级成为决策，而其显式排除规则、用户 `enabled` 意图与服务商状态的独立性、以及 `default_model` 阻断规则继续有效。[API Gateway Aggregated Models Open in a Dialog With a Shared Count](../feature/2026-09-17-api-fusion-aggregated-models-dialog.md)、[API Fusion Terminal Sync Writes an Independent Gateway Provider](../feature/2026-09-17-api-fusion-terminal-independent-provider.md) 与 [API Gateway Model List Tab Reuses the Shared Aggregation](../feature/2026-09-18-api-gateway-model-list-tab.md) 被保留并交叉链接，因为本次变更修正了它们记录的候选谓词——服务商谓词仅为 `enabled`，行在被用户禁用或自动禁用时跳过——而它们的聚合、独立服务商与模型列表决策保持不变。弹框记录所记载的弹框界面由 [API Gateway Aggregated Models Open the Model List Tab Instead of a Dialog](../simplification/2026-09-21-gateway-aggregated-models-open-model-list-tab.md) 部分取代，后者同样被保留并交叉链接，而聚合与模型列表决策仍然成立。[Gateway Session Affinity Pins a Session and Model to One Upstream](2026-09-20-gateway-session-affinity-routing.md) 因候选过滤措辞的修正、[Gateway Template Model Retirement Disables Derived Mappings](2026-09-20-gateway-template-model-retirement.md) 因取代关系措辞的修正被保留并交叉链接；它们的决策继续有效。[Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast](2026-09-23-gateway-auto-disable-recovery.md) 被保留并交叉链接，作为只部分取代本记录「恢复仅手动」陈述、其此前 401/403 计数规则、以及其对自动禁用行「任何尝试都被排除」的未加限定表述的记录，而行级运行时状态、阈值规则、结算点、手动恢复命令与弹窗界面继续有效。用量统计、请求日志、模型价格、服务商模板、快速失败、逐尝试日志与双语 Notes 记录互不相关，因此没有任何记录被取代。密钥池变更取代本记录「401/403 立即禁用该行」与「被 401/403 禁用的行保持仅手动恢复」的陈述：鉴权失败现结算到所尝试的 key 并在请求内轮换，由 [Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](2026-09-24-gateway-key-pool-rotation.md) 记录；行级运行时状态、阈值规则、结算点、手动恢复命令与弹窗界面继续有效。部分取代：[Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md) 只取代本记录把服务商级运行字段作为读取时清空的反序列化兼容保留的决定及其 `resolveMappingPreview` 引用；本记录的行级运行状态、结算、恢复与谓词决定仍然成立。
- `MEMORY.md` 与 `navigation.json` 的 `ai-gateway` / `ai-gateway-backend` 条目在同一变更中承载行级运行时状态语义，`navigation.md` 已按权威 JSON 重新生成。
