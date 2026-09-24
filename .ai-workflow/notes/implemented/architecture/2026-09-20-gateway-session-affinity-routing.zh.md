# Agent Note: Gateway Session Affinity Pins a Session and Model to One Upstream

Status: implemented

[English](2026-09-20-gateway-session-affinity-routing.md) | 中文

## Problem

中继对每个请求都重新洗牌候选（`selection::shuffled_candidates`），因此有 N 个可用上游时某个账号被选中的概率是 1/N，同一客户端会话的连续请求会落在不同账号上，也没有账号能保持热前缀。服务商侧的提示缓存只有在同一账号持续服务同一会话前缀时才有收益，所以在纯每请求洗牌下，该前缀会在随机选中的账号上被重写而不是被复用。两个终端客户端本就发送会话标识：由于网关以 provider key `gateway` 注册、并不以 `opencode` 为前缀，OpenCode 发送 `x-session-affinity` 与 `X-Session-Id`；Codex 在当前版本发送 `session-id`，在更早版本发送 `session_id` 或 `conversation_id`——但此前没有任何选择规则使用这些头。

## Decision

亲和基于请求头，因为会话标识只能来自终端实际发送的请求头，而两个终端本就都会发送；它限定在单个会话与模型范围，因此不同会话仍会分散到各账号。网关从固定的客户端请求头优先级列表中解析会话标识——`x-session-affinity`、`x-opencode-session`、`session-id`、`session_id`、`conversation_id`、`thread-id`、`x-session-id`——取第一个存在且 trim 后非空的头。值为空或全空白的头等同不存在并继续向后查找，列表之外的请求头绝不参与，入站映射按已小写读取。`session-id` 刻意排在 `thread-id` 之前：Codex 会话的根线程与每个 subagent 请求都携带同一个 `session-id`，而 `thread-id` 只标识单个线程，因此若优先级偏向 `thread-id`，一个会话的请求会被拆散到多个绑定上。

绑定键是会话值与 trim 后本地模型名的组合，因此同一会话的两个模型各自独立绑定，`model` 缺失或全空白时视为不存在。某会话与模型的第一个请求绑定到既有洗牌本就排在最前的服务商，并在选择时写入该绑定：绑定查找、洗牌与首请求写入发生在绑定存储的同一个临界区内，不存在的情况也在其中复查，因此同时到达的两个同会话同模型请求会选出同一个首选服务商。之后的请求把本次请求的候选列表重排为绑定服务商在前，其余候选的相对顺序保持不变。

绑定绝不放宽候选集：它只重排候选过滤已经返回的结果，因此协议不一致、映射被用户禁用或自动禁用、或服务商被禁用的服务商绝不因为「它是绑定」而被尝试。绑定在每次请求的终局上游结果之后结算一次：当绑定服务商不在本次请求的可用候选中时，绑定立即被替换为实际完成该请求的服务商，未命中计数为零，且本次请求不因原绑定产生任何尝试或失败计数。绑定服务商仍可用时，一次在其他服务商上到达终局上游结果的请求记一次未命中，连续第二次未命中把绑定迁移到完成该次请求的服务商；由绑定服务商完成的请求把未命中计数归零，下游取消不记录任何内容。无会话头或无 trim 后非空模型的请求既不读取也不写入绑定；仅探测请求同样如此——它是找到合格半开探测并以空候选列表尝试该探测的零候选请求，见 [Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast](2026-09-23-gateway-auto-disable-recovery.md)。

空闲超过 30 分钟的绑定视为不存在，绑定表最多保存 1024 条，超出时逐出最近最少使用的一条。绑定只存在于进程内存：绝不落盘，重启后从无绑定开始，且头名列表、其优先级、两次未命中阈值、空闲过期与条目上限都是常量；网关配置与用量数据库携带由版本门控迁移管理的 schema 与版本标记（[Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md)），该迁移不持久化任何绑定状态。

缓存收益在上线后按同一模型与上线前对比衡量：用量分析中按服务商的 Cache read 命中率与请求数占比，取上线后第一周与之前一周对比。`UsageStatsPanel.tsx` 直接渲染后端持有的规范 `cache_hit_rate_percent`，因此显示的命中率是后端按 token 加权的结果，而不是前端估算；在该衡量口径确定时，面板仍在前端按 `cache_read / (input_tokens + cache_read)` 计算并低估 OpenAI 系服务商的命中率（其 `prompt_tokens` 已包含缓存命中的部分），该历史偏差在每个上游上稳定，因此依据仍是同一上游上线前后的趋势。

