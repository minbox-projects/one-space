# Agent Note: Model Pools Route One Public Alias Across Explicit Source Candidates

Status: implemented

[English](2026-10-13-free-model-pool.md) | 中文

## Problem

网关此前把每个公共模型 id 只经恰好一条上游映射行暴露。持有多个免费或促销映射的操作者——例如同一服务商的两条映射，或多个账户——无法把它们作为一个公共模型发布：调用方必须知道单条映射的本地模型名，每条映射各自携带自己的健康状态，而某条映射上的模型级限额也没有任何边界把它与其他映射隔开。也没有任何持久化位置能表达“这些精确的来源映射构成一个公共模型、在它们之间轮转，并且在免费来源耗尽后绝不回落到普通付费默认模型”。本记录只覆盖计划 20261013-free-model-pool 的 Step 1：后端路由、存储与校验边界。运营商界面、终端集成与该特性专属的 OpenAI Chat 到 Responses 转换不属于本记录。

## Decision

- 在不改变 schema 的前提下持久化两个 `#[serde(default)]`、始终序列化的 `GatewayConfig` 字段：`model_pools: Vec<ModelPool>` 与 `model_pool_reserved_models: Vec<String>`。`GATEWAY_CONFIG_SCHEMA_VERSION` 保持 3，不引入迁移，因此在模型池出现之前写入的配置会读为空集合。`ModelPool { id, name, model, enabled, candidates }` 把 `model` 作为公共模型 id 暴露；`ModelPoolCandidate { provider_id, local_model, upstream_model, protocol, weight }` 是对既有服务商映射行的精确引用，`weight` 默认 1、范围 1..=100；`PoolCandidateIdentity { pool_id, provider_id, local_model, upstream_model, protocol }` 是一个已准备候选的仅传输身份。模型池记录绝不复制来源 key 或 endpoint。
- 在新模块 `src-tauri/src/ai_gateway/model_pools.rs` 中承载模型池边界。`plan_pool_request` 在任何普通候选构建之前运行：它把 trim 后的精确别名与当前模型池匹配，退役保留别名一律 fail-closed；禁用的池、对模型池别名的 `AnthropicMessages` 入站请求，或没有任何可用候选与合格探测的池返回标准 `all_providers_unavailable` 502（原生 `/v1/messages` 入口沿用既有 Anthropic 错误信封）。任何普通服务商或默认模型都不得服务当前或退役的模型池别名。
- 对最新配置解析每个候选。只有当候选的服务商存在、其原始 trim 后 `(local_model, upstream_model)` 映射仍以相同生效协议可解析、服务商与映射均已启用且未被自动禁用、候选能服务入站协议，且服务商有可用 key 时，该候选才可用。`prepare_provider` 把真实服务商、密钥池与端点克隆成单映射路由副本，把本地模型改写为池别名，清空 `default_model` 以免任何未选中的同级被选中，并挂上仅传输的 `pool_candidate` 身份。
- 以池专属的平滑加权轮询为可用候选排序。调度键是池 id 加完整候选身份（服务商、原始 local/upstream 对、存储的协议）加入站协议，因此两个池、同一服务商的两个模型，以及路由到同一存储候选的 Chat/Responses 入口各自拥有独立循环。模型池请求绝不读取也不写入会话亲和。只有可用候选占用主槽；至多一个映射或 key 半开探测追加在可用候选之后，且绝不拓宽到该池选中集合之外。
- 把健康、诊断与记账落回真实来源。`attempt_policy::resolved_health_target` 对已准备的池候选返回存储的原始服务商/本地/上游映射，因此探测、重试与映射自动禁用都结算在真实行上。失败的池尝试点名真实服务商并附上原始映射身份，用量日志把公共别名记为 `local_model`、同时记录真实服务商 id 与上游模型；计价仍使用真实来源服务商的价格行。
- 把恢复限定在候选上。候选级失败——结构化 `error.type` 为 `FreeUsageLimitError` 的 429，或额度分类器视为模型级/未知范围耗尽的 400/402/429 消息——会记录进程内存候选冷却并只丢弃该候选，不标记共享 key、不耗尽它的 key、也不登记映射健康。冷却按池加完整候选身份键控、并在各入站之间共享；`cool_pool_candidate` 保留精确 `Duration`，缺失或非法延迟回退到 `POOL_CANDIDATE_COOLDOWN_SECS = 60`，有效的 `retry-after-ms: 0` 让候选立即重新可用。显式账户资金/额度耗尽信号与普通凭据 401/403、非凭据 403 仍保持既有 key、账户与映射策略。
- 通过 `commands.rs` 与 `app_runtime/run_app.rs` 暴露四个类型化命令：`ai_gateway_upsert_model_pool(pool)`、`ai_gateway_delete_model_pool(pool_id)`、`ai_gateway_set_model_pool_enabled(pool_id, enabled)` 与 `ai_gateway_model_pools_status()`。upsert 在既有串行 `storage::modify_config` 原语下校验并写入，因此校验或写入失败不改变任何东西。
- 保持新引用精确、旧引用可保留。新候选必须对最新配置可解析，而已经存储的引用在其来源变得不可用后依然有效，以便操作者保留或显式移除；候选需要非空字段、无重复 tuple 与范围内 weight。公共模型 id 不得与普通映射或另一当前池冲突。重命名或删除池会把其先前别名推入 `model_pool_reserved_models`；新池可复用退役别名，而 `ai_gateway_upsert_provider` 会拒绝将获取当前或退役别名的普通映射。
- 发布非机密状态。`ai_gateway_model_pools_status()` 返回每个池的身份与候选，只含来源身份、可用性代码（`ready`、`cooling`、`source_disabled`、`mapping_unavailable`、`missing_provider`、`no_usable_key` 或 `unsupported_protocol`）与 epoch 秒冷却截止（由单调截止向上取整）。它不发起任何网络或额度探测，也不持久化任何东西。
- 仅在成功发布后清理自有状态。`storage::write_config` 与 `storage::modify_config` 把此前持久化的池与已发布的池做差，并调用 `model_pools::prune_removed_pools` 清除被移除池或候选自有的调度与冷却条目；写入失败不清理任何东西。

