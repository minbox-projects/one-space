# Agent Note: Gateway Key Health Rotates Bare 429s, Recovers Quota Marks after a TTL, Retries a Single Candidate and Pins the Query Source Key

Status: implemented

[English](2026-09-25-gateway-key-health-lifecycle.md) | 中文

## Problem

有序密钥池此前只把 401/403 标记为 `authentication`、把额度归类为耗尽的 429 标记为 `quota`，其他失败要么回退到其他服务商、要么终止本次尝试。因此，未被额度消息分类器归类为额度耗尽的普通限流 429 被当作任意临时失败处理：请求不会改用该服务商的下一个可用 key，尽管被尝试的 key 可能立即恢复。被额度标记的 key 在此前的全部标记 60 秒探测或操作者手动操作之前一直停用，而上游额度窗口会自行重置。恰好只有一个可服务候选的请求在临时 500 或网络错误时立即失败，完全不在该服务商上重试。只读的额度与用量查询还会固定使用第一个启用的 key，即使网关已将其运行时标记，于是卡片可能用一个已停用的 key 去查询账户。

## Decision

尝试结果现在区分持久性 key 域失败与裸限流 429。`key_failure_kind` 仍把 401/403 归类为 `authentication`、把额度耗尽的 429 归类为 `quota`，这些标记与此前完全一样持久化；未被额度消息分类器归类的 429 设置 `bare_rotation` 且不持久化任何内容。`select_usable_key(provider, now, attempted)` 按列表顺序返回第一个启用、本趟尚未尝试、且未被标记或带有已过 TTL 额度标记的 key，每趟维护自己的 `attempted_keys` 集合，因此裸 429 轮换对每个可用 key 最多尝试一次。裸 429 绝不标记 key、绝不登记映射健康、轮换本身也绝不消耗重试预算：请求立即改用下一个未尝试的可用 key，当本趟用尽可用 key 时，被记住的失败带着完整预算进入既有有界重试调度——每服务商一次初轮尝试加最多 `MAX_RETRIES_PER_PROVIDER`（5）次重试，每次重试都是新的一趟，从第一个可用 key 重新开始并清空已尝试集合。流式仅在写出首个转发字节前轮换，首字节写出后的失败保持截断路径。网络错误、5xx、404 与其他 4xx 仍然既不标记也不轮换。

`types_config::KEY_QUOTA_MARK_TTL_SECS = 1800` 为额度标记的 key 提供懒恢复路径。`selection::quota_mark_expired(key, now)` 仅对带有 `marked_at`、且标记龄至少达到 TTL 的额度标记（`auto_marked` 且 `failure_kind == Quota`）为真——恰好 1800 秒含边界，1800 秒以下不含。正常列表顺序选择可以选中这样的 key；选中时在内存中清除其运行时状态并经串行化配置写入持久化该清除，因此它按列表优先级服务。若它实际仍然耗尽，本次尝试会把它重新标记为 quota，同一请求继续使用下一个可用 key。auth 标记与没有标记时间的额度标记绝不经该 TTL 过期，既有的全部标记 60 秒单次探测规则（`KEY_PROBE_COOLDOWN_SECS = 60`）对未过 TTL 的标记保持不变。

两条尝试路径都移除了候选数量门槛。可重试的首字节前失败——上游 500、网络错误或其他可重试类别——即使请求恰好只有一个可服务候选也进入有界重试队列，仍受既有每服务商重试上限、带抖动的指数退避、`Retry-After` 优先与 120 秒等待预算约束；`attempt_non_streaming` 与 `attempt_streaming` 都在 `candidate.attempts <= MAX_RETRIES_PER_PROVIDER` 时入队。首字节写出后的流式失败保持既有截断路径、不重试，不可重试类别保持既有的跳过或返回客户端行为。多候选调度——fallback-first 初轮、按最早截止时间串行重试以及同一上限、退避、`Retry-After` 优先与预算——保持不变。

`selection::pinned_key_value` 现在按如下梯子解析只读查询源 key：列表顺序第一个启用且未被标记的 key，其次全部启用 key 都被标记时取第一个启用的 key，最后没有任何启用 key 时取第一个存储的 key。空池仍然没有固定 key，`resolve_quota_request` 与 `resolve_go_usage_request` 都保留既有 no-key 错误。CommandCode 额度与 OpenCode Go 用量使用该规则，其既有「源 key 值或基础 URL 变化即失效」的缓存失效保持不变。

## Alternatives considered

