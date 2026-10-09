# Agent Note: Gateway Configuration Writes Serialize on One Lock and Reads Cache by File Identity

Status: implemented

[English](2026-09-25-gateway-config-write-serialization-and-read-cache.md) | 中文

## Problem

每一次配置变更——请求后的映射健康结算、key 标记/重新计时/清除写入、用户命令保存与模板同步——都是一次整文件读改写：`read_config` 解密并解析整个 `ai_gateway.json`，调用方修改自己的快照，`write_config` 再加密并把替换文件 rename 覆盖原文件。两个并发写入者可能读到同一份基线状态，后一次 rename 会静默丢弃先写入者的变更；不产生任何变化的结算仍会重写加密文件；每次尝试时读取又都要重新解密。读取路径上的懒迁移让问题更糟，因为读取者可能在写入者发布过程中迁移并重写文件，覆盖写入者更新的配置。固定临时路径（`path.with_extension("tmp")`）还允许两个并发写入者碰撞到同一个临时文件，而变更闭包内的 panic 可能毒化整个序列持有的锁，使网关面临永久失效的风险。

## Decision

`storage::modify_config(mutate)` 是串行化的配置读改写入口。它获取进程级 `CONFIG_WRITE_LOCK: Mutex<()>`，经回收的 guard 从锁中毒中恢复；缓存热时从读取缓存加载当前配置，否则经 `read_config_locked` 加载——后者绝不嵌套获取锁、也绝不嵌套重写；运行调用方的 `FnOnce(&mut GatewayConfig) -> Result<(bool, T), String>`；仅当闭包报告状态变化、或锁内文件仍需遗留迁移时才持久化；并返回闭包的值。闭包必须保持本地且绝不等待网络，因此锁绝不会在上游网络等待期间被持有。

现在每个生产变更都经该原语执行：映射健康结算（`RequestHealth::apply` 与 `apply_failure`）、key 标记与清除写入（`persist_key_runtime_state`，含 TTL 选中清除）、服务商新增/删除/启用/重新启用命令、本地 Key 新增、删除、默认与变更命令、网关启用标志、用量保留、模板自动刷新、模板同步、派生服务商创建与删除，以及终端同步台账写入。`ai_gateway_sync_provider_template` 在锁外拉取模板模型清单并读取服务商载荷，在锁内对最新配置应用 `apply_template_sync_from_body`，随后在该变更之后执行 best-effort 终端刷新；刷新经自己的串行变更持久化台账，因此任何上游网络等待都绝不处于锁内。

仅当变更自身前后比较并报告无变化时写入才被抑制：映射健康结算（`RequestHealth::apply` 与 `apply_failure`，比较受影响行的 `MappingRuntimeState` 快照）、运行时 key 状态（`persist_key_runtime_state`，把变更后的 key 与其克隆比较）与网关启用标志（`persist_enabled`，在标志已经一致时报告无变化）。其余命令保存无条件报告 `changed = true`，因此对服务商及其映射集或 key 的幂等重复保存、删除不存在的 id、或重新选择当前默认 key 仍可能重写加密文件；计划只要求对结算与运行时 key/映射状态写入做抑制。被抑制的变更仍用新解析结果预热读取缓存，但保持加密文件字节完全不变。

发布是原子且无碰撞的。`write_config_through_temp` 归一化、加密并把载荷写入调用方持有的临时文件，再将其 rename 覆盖配置路径；`modify_config` 与 `write_config` 经 `unique_config_temp_path` 派生该路径，在文件名后追加进程 id 与进程单调计数器（`CONFIG_TEMP_COUNTER`），因此任何两次写入都不可能选中同一个临时文件。`write_config_through_temp` 返回真正落盘的归一化值，调用方存入读取缓存的是该值——而不是归一化前的输入。成功发布后，写入前存在且不在已发布配置中的服务商 id，其调度器条目由 [API Gateway Scheduler Accounts per Provider, Local Model and Protocol and Prunes Deleted Providers](2026-09-25-gateway-scheduler-accounting-scope.md) 清理；写入失败不清理任何条目。

读取由按文件身份作键的进程内存缓存服务。`CONFIG_READ_CACHE` 为每个配置路径保存一个 `CachedConfig`——归一化配置加上路径、文件字节长度与修改时间——只要 `file_identity` 仍匹配，`read_config_file` 就返回缓存值，因此热读取绝不解密。每个进程内写入都在文件的新身份下存入已发布的归一化值；改变长度或修改时间的直接磁盘重写仅凭身份检查即可使条目失效，无需任何失效辅助函数；文件缺失或为空则清除条目。

懒迁移在同一把锁下保持原子且一次性。当 `read_config_locked` 报告存储版本更旧时，`read_config_file` 获取 `CONFIG_WRITE_LOCK`，在锁内重读文件并重新确认版本，之后才盖上当前版本并经 `write_config` 重写；若期间已有并发写入者发布当前版本配置，读取者直接服务该配置且不写入。迁移重写失败保留磁盘上此前的完整字节，下一次读取重试。

