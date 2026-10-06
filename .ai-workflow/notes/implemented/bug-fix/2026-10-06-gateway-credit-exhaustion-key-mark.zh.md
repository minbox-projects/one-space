# Agent Note: Gateway Credit-Exhaustion Responses Mark the Attempted Key as Quota

Status: implemented

[English](2026-10-06-gateway-credit-exhaustion-key-mark.md) | 中文

## Problem

密钥池分类此前只把命中额度消息分类器的 429 升级为 key 域额度失败：`selection::classify_failure_with_message` 返回 `FailureClass::Retryable`、`runtime_http::key_failure_kind` 返回 `Some(KeyFailureKind::Quota)` 仅限正文命中 `selection::is_quota_exceeded_message` 的 HTTP 429。CommandCode 当前的额度耗尽响应是携带 `insufficient credits` 正文的 HTTP 400，402 额度响应也是同样形态，因此两者都被归类为普通客户端错误：所尝试的 key 得不到任何运行时标记，耗尽的 key 仍留在后续请求的选择集内，而且即使另一个 key 或候选仍能服务，上游错误也会被回传给调用方。

## Decision

命中额度消息分类器（`selection::is_quota_exceeded_message`）的 HTTP 400、402 或 429 现在都是 key 域额度失败。`selection::classify_failure_with_message` 对这三个状态返回 `FailureClass::Retryable` 而不是 `ReturnToClient`，`runtime_http::key_failure_kind` 对同一批状态返回 `Some(KeyFailureKind::Quota)`。所尝试的启用 key 经既有运行字段（`auto_marked`、`failure_kind`、`marked_at`、脱敏 `reason`）标记为 `quota`，标记被持久化，同一请求立即在下一个未尝试的可用 key 或候选上继续，不等待退避、不消耗每请求重试预算、也不登记映射行健康。只要还有候选可服务，请求绝不把这样的上游响应回传给调用方。

状态是本次决策唯一新增的部分。未命中额度消息分类器的 HTTP 400 或 402 保持 `FailureClass::ReturnToClient` 且不产生 key 标记；401 与带凭据文本的 403 保持 `authentication`；无额度文本的裸限流 429 仍在同一趟内轮换且不持久化任何标记；网络错误、5xx、404 与其他 4xx 保持此前行为；首字节写出后的流式失败保持截断路径且不标记任何内容。由于分类器基于关键词并与既有 429 处理共用，正文含额度关键词但并非账号耗尽的 400（例如 `insufficient permissions`）也会走 key 标记路径；这一已接受的宽泛度由下方未改动的恢复路径界定。

恢复不变。由该信号产生的额度标记只通过 `KEY_QUOTA_MARK_TTL_SECS = 1800` 的正常列表顺序选择（选中即清除、仍然耗尽则重新标记）、没有可用 key 时唯一一次 `KEY_PROBE_COOLDOWN_SECS = 60` 半开探测、或 `ai_gateway_reenable_provider_key` 恢复。没有新增任何 TTL、探测、后台流量或手动路径，且 `enabled` 标志、映射行、服务商状态、价格、终端同步与用量数据库 schema 均未被触碰。

## Alternatives considered

- 对额度耗尽响应自动禁用映射行或服务商：未采纳，因为该状况属于账号凭据而非模型；同一服务商的另一个 key 仍可服务，既有的 key 运行时标记已带有 TTL、探测与手动恢复，无需让其他模型或 key 离开服务。
- 从 CommandCode 额度快照而非转发响应触发标记：未采纳，因为额度查询是服务商专属、只读且非权威的（未公开端点、五分钟缓存、失败不缓存）；转发必须对真实上游响应作出反应并保持基于状态加文本。
- 为额度标记使用更长或仅手动的 TTL：未采纳，因为既有的 1800 秒 TTL 与全部标记探测已经为恢复设好边界，更长或仅手动的窗口会让已恢复的账号继续停用或增加操作者负担，而没有新证据。
- 增加服务商专属或 host 专属规则：未采纳，因为分类保持状态加脱敏错误文本，正如鉴权分类已经做的那样；host 规则无法覆盖每个上游，还会重复共享分类器。

## Consequences

- 额度耗尽的 HTTP 400/402/429 响应是 key 域额度失败：所尝试的 key 被标记并持久化，请求立即轮换到下一个可用 key 或候选，只要还有候选可服务就绝不把上游错误回传。
- 其余判定边界全部不变：无额度文本的 400/402 保持回传调用方且不标记，401 与带凭据文本的 403 保持 authentication，裸 429 在同一趟内轮换且不持久化，网络/5xx/404/其他 4xx 保持其类别，流式保持首字节边界。
- 恢复未被触碰：1800 秒 TTL 重新选择、唯一一次 60 秒探测与 `ai_gateway_reenable_provider_key` 是仅有的路径，标记不改变任何 `enabled` 标志、映射行、服务商状态、价格、终端同步或用量 schema。
- 可见表面是既有的服务商卡片 marked-key 区块，带有既有 `quota` 状态标签、标记时间、脱敏原因与重新启用控件；前端类型、组件与 i18n 键均无变化。
- 验证：`src-tauri/src/ai_gateway/tests.rs` 中的行为套件覆盖额度 400 的标记与回退、双 key 轮换、全部标记跳过及其 `no usable upstream key (1 quota exhausted)` 摘要、无额度 400 反例、带与不带命中消息的 400/402/429 分类矩阵与关键词宽泛度用例，以及不变的 TTL、探测与手动重新启用回归套件；`cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway` 以 0 退出且 602 个测试通过，前端 AI 网关回归 505 个测试通过。
- Supersession（取代评估）：部分取代。[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](../architecture/2026-09-24-gateway-key-pool-rotation.md)、[Gateway Key Health Rotates Bare 429s, Recovers Quota Marks after a TTL, Retries a Single Candidate and Pins the Query Source Key](../architecture/2026-09-25-gateway-key-health-lifecycle.md) 与 [Gateway Key Authentication Marks Are Credential-Scoped and Surfaced per Provider](2026-09-28-gateway-key-failure-scope-and-surfacing.md) 被保留并交叉链接；本记录只取代它们「命中额度消息分类器只在 HTTP 429 标记 key」的陈述，改为 HTTP 400/402/429 的状态加额度消息规则，而有序选择、auth/quota 持久标记、裸 429 轮换、TTL 与探测恢复、凭据限定的 403 规则、诊断与卡片操作全部继续有效。
- `MEMORY.md` 与 `ai-gateway-backend` 导航条目在同一变更中更新了扩展后的分类及其边界，`navigation.md` 已按权威 JSON 重新生成。
