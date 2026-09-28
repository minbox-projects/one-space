# Agent Note: API Gateway Scheduler Accounts per Provider, Local Model and Protocol and Prunes Deleted Providers

Status: implemented

[English](2026-09-25-gateway-scheduler-accounting-scope.md) | 中文

## Problem

平滑加权轮询调度器此前在进程全局表中只按服务商 id 保存一个当前权重。当一个服务商可经多个本地模型或两种入站协议触达时，候选集合不同的请求共享这一个当前权重：一个模型的流量会消耗掉另一个模型累积的当前权重，因此在混合候选集合下，即使等权重模型自身的候选集与权重并未变化，其胜出序列也会偏斜。被从配置中删除的服务商还会把其调度器条目永久留在进程全局表中，于是用同一 id 重新创建服务商会延续陈旧计费，而且没有任何接缝可以观察条目是否真的被清除。

## Decision

调度键现在是 `WeightedSchedulerKey` 三元组：服务商 id、trim 后的请求本地模型、入站协议。本地模型分量采用与模型解析相同的归一化——去空白，空或缺失视为不存在；协议分量是 `UpstreamProtocol::endpoint_path()` 返回的规范端点后缀（`/chat/completions` 或 `/responses`），一个稳定的 `&'static str`。`selection::weighted_candidates(candidates, local_model: Option<&str>, protocol: UpstreamProtocol)` 为每个候选构造一个键，请求入口在会话亲和临界区内解析顺序时传入请求本地模型与入站协议，因此每个服务商的当前权重按「服务商、模型、协议」作用域各存在一份。

SWRR 算法本身不变：每个候选的当前权重累加其有效权重（`weight.max(1)`），当前权重最大者胜出（平手按 provider.id 字典序升序决胜），选中者的当前权重扣减本次所有参选权重之和，剩余候选按更新后的当前权重降序排列并沿用同一 provider id 决胜。零或单候选列表仍直通返回、不修改调度器状态，被禁用或自动禁用的服务商仍由候选解析前置过滤。

调度器条目只在真实删除时、且只在配置成功发布之后被清理。`write_config` 与 `modify_config` 在发布前读取同一路径上已持久化的服务商 id（`persisted_provider_ids`），发布新配置后调用 `prune_removed_providers(before_ids, &published)`，把恰好 `before - after` 的集合交给 `selection::prune_weighted_scheduler`；写入失败不清理任何条目。不在已发布配置中、但此前也从未存在的 id 属于共享进程全局调度器的另一份配置——例如并行的多 home 测试进程——被刻意保留。`#[cfg(test)]` 访问器 `weighted_scheduler_entry_count(provider_id)` 统计某一服务商 id 跨所有模型与协议作用域的条目数，既有直接调度器测试已机械适配新键参数、未弱化任何断言。

## Alternatives considered

- 保留仅按服务商 id 计费：未采纳，因为不同候选集共享一个当前权重会让一个模型或协议的流量扭曲另一个的分布，而这正是观察到的故障。
- 只按服务商加本地模型计费，或只按服务商加协议计费：未采纳，因为两个维度隔离的都是真实作用域；同一服务商可以在 `chat_completions` 与 `responses` 下用不同的对端候选服务同一本地模型，同一协议下的不同本地模型也来自不同候选集合，去掉任一维度都会在该轴上重新引入偏斜。
- 按完整候选集合或其哈希计费：未采纳，因为候选成员会随用户启停与映射编辑变化，计费会因无关编辑而碎片化或重置；服务商、模型与协议才是稳定的、操作者可见的作用域，算法、权重与平手决胜均不在变更范围内。
- 修改 SWRR 算法本身或权重语义：未采纳，因为本计划保持算法、权重范围与确定性平手决胜不变；只有计费维度移动，同集合平滑序列是回归测试对象。
- 清理写入者配置中缺失的每个服务商 id，而不是同一路径的前后差集：未采纳，因为调度器是进程全局的、由多份配置共享（例如并行的多 home 测试），对本写入者缺失的 id 可能仍属于另一份配置，绝不能清除。
- 每次配置写入都清空整个调度器表：未采纳，因为任何服务商编辑都会重置所有无关服务商的平滑性，而且写入失败绝不能改动调度器状态。
- 在候选选择时懒清理，而不是发布之后：未采纳，因为选择需要在请求路径上比较配置状态，并会在一次尝试过程中改动调度器状态；清理应属于证明删除已经落地的成功发布。

## Consequences

- 混合候选集合不再按模型偏斜：等权重服务商 A、B、C 中 M1 映射到 {A, B}、M2 映射到 {A, B, C} 时，M1 的胜出序列在 A 与 B 之间一比一交替，而 M2 按一比一比一循环。
- 同一服务商与本地模型在不同协议下保持独立的胜出序列，因为入站协议是计费键的一部分。
- SWRR 的累加、选择、平手决胜、剩余候选排序与零或单候选直通均不变，同集合 3:1 平滑序列继续成立；会话亲和仍在自己的临界区内解析顺序，其绑定规则未被触碰。
- 被删除并持久化的服务商在成功发布之后其调度器条目被移除，因此重新创建该 id 会从全新状态开始；写入失败、仅另一份配置知晓的 id、以及仅仅缺失的 id 都绝不触发任何清理。
- 验证：`src-tauri/src/ai_gateway/tests.rs` 中的中继级行为测试覆盖 AC-032 至 AC-036（混合集合分布、3:1 平滑回归、独立协议序列、经调度器访问器观察的已删除服务商清理、以及空或单候选直通），已适配的直接调度器测试保留全部断言，完整 `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway` 套件证明没有其他调度或会话亲和回归。
- `MEMORY.md` 与 `.ai-workflow/index/navigation.json` 已在同一变更中更新复合计费键与发布时清理；持久化配置 schema 保持版本 2、无新字段也无新迁移，Tauri 命令签名与前端不变，回滚只还原调度器隔离与清理行为。
- Supersession：部分取代。[API Gateway Smooth Weighted Round Robin Routes Requests and Fallback Candidates](2026-09-20-api-gateway-weighted-routing.md) 予以保留并交叉链接；本记录只取代其「按服务商 id 作键的进程全局状态」前提并新增发布时清理，而其服务商权重配置、SWRR 算法、平手决胜、与会话亲和的协同以及零或单候选直通继续有效。[Gateway Session Affinity Pins a Session and Model to One Upstream](2026-09-20-gateway-session-affinity-routing.md) 不受影响：亲和重排原样消费按作用域排序的结果，该记录没有任何事实变化，因此不记录取代。本记录所依赖的配置写入机制由 [Gateway Configuration Writes Serialize on One Lock and Reads Cache by File Identity](2026-09-25-gateway-config-write-serialization-and-read-cache.md) 记载；其他活动记录均未记载调度器计费。
