# Agent Note: API Gateway Smooth Weighted Round Robin Routes Requests and Fallback Candidates

Status: implemented

[English](2026-09-20-api-gateway-weighted-routing.md) | 中文

## Problem

此前，网关对所有可用上游服务商一视同仁，通过均匀随机洗牌（`selection::shuffled_candidates`）对候选进行排序。当用户配置了速率限制、配额额度、账户层级或 Token 余额不对称的多个账号或服务商（例如同时配置高配额付费账号与低配额免费账号）时，均匀洗牌会按等概率（1/N）分流，导致低配额服务商迅速耗尽配额，而高配额服务商的容量未能充分利用。此外，当请求携带新会话的亲和请求头时，初次会话绑定同样由该均匀随机洗牌播种，导致长会话在各账号间均匀散列，无法体现用户设定的容量倾斜。在前端界面中，服务商在 `ProviderDetailDialog` 中缺少权重配置项，且在服务商卡片上没有任何容量分配的视觉标识。

## Decision

每个上游服务商配置（`GatewayUpstreamProvider`）支持整数 `weight: u32`，默认值为 1，允许取值范围为 1 到 100（`MIN_PROVIDER_WEIGHT` 至 `MAX_PROVIDER_WEIGHT`）。该字段标注 `#[serde(default = "default_provider_weight")]`，确保旧版本 `api_gateway.json` 配置文件无需手动迁移即可缺省反序列化为权重 1。通过 `api_gateway_save_config` 与 `api_gateway_upsert_provider` 保存配置时进行严格范围校验，超出 1 到 100 范围的值将被拒绝写入并返回可操作错误。前端在 `ProviderDetailDialog` 中提供专用的权重数值输入框（带“1–100”辅助提示文案）并校验正整数范围，在 `UpstreamProviderList` 服务商卡片上直观展示 `Weight: X`（中文为 `权重: X`）徽标。

候选调度采用在 `selection::weighted_candidates` 中实现的平滑加权轮询（SWRR）算法，彻底替代既有的均匀随机洗牌。由进程全局状态表（`WEIGHTED_SCHEDULER`）跟踪每个服务商的动态当前权重。每次候选解析时：(1) 每个候选的当前动态权重累加其有效配置权重（`weight.max(1)`）；(2) 选出当前动态权重最大者作为主选上游，若有平手则以服务商 ID 字典序（升序）决胜；(3) 扣减选中者的当前动态权重，扣除值为本次所有参选候选的权重总和；(4) 剩余候选按其更新后的当前动态权重降序排列（平手同样以服务商 ID 字典序升序决胜），构成有序的 fallback 降级序列。若候选列表为空或仅有一个候选，则直通返回且不修改调度器状态。被用户禁用或自动禁用的服务商与模型已由候选解析前置过滤，绝不参与权重累加或调度。

加权路由与会话亲和（`session_affinity`）深度协同。不带会话头的无状态请求直接由 `weighted_candidates` 完成排序与调度。当请求引入尚无绑定的新会话时，首选上游候选在会话亲和临界区内直接由 `weighted_candidates` 决出并存为该会话的固定绑定，确保长会话流量随着时间推移严格按照配置权重比例在各个服务商账号间平滑分配。当请求匹配已存在的有效会话绑定且该绑定服务商依然可用时，该绑定服务商固定排在首位以延续前缀缓存效益，而其余所有可用候选依然保留由 `weighted_candidates` 生成的加权 fallback 降级序列。既有的会话亲和生命周期、LRU 缓存容量上限、连续两次未命中迁移规则及故障转移行为完全保留。

## Alternatives considered

- 随机加权选择（轮盘赌采样）：未采纳，因为伪随机加权采样在较小请求窗口内容易产生聚集与流量毛刺，而平滑加权轮询（SWRR）能在上游之间提供数学上确定且平滑交错的请求分流。
- 不带权重的纯无状态轮询：未采纳，因为各服务商的速率限制与配额层级存在显著不对称性，无加权轮询无法反映各账号之间的容量差异。
- 剩余候选采用随机降级顺序：未采纳，因为当主选候选失败时，降级重试应优先尝试具备次高动态容量的服务商，而非随机盲选。
- 脱离权重的独立会话路由池：未采纳，因为会话亲和在初次分配时理应自然遵循账号容量比例，无需引入冗余的路由池配置。
- 浮点数权重或无界大整数：未采纳，因为 1 到 100 的整数范围对运维者直观易懂，避免了权重累加中的浮点精度陷阱，并足以覆盖常见的多账号配比需求。

## Consequences

- 上游服务商配置支持 `weight: u32`（范围 1–100，默认 1，对应 `MIN_PROVIDER_WEIGHT` 至 `MAX_PROVIDER_WEIGHT`），对旧版配置无缝反序列化且无需 schema 迁移；超出 1–100 范围的值在 `api_gateway_save_config` 与 `api_gateway_upsert_provider` 中被拒绝。
- 网关以平滑加权轮询（`selection::weighted_candidates`）替代均匀随机洗牌，选取当前动态权重最大者作为主选上游，并将剩余候选按动态权重降序排列用于 fallback 降级重试；空或单候选请求直通返回且无调度器开销。
- 会话亲和与 SWRR 深度协同：无会话请求直接使用 SWRR 排序；新会话首次请求的首选上游由 SWRR 决定，使多会话负载在账号间按权重比例分配；已有活跃绑定保持绑定服务商排在首位，而其余候选保持加权 fallback 序列。
- 前端 `ProviderDetailDialog` 提供经过校验的权重输入框（1–100，默认 1），`UpstreamProviderList` 在每个服务商卡片上渲染 `Weight: X` / `权重: X` 徽标，提供完整的中英文双语国际化支持。
- 部分取代：[Gateway Session Affinity Pins a Session and Model to One Upstream](2026-09-20-gateway-session-affinity-routing.md) 保留并由本记录交叉链接；本记录仅取代其依赖均匀随机洗牌（`selection::shuffled_candidates`）进行初始候选排序与新会话播种的行为，其请求头优先级、30 分钟空闲过期、1024 条 LRU 上限及连续两次未命中迁移规则依然全部有效。
- `MEMORY.md` 在同一变更中记录了服务商权重配置属性、SWRR 候选调度与降级排序算法，以及与会话亲和的协同规范；导航索引与全量测试套件验证通过。