持久化层不再自行发明值：普通读取或写入除 `normalize_stored_config` 规范化外原样返回存储配置，写入路径仍应用 `scope_model_prices` 裁剪。显式存储值——包括空 `reasoning_efforts` 列表、显式或零价格、已存 `local_model` 以及全部持久化 schema 字段——均被保留；推理强度绝不自动生成，价格行绝不跨服务商借用。此前的隐式查询 `query_model_reasoning_efforts` 与 `normalize_template_prices_and_efforts` 连同两处调用由 [Core Workflows Cleanup and Optimization](2026-10-09-core-workflows-cleanup-and-optimization.md) Step 7 移除；运行态（结算与 key 标记）写入不再触发模板或价格重算。

## Alternatives considered

- 保留每次变更的整文件读取加原子 rename：未采纳，因为并发写入者仍互相丢失变更，无操作结算也仍会重写加密文件；串行化必须覆盖整个读改写，而不只是 rename。
- 按服务商、按命令种类或按字段加锁：未采纳，因为持久化单位是一次整文件加密重写，任意两个写入者都会替换整个文件；更细的锁仍会交错，而单一进程级锁在人工编辑频率下不存在竞争。
- 用操作系统文件锁或锁文件替代进程级互斥锁：未采纳，因为进程是唯一写入者，文件锁可能在崩溃后滞留，而进程内锁中毒可在下一次调用时恢复。
- 仅通过显式辅助调用使读取缓存失效：未采纳，因为直接磁盘重写会留下陈旧缓存；路径加长度加修改时间可观察到任何变化（包括绕过生产写入路径的变化），无需失效 API。
- 保留固定临时路径（`path.with_extension("tmp")`）：未采纳，因为并发写入者可能碰撞到同一个临时文件；进程 id 加单调计数器使每个临时路径唯一。
- 锁中毒后永久失败：未采纳，因为 panic 的变更绝不能使网关失效；每个加锁点都经中毒 guard 恢复。
- 为简化写入而在持有锁时拉取模板模型：未采纳，因为缓慢或挂起的上游会阻塞所有配置变更；拉取在锁外执行，只有暂存后的应用在锁内执行。
- 获取迁移锁后不重读、不重新确认版本：未采纳，因为并发写入者可能已经发布迁移并盖章后的配置；锁内重确认保证迁移一次性且绝不覆盖更新的内容。

## Consequences

- 并发配置写入者不再互相丢失变更：并发 key 标记、key 标记与映射健康结算竞争、重复写入者以及用户编辑与结算竞争，最终都留下完整、可读的配置。
- 不产生任何变化的结算或运行时 key 状态写入保持加密文件字节完全不变，而幂等的命令重复保存仍可能重写该文件；被抑制的变更仍以预热的缓存让下一次读取无需解密；锁中毒被恢复，网关继续服务。
- 写锁绝不会在上游网络等待期间被持有：模板拉取发生在变更之前、终端刷新发生在变更之后，且在中继请求等待缓慢上游时并发变更仍能完成。
- 热读取绝不解密，任何文件身份变化都让下一次读取观察到新内容，包括不调用任何失效辅助函数的直接磁盘重写；文件缺失或为空使条目失效。
- 每次写入都使用唯一临时文件与原子 rename，因此部分写入绝不可能发布，写入失败保留此前的完整字节；版本门控懒迁移保持原子且一次性，其版本在锁内重新确认。
- 成功发布把归一化值存入读取缓存并驱动差集式调度器清理；写入失败不触碰缓存与调度器。
- 验证：`src-tauri/src/ai_gateway/tests/routing_hardening.rs` 覆盖 AC-001 至 AC-005 与 AC-008 至 AC-009（八线程并发 key 标记、key 标记与结算竞争、重复并发写入者、用户编辑与结算竞争、并发写入下一次性迁移、绕过失效辅助函数的直接磁盘重写所驱动的元数据缓存一致性、以及字节不变的无操作结算），完整 `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway` 套件覆盖迁移、命令、用量日志与接缝套件。
- `MEMORY.md` 已在同一变更中更新串行写入原语、文件身份读取缓存与无操作抑制；持久化配置 schema 保持版本 2、无新字段也无新迁移，Tauri 命令签名与前端不变，回滚只还原并发、缓存与重写行为。
- Supersession：无取代。[Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md) 予以保留并交叉链接；本记录在其懒重写之外增加串行化、版本重确认与读取缓存，而其 schema 门控、一次性懒迁移、遗留文件改名与清理决策继续有效、未变。[API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md) 予以保留并交叉链接；模板拉取现在在写锁之外执行，暂存同步经串行变更持久化，而其增量传播规则、写入边界与「一次原子写入」的结果继续有效。其他活动记录均未记载配置写入或读取机制。