- 为裸 429 持久化 key 标记或冷却：未采纳，因为裸限流 429 往往只是临时的或按请求计，持久化会让该 key 停用、把 key 状态加入服务商对话框并破坏密钥池的列表优先级；本趟范围的已尝试集合已经为轮换设定了边界。
- 裸 429 时重试同一个 key 而不是轮换到下一个可用 key：未采纳，因为请求应立即改用下一个可用 key；反复联系同一个被限流的 key 只会延误密钥池本该提供的健康 key。
- 把轮换计入每服务商重试预算：未采纳，因为轮换不是重试；预算必须保持完整，让服务商在整趟全被限流后仍能收到完整的有界重试调度。
- 让一趟无限轮换，或允许同一个 key 在同一趟内被再次尝试：未采纳，因为服务商可能因此无限循环；每一趟对每个可用 key 最多尝试一次，然后把失败交给有界重试调度。
- 把 TTL 应用于 auth 标记或没有标记时间的额度标记：未采纳，因为被拒绝的凭据无法随时间恢复、需要操作者处理，而缺失标记时间没有任何已流逝 TTL 的证据；两者都保持永久标记。
- 通过半开探测而不是正常选择恢复已过 TTL 的 key：未采纳，因为探测是在没有任何可用 key 时的最后手段、按映射键单飞；TTL 恢复是确定性的新鲜度，属于正常列表顺序选择，选中即清除、仍然耗尽则重新标记。
- 为额度恢复选择无 TTL 或后台探测：未采纳，因为额度窗口无需操作者操作即可重置，而请求驱动恢复的契约不允许后台探测；固定 30 分钟 TTL 把代价限制为每个仍耗尽的 key 每个周期最多一次白费调用，并且不触碰全部标记的 60 秒探测。
- 保留单候选快速失败：未采纳，因为那样临时 500 或网络错误会立即暴露、无法在唯一服务商上重试；既有的上限、指数退避与 120 秒预算已经为额外尝试设好了边界，早先的理由不再压过临时恢复的价值。
- 把查询源 key 固定为正在服务的 key 或第一个存储的 key：未采纳，因为跟随服务 key 会让信息性查询耦合到流量，而第一个存储的 key 可能被禁用或运行时标记；梯子优先使用第一个可用 key，只有都不可用时才回退。

## Consequences

- 裸 429 轮换有界：一趟对每个可用 key 最多尝试一次，轮换本身不持久化标记、不登记映射健康、不消耗重试预算，用尽一趟后把服务商交给既有有界重试调度并保留完整预算；流式保持首字节边界，其他失败类别保持此前行为。
- 被额度标记的 key 在标记龄至少达到 `KEY_QUOTA_MARK_TTL_SECS = 1800` 秒（边界含）后按列表优先级恢复；选中时清除其运行时状态并经串行化配置写入持久化该清除，仍然耗尽的一次尝试会重新标记并继续下一个 key，而 auth 标记、无标记时间的标记与全部标记的 60 秒探测不变。
- 单个可服务候选在非流式与流式路径都与多候选请求一样获得有界重试调度；多候选的 fallback-first 调度、`Retry-After` 优先、每服务商上限与 120 秒预算不变，首字节写出后的流式失败仍截断而不重试。
- 额度与用量查询依次使用第一个启用且未标记的 key、第一个启用的 key、第一个存储的 key；空池保留既有 no-key 错误，查询保持只读，既有「源 key 值或基础 URL 变化即失效」的缓存失效不变。
- 验证：`src-tauri/src/ai_gateway/tests.rs` 中的行为测试覆盖 AC-010 至 AC-031（裸 429 轮换与其重试预算守恒、流式首字节边界、TTL 边界、选中即清除、仍然耗尽时的重新标记、auth 与缺失时间排除、保持不变的全部标记探测、单候选与多候选重试调度、以及源 key 梯子），`src-tauri/src/ai_gateway/tests/quota.rs` 与 `tests/go_usage.rs` 覆盖两个解析器及其缓存失效，完整 `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway` 套件证明既有密钥池、重试、快速失败与额度套件只在本计划要求处发生行为变化。
- `MEMORY.md` 已在同一变更中更新裸 429 轮换、额度 TTL 及其常量、单候选重试与查询源 key；持久化配置 schema 保持版本 2、无新字段也无新迁移，Tauri 命令签名与前端不变，回滚只还原路由与恢复行为。
- Supersession：部分取代。[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](2026-09-24-gateway-key-pool-rotation.md) 予以保留并交叉链接；本记录只取代其「临时 429 绝不轮换」「额度标记的 key 仅经探测或手动重新启用恢复」「查询字段固定使用第一个启用的 key」的陈述，而其有序选择、auth/quota 持久标记、空池跳过服务商、鉴权手动恢复与 upsert 保留继续有效。[Gateway Single-Candidate Fast Fail and Standard Error Responses](2026-09-18-gateway-fast-fail-and-standard-errors.md) 予以保留并交叉链接；本记录只取代其单候选单次尝试决策，而其统一错误信封、首字节前流式 502、上游 4xx 正文处理与中途流截断决策继续有效。[CommandCode Provider Cards Show Account Quota](../feature/2026-09-23-commandcode-provider-quota.md) 与 [OpenCode Go Provider Cards Show Usage](../feature/2026-09-24-opencode-go-provider-usage.md) 予以保留并交叉链接；本记录把其固定的源 key 细化为「第一个启用且未标记」的梯子，而端点、host 规则、五分钟缓存与只读边界继续有效。
