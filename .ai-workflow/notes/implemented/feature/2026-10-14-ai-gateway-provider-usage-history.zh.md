# Agent Note: AI Gateway Provider Daily Usage History

Status: implemented

[English](2026-10-14-ai-gateway-provider-usage-history.md) | 中文

## Problem

网关此前在 `ai_gateway_usage.db` 中按每个完成的尝试记录一行请求日志明细，并完全从该明细表回答用量查询与日志查询。配置的明细保留（默认 90 天，可接受范围 1–365）会在每次写入时永久删除更旧的行，因此也删除了统计的唯一依据：随着行到期，可见的最早日期不断前移，按服务商的合计只能从尚存的明细重算，服务商无法跨越超过保留窗口的时期被查看。普通分组与筛选以记录的服务商显示名为键，而该名称既不唯一也不稳定，因此被重命名的服务商分裂为两条序列、同名服务商被合并、已删除的服务商完全无法触达。既没有持久的按服务商日历，也没有稳定身份，更无法区分「确实没有请求的一天」与「记录早已被删除的一天」。

## Decision

在当前 profile 的 `ai_gateway_usage.db` 中，于请求日志明细之旁持久化只增的按服务商日汇总，并通过新增的 Provider history 子页签展示。

| Table | Key | Stored facts |
| --- | --- | --- |
| `provider_daily_usage` | `(provider_id, day)`，其中 `day` 为 UTC+8 `YYYY-MM-DD` | 写入时记录的身份与名称、首末请求时间戳，以及与明细行可对账的只增指标 |
| `provider_usage_metadata` | `continuity_start_day` | 首次创建汇总时盖上的连续记录边界 |

只存储真正记录了终止用量的日行，且只存只增指标加记录的身份/名称与首末时间戳；完整的 UTC+8 日历、零与不可用的分类以及一切比率都在读取时生成，累计总量对只增日值求和，缓存命中率由合格分子与分母求和后一次性派生，绝不平均百分比。该功能是 profile 本地的，不存储任何请求或响应正文、请求头与凭据。

用量数据库经其自身的 `PRAGMA user_version` 在永久迁移模块内从 1 推进到 2，而加密配置 schema 保持 3。在 `BEGIN IMMEDIATE` 写事务内，迁移在取得写边界后重读版本，仅当存储版本低于 1 时才执行遗留 `result = 'cancelled'` 清理，随后创建汇总表、回填保留的、结果为成功或失败且 provider id 非空的终止行、写入连续边界并盖上版本 2。回填是一个事务，因此重复或并发打开都不会重复回填，迁移失败会整体回滚、不留部分汇总也不推进版本，并在之后的打开中重试；原始 tokens、金额与 total 字段绝不改写。

明细插入与其合格的日贡献在单行（`append_with_accounting`）与批量（`append_batch_with_accounting`）两条追加路径上都于同一事务内提交，保留清理仅在该单元成功后运行且只删除明细行，因此批量中途或汇总更新失败既不留下部分明细行也不留下部分汇总。贡献复用既有的缓存合格与未定价谓词，尝试与空 provider 行绝不贡献。历史读取使用同一个一致读事务，因此总量与日值不会看到不同的追加快照。

在既有四个命令之外新增两个：`ai_gateway_usage_providers` 返回配置身份与历史身份的并集、以精确非空 provider id 为键（当前配置名优先、已删除身份取最近记录名、未使用的已配置服务商返回 `has_usage = false`），`ai_gateway_provider_usage_history(provider_id)` 返回日历、覆盖与指标（空白 id 报错、未知非空 id 返回空历史）。`ai_gateway_request_logs` 新增可选 `provider_id` 与 `day` 参数：严格的 UTC+8 `YYYY-MM-DD` 半开窗口优先于快捷范围，并与既有名称、状态、模型与分组筛选按 AND 组合，空白或非法值为可操作错误、未知非空 id 返回空数据，两者都省略时保持既有六键调用负载不变。

