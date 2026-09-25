# Agent Note: Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth

Status: implemented

[English](2026-09-24-gateway-key-pool-rotation.md) | 中文

## Problem

上游服务商此前只能持有一个凭据：401/403 会立即禁用映射行，而耗尽或被吊销的 key 只能靠操作者手工编辑服务商来更换；同时，按项目、账号或额度周期签发多个 key 的服务商没有办法让这些 key 保持有序并持续服务。多 key 密钥池还必须在失败时保持可预期：耗尽或被拒绝的 key 绝不能被无限重试，恢复既不能消耗有限的重试预算、也不能扭曲映射行健康，操作者还必须能看出哪个 key 已停用并有意恢复它。因此本次变更必须决定：密钥池如何服务流量、被标记的 key 何时可以自行恢复、以及哪些失败保持手动。

## Decision

上游服务商持久化一个有序密钥池（`GatewayUpstreamProvider.keys: Vec<UpstreamKey>`，schema 版本 2），列表顺序即用户可见的优先级。每个条目携带稳定 id、必填非空名称、值、用户 `enabled` 标志与运行时标记字段（`auto_marked`、取值为 `authentication` 或 `quota` 的 `failure_kind`、`marked_at`、`reason`），且所有字段始终序列化。每当网关准备一次尝试时，它按列表顺序选择第一个启用且未被标记的 key，因此流量固定落在优先级最高的可用 key 上，而不是在池中分散，也不会在失败后自行重排。

轮换只限两种 key 专属失败类别。401/403 把所尝试的 key 标记为 `authentication`，额度归类为耗尽的 429 标记为 `quota`，随后同一请求立即在下一个可用 key 上继续，不等待退避、不消耗每请求重试预算、也不登记映射行健康；流式请求仅在写出首个客户端字节之前轮换。临时 429、网络错误、5xx、404 与其他 4xx 绝不标记任何 key、也绝不轮换。标记持久化在加密配置中，因此重启绝不会重试鉴权失败的 key，额度冷却也跨重启保留。

恢复基于冷却探测或显式操作。只有当不存在启用且未标记的 key 时，请求才会在 `KEY_PROBE_COOLDOWN_SECS = 60` 之后探测最多一个合格 key，按最旧 `marked_at` 优先、平手按列表顺序；探测成功即清除标记并用该 key 服务，失败则重新计时。`authentication` 标记的 key 绝不被探测：它们仅通过 `ai_gateway_reenable_provider_key(provider_id, key_id)` 或编辑其值恢复，两者都清除运行时标记且绝不改变用户 `enabled` 标志。密钥池没有可用 key 也没有合格探测时，该服务商被视为不可用：选择跳过它且不触碰其映射行或计数，请求沿用既有 fallback 或标准 `all_providers_unavailable` 路径。空 key 列表允许保存，并按同一规则使该服务商不可服务：它没有任何可选或可探测的 key，因此被跳过、绝不以空凭据尝试，也不会出现错误循环。

服务商 upsert 按 key id 保留运行时状态，空白编辑值保留已存值，改值清除运行时状态，重命名、重排、启用或禁用保留运行时状态；空名称与新 key 空值被拒绝，空列表被接受。错误脱敏覆盖密钥池中每个非空值。

## Alternatives considered

- 在池中按轮询或最近最少使用分散流量：未采纳，因为列表顺序是用户的优先级；请求必须停留在优先级最高的可用 key 上，失败历史也绝不能静默改写顺序。
- 高优先级 key 被标记时把低优先级 key 提升到最前：未采纳，因为那会让密钥池行为取决于失败历史并掩盖用户可见优先级；固定顺序改为依赖手动重新启用与额度探测。
- 在仍存在未标记可用 key 时探测被标记的 key：未采纳，因为探测是恢复兜底而不是健康检查；正常服务期间探测会为不应承载流量的 key 花费请求，并模糊优先级契约。
- 像额度标记一样探测 auth 标记的 key：未采纳，因为被拒绝的凭据不经操作者处理无法恢复；探测只会增加延迟与日志噪音，并向上游重复发送已知无效的凭据。
- 保留映射行 401/403 即时禁用与额度 429 映射健康计数：未采纳，因为失败属于凭据而非模型；key 域结算让映射健康仍然表示模型可用性，并让同一请求在有效 key 上继续。
- 在后台测试鉴权 key 并自动重新启用：未采纳，因为那会向上游账号施加非请求产生的流量，而且没有依据假定被吊销的凭据已恢复有效。

## Consequences

- 轮换有界：一个请求最多尝试可用 key 数加一次探测，key 域失败不等待退避、不消耗重试预算，也绝不污染映射健康、用量统计或终端同步。
- auth 与 quota 标记持久化并跨重启保留；auth 标记的 key 在用户重新启用或改值之前一直停用，quota 标记的 key 在其 60 秒冷却后或经手动重新启用恢复。upsert 对未变 id 保留运行时状态，新 key 或改值 key 从健康开始。
- 服务商卡片汇总 key 名称与状态，对话框编辑器支持新增、重命名、改值、删除、重排、启用、禁用与单个手动重新启用，空白编辑保留已存值、改值清除运行时状态。
- 错误脱敏覆盖密钥池中每个非空 key 值，整池的值绝不进入日志、错误文本或终端配置。
- 验证：行为测试覆盖有序选择、401/403 与额度轮换且映射健康不受影响、临时失败不轮换、冷却边界与探测选择、服务商跳过（含空池：不可服务、绝不被尝试且无错误循环）、upsert 保留与整池脱敏；前端覆盖编辑器、卡片汇总与双语文案。
- Supersession（取代评估）：部分取代。[Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast](2026-09-23-gateway-auto-disable-recovery.md) 与 [Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](2026-09-20-gateway-per-model-auto-disable.md) 被保留并交叉链接；本记录只取代它们「401/403 立即禁用映射行」「被鉴权禁用的行保持仅手动恢复」「额度耗尽 429 计入映射健康」的陈述，而阈值探测、恢复宽限、行运行时状态与手动恢复命令继续有效。[Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md) 被扩展而非取代：schema 版本 2 把单凭据转换加入其版本门控迁移。[CommandCode Provider Cards Show Account Quota](../feature/2026-09-23-commandcode-provider-quota.md) 与 [OpenCode Go Provider Cards Show Usage](../feature/2026-09-24-opencode-go-provider-usage.md) 被保留并交叉链接；本记录只取代其单一保存 Key 来源，改为固定查询第一个启用的 key 与空池 no-key 错误，而端点、host 规则、缓存与只读边界继续有效。[Gateway Per-Attempt Request Logging and Stored Error Text](2026-09-20-gateway-per-attempt-logging-and-error-text.md) 被保留并交叉链接；其按尝试行契约继续有效，脱敏现覆盖整池。[API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md) 与 [Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md) 被保留并交叉链接；单一创建 key 现存为第一条 `Default` 命名密钥池条目，而它们的模板语义继续有效。