## Alternatives considered

- 用前缀哈希推导会话标识：未采纳，因为会话标识只能来自终端实际发送的请求头，而前缀哈希从请求内容推断身份，而不是来自客户端声明的身份。
- 检查请求体寻找会话标识：未采纳，原因相同，且中继只负责转发请求体；从请求体推断出的身份不是终端发送的会话标识。
- 把 `x-codex-turn-state`、`x-codex-window-id` 或 `originator` 用作键来源：未采纳，因为它们都不是终端发送的会话标识；只携带这些头的请求与不携带会话头的请求行为完全一致，绝不创建或复用绑定。
- 区分父子会话：未采纳，因为 subagent 必须与其会话共用一个绑定，所以 `x-parent-session-id` 被忽略（它不在列表内），而不是拆分绑定。
- 让头名列表、其优先级、阈值、过期或上限可配置：未采纳，因为本次改动不新增配置、也不新增界面；它们保持为常量，也不引入自己的迁移，网关级 schema 标记由 [Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md) 承载。

## Consequences

- 携带任一已知会话头且模型 trim 后非空的请求，在绑定服务商仍可用时优先尝试它；不携带任何已知会话头、或值为空/全空白的请求与现有每请求洗牌完全一致，且既不读取也不写入绑定，因此未知或改名的头会退化到现状行为，而不是失败。
- 各会话仍会分散到各账号：绑定按会话与模型隔离、由现有洗牌播种、空闲过期并有上限。持续失败的绑定在连续第二次未命中时迁移，而不是让每个请求都付一次失败尝试；不再可用的绑定在同一次请求内被实际服务的服务商替换，原服务商不产生任何尝试与失败计数。
- 候选集、其余候选的顺序、失败分类、重试顺序与退避、冷却截止时间、自动禁用、`RequestHealth` 结算、错误信封、调用方可见的响应字节与用量日志行均保持不变；唯一差异是优先尝试哪个可用候选，且绑定绝不放宽候选集。
- 绑定只存在于进程内存：绝不落盘，无需迁移或回填，回滚或重启后从无绑定开始，每请求洗牌再次成为唯一的选择规则；网关级 schema 版本由 [Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md) 承载。
- 交付的规则位于 `src-tauri/src/api_gateway/selection.rs`（`SESSION_ID_HEADERS`、`resolve_session_id`、`SessionAffinityStore`、`reorder_bound_first` 与进程级 `session_affinity` 表），并在 `src-tauri/src/api_gateway/runtime_http.rs` 中围绕既有的候选过滤与洗牌应用；`src-tauri/src/api_gateway/tests.rs` 中的行为测试覆盖解析器及其边界、按模型绑定、并发选择、重绑、迁移、重置、暂停时钟下的空闲过期、LRU 上限、取消与会话头失败路径。
- `MEMORY.md` 在同一变更中记录亲和标准与初轮候选顺序的会话绑定例外；交付的符号都是模块私有（`pub(in crate::api_gateway)`）且没有文件路径变化，因此导航索引无需新增条目，`navigation.json` 与其生成的 `navigation.md` 保持权威且不变。
- 取代评估：无取代。[Gateway Per-Model Mapping Disable Is an Explicit Exclusion](2026-09-18-gateway-per-mapping-disable.md) 保留并交叉链接，因为亲和重排只重排其排除规则已留下的候选集，其决策仍然有效；[Gateway Per-Model Auto-Disable Settles Health on the Mapping Row](2026-09-20-gateway-per-model-auto-disable.md) 出于同一原因保留并交叉链接，其结算的行运行时状态进一步收窄该候选集，而重排绝不放宽它；[Gateway Single-Candidate Fast Fail and Standard Error Responses](2026-09-18-gateway-fast-fail-and-standard-errors.md) 保留并交叉链接，因为亲和只改变其 fallback-first 调度中优先尝试哪个可用候选，其调度与错误信封决策仍然有效；[Gateway Per-Attempt Request Logging and Stored Error Text](2026-09-20-gateway-per-attempt-logging-and-error-text.md) 与候选选择无关，亲和的候选重排不改变其日志契约；[Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast](2026-09-23-gateway-auto-disable-recovery.md) 被保留并交叉链接，因为仅探测请求以空候选列表尝试其探测且仍既不读取也不写入绑定，本记录的决策继续有效。用量统计、模型价格、服务商模板、终端同步、聚合模型、本地 Key 与双语文档记录均不相关，因此没有任何记录被取代。