在前端，`ProviderUsageHistoryPanel` 是 Usage and logs 内的第三个子页签。稳定 ID 选择器使用身份并集并做同名消歧、已删除标记与未使用服务商空态。它展示最早可用日、连续记录起点与当前结束日、较早保留历史可能不完整时的覆盖说明、累计卡、可横向滚动的 token/请求/花费趋势以及跨年保留完整日期的完整逐日表格；不可用历史日显示占位符而非数值 0，今天标记为进行中，只有连续覆盖内的无请求日显示 0。某一行可打开既有请求日志视图并聚焦到精确 provider id 与 UTC+8 日：聚焦一次性生效、从第 1 页开始并保留 attempts 可见与 50 行分页、返回时恢复同一服务商历史，而显式清除会恢复普通快捷范围浏览、模型池快捷方式会清除 provider/day 聚焦并仍重新筛选其别名。它区分「保留明细为空」「普通筛选无匹配」与「查询错误」，并在未激活时不发起查询、拒绝被取代的响应。所有新增文本均为中英双语。

## Alternatives considered

- 只保留永久明细行，或按需重扫并重算整个历史，而不做持久的日汇总：不采用，因为永久保留与配置的 1–365 天明细边界相矛盾，而终身重扫会随着明细过期与价格变动重新计算数值，而存储的只增汇总能在清理后存活并让已记录金额保持权威。
- 以记录的服务商名而非稳定 id 对新历史分组：不采用，因为名称既不唯一也不稳定，同名服务商会被合并、被重命名的服务商会分裂；普通日志分组与筛选保留其记录名语义，新历史是稳定 ID 的补充而非替代。
- 把迁移前的缺失日归类为 0 而非不可用：不采用，因为迁移只能恢复仍然存在的行，缺失的历史日可能只是早已被清理；0 只留给已知连续覆盖内的无请求日，较早的保留数值被限定为可能不完整。
- 用相互独立的语句分别写入明细行与汇总贡献：不采用，因为两者之间崩溃会留下明细行缺汇总、或汇总没有明细；两条追加路径各自单事务、随后保留清理只删明细行，可保持二者一致。

## Consequences

- 两张汇总表与用量库版本 2 都是只增的；加密配置 schema 保持 3，请求日志列与普通查询语义不变，不维护汇总的旧二进制在写入期间会暂停连续性保证。
- 明细保留仍在配置的 1–365 天窗口（默认 90）内永久删除请求日志行，但不再删除任何日汇总，因此按服务商日历与累计总量会保留到数据库本身被删除为止。
- 回滚会移除功能代码而不做破坏性降级迁移并保留只增表；降级前必须保存一份匹配的当前 profile 数据库备份，回到新二进制时需要显式把降级区间视为连续性保证之外，或恢复该备份，因为降级区间内被删除的流量无法恢复，且本变更不新增任何自动降级对账或缺口记录 API。
- 六个命令 `ai_gateway_usage_stats`、`ai_gateway_request_logs`、`ai_gateway_usage_retention_get`、`ai_gateway_usage_retention_save`、`ai_gateway_usage_providers` 与 `ai_gateway_provider_usage_history` 经 `ai_gateway` 模块暴露并在 `app_runtime/run_app.rs` 注册；前端使用类型化封装与 DTO，不直接拼装命令字符串。
- `MEMORY.md` 与 `ai-gateway` / `ai-gateway-backend` 的导航职责描述该存储、身份、覆盖与命令契约，`navigation.md` 由权威 JSON 重新生成；`docs/USAGE.md` 说明第三个子页签、最早可用日并非服务商实际首次使用、只有保留历史可恢复、普通统计仍受明细保留限制，以及日历史是持久的。
- 部分取代：[API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) 保留并交叉链接；本记录只取代其中记录的统计生命期边界——普通统计仍由保留的明细行计算，而新的 provider/day 汇总独立于明细清理而持久存在——其 SQLite 日志、记录时价格冻结、保留、`group_by` 契约与 `local_model` 面决策仍然有效。
- [Gateway Request Logs Group and Filter by Provider Name](2026-09-28-gateway-log-provider-grouping-filter.md) 的记录名服务商分组与筛选决策继续支配普通请求日志分组与筛选；此处的稳定 ID 历史是补充，不改变该契约。
- 用量库版本在 [Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md) 所记录的永久版本门控下从 1 推进到 2，其迁移决策与 `cancelled` 清理门控保持不变。