## Alternatives considered

- 在每个池候选内存储独立服务商、key 与 endpoint，而不是引用既有映射行：拒绝，因为这会复制凭据、破坏 endpoint 与 key 的单一事实来源，并让池与来源映射的健康脱节。
- 对池候选复用普通调度器与会话亲和：拒绝，因为普通调度器按服务商、本地模型与协议键控，无法区分同一服务商的两个同级候选，且会话亲和会把池别名固定到单个候选，破坏池存在的轮转目的。
- 持久化候选冷却或发起推测性启动/网络探测：拒绝，因为它们在 Step 1 范围之外；冷却留在进程内存，既有每请求探测仍至多一个。
- 让禁用或退役别名回落到普通路由：拒绝，因为过期客户端随后可能把退役的免费模型别名发往无关的付费默认模型；别名边界必须 fail-closed。
- 在候选级模型限额上标记共享 key：拒绝，因为模型级额度会让一个被健康同级模型共享的 key 退出服务；只有该候选冷却，而显式账户耗尽保持既有账户与 key 策略。
- 为模型池字段提升配置 schema 或新增迁移：拒绝，因为 `#[serde(default)]` 已使旧文件可读，所以 schema 保持 3 且无迁移。

## Consequences

- 当前池别名只在池专属加权轮转下经其选中候选路由，禁用或退役别名 fail-closed，绝不触及普通来源或默认模型。同协议请求是 Step 1 的传输基线；既有的 `anthropic_messages` 候选经既有适配器服务，与普通路由已有的行为一致，而池专属的 OpenAI Chat 到 Responses 转换不在此交付。
- 健康、探测与诊断停留在真实来源行上：映射自动禁用、key 轮换与全部不可用消息点名真实服务商与原始映射，用量日志保留公共别名并伴有真实服务商与上游模型。
- 配置兼容性与回退：schema 保持 3，含两个 serde 默认字段，因此旧二进制能读取文件但可能丢弃池字段；回到旧二进制需要保留同一 profile 的加密配置与匹配的密钥材料，且退役别名不受该旧写入者保护。
- 本步骤不交付运营商界面：没有模型池管理 UI 或终端工具同步，池专属的 Chat 到 Responses 转换不属于本记录。
- 验证：Step 1 行为测试为 `src-tauri/src/ai_gateway/tests/model_pools.rs`（由 `src-tauri/src/ai_gateway/tests.rs` 注册），工作树中的原始命令集全部以 0 退出：`cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::tests::model_pools::`（15 通过）、`cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::tests::attempt_policy::`（15 通过）与 `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::tests::routing_hardening::`（12 通过），无警告。该套件先被写成可编译并失败的 RED 基线，而主流程自检发现的五个受支持路径缺陷都针对可编译、可观察的回归进行了修复。
- 已知覆盖缺口：池候选用到的合格映射或 key 探测路径已实现，但仅有 key 探测的情形未被直接断言；亚秒与长 header 到期未被直接断言；小数 `weight` 表现为 serde 反序列化错误，而不是范围校验使用的候选字段错误。
- 部分取代：[API Gateway Smooth Weighted Round Robin Routes Requests and Fallback Candidates](../architecture/2026-09-20-api-gateway-weighted-routing.md)、[Gateway Session Affinity Pins a Session and Model to One Upstream](../architecture/2026-09-20-gateway-session-affinity-routing.md)、[API Gateway Scheduler Accounts per Provider, Local Model and Protocol and Prunes Deleted Providers](../architecture/2026-09-25-gateway-scheduler-accounting-scope.md)、[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](../architecture/2026-09-24-gateway-key-pool-rotation.md)、[Gateway Key Health Rotates Bare 429s, Recovers Quota Marks after a TTL, Retries a Single Candidate and Pins the Query Source Key](../architecture/2026-09-25-gateway-key-health-lifecycle.md)、[Gateway Key Authentication Marks Are Credential-Scoped and Surfaced per Provider](../bug-fix/2026-09-28-gateway-key-failure-scope-and-surfacing.md) 与 [Native Anthropic Messages Entry, Protocol Recognition and OpenAI Entry Conversion](../architecture/2026-10-11-ai-gateway-anthropic-messages.md) 均予保留并交叉引用。本记录只改变池专属候选调度、池请求刻意不设会话亲和、绕过共享 key 标记的候选级冷却，以及原生 Messages 的池别名边界；所有被保留记录的普通路由调度、亲和、key 恢复、账户策略与终端所有权保证仍然成立。
