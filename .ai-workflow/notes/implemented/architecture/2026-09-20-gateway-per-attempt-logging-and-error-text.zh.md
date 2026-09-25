# Agent Note: Gateway Per-Attempt Request Logging and Stored Error Text

Status: implemented

[English](2026-09-20-gateway-per-attempt-logging-and-error-text.md) | 中文

## Problem

请求日志此前对每个入站请求只记录恰好一行，取自网关的最终结果，因此一个在若干服务商失败后又恢复成功的请求不会留下任何记录说明是哪个服务商失败、返回了什么、服务了哪个模型。运维者唯一能看到的失败文本是调用方可见的 `all_providers_unavailable` 信封，它聚合服务商名与状态级原因，不携带各服务商自己的 `error.message`，而且它是响应而不是持久化记录。改为持久化上游原始正文则会越过日志数据库「不保存正文、请求头与凭据」的边界。

## Decision

每个以业务结果正常完成的入站请求按「每个已完成的上游尝试一行 + 该请求恰好一个终止行」写入。每条尝试行绑定该次尝试的服务商与上游模型；当网关完全确定一次尝试的结果时该尝试即完成：非流式响应体已读取、非 2xx 响应、因缺失或非法 SSE 流而被拒绝的响应、空流、建连或流失败，或正常结束的流。已完成的尝试会缓冲到入站请求结果确定；后续候选成功或请求以其他方式正常完成时，缓冲仍保留各服务商的失败归属。本记录原先写明的取消持久化规则已被 [Gateway Cancelled Requests Are Not Logs](../simplification/2026-09-21-gateway-cancelled-requests-are-not-logs.md) 部分取代：下游 transport 取消或响应不可交付会丢弃整个缓冲集合，包括已完成的尝试。

每个正常完成的请求恰好一行是终止行：成功的那次尝试、`ReturnToClient` 的那次尝试、中途流失败的那次尝试、全部候选耗尽时按时间最后完成的尝试、存在合格半开探测的零候选请求所执行的正常探测尝试，或无可服务候选且无合格探测时既有的合成 502 行。耗尽的非流式终止行记录最后一次观测到的上游状态（网络失败记 0）而不是调用方的传输层 502，与流式路径已有的规则一致；无候选行保持 502，且调用方可见的字节不变。用量统计（totals、时间分桶、按模型与按服务商明细、`unpriced_count`）只聚合终止行；按服务商明细另排除 `provider_id=''`，无候选 failure 仍计入 totals/models。尝试了合格半开探测的零候选请求改为写入该探测的正常命名尝试行加其终止行，像其他命名尝试一样归属被探测的服务商。不分组列表、其 `total`、状态与模型筛选以及范围内 `local_model` 面考虑所有可见 success/failure 行，因此正常完成请求的已完成尝试行保持可见。历史 `cancelled` 行在旧数据库首次打开时被一次性删除且兼容表示被移除，两者同属取代取消语义的新决策与版本门控的数据库清理（[Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md)）。

到达过上游的失败在新引入的可空 `error_message` 列中保存可读错误文本：正文携带非空 `error.message` 时取标准 JSON 信封的该字段，否则取正文的可读摘要（lossy UTF-8 解码、去除 HTML 标记、折叠空白并 trim）。传输或流失败保存其描述，从未到达上游的尝试保存 `null`，清洗后为空的文本保存 `null`。写入前把该服务商有序密钥池中每个非空 key 值原样替换为 `[redacted]`、掩蔽 `Bearer` 与 `sk-` 形状的凭据，并把文本截断到 4096 字符（在 Unicode 标量边界截断，仅发生截断时追加 `…`）；请求头、请求正文与完整原始响应体绝不保存。

## Alternatives considered

- 保留每请求一行、仅凭调用方可见的 `all_providers_unavailable` 文本排查失败：未采纳，因为该信封聚合服务商名与状态级原因，不携带各服务商的消息，且其他候选恢复成功的失败完全不会被记录。
- 增加请求标识符与按请求分组视图来替代按尝试行：未采纳，因为扁平请求日志才是运维者已在阅读的界面，标识符加新的分组模式会增加 schema 与界面投入，却不能在列表中把失败归属到服务商。
- 让统计计入尝试行：未采纳，因为多尝试请求会把合计、分桶与明细抬高并使其不再可与历史数据比较，因此所有聚合与分组视图只计入终止行。
- 持久化完整的上游原始正文或整个错误信封：未采纳，因为日志数据库的边界禁止请求正文、请求头与凭据；只能保存提取、清洗并有界的文本。
- 为升级前的行回填消息或非终止状态：未采纳，因为升级前的行本就没有错误文本，把它们缺省视为终止且无消息即可保留其统计含义，无需改写历史。
- 让耗尽的非流式终止行保留调用方可见的传输层 502：未采纳，因为日志应记录上游实际返回的内容，且流式路径已经记录最后一次观测到的上游状态；无论日志状态如何，调用方可见的响应都不变。
- 在读取时而非写入时做脱敏与截断：未采纳，因为存储行本身就是隐私边界——每个读取者、查询与未来的导出都必须自身安全——因此清洗发生在插入之前。

## Consequences

- `usage_logs` 表新增可空 `error_message` 列与非空、缺省即终止的 `terminal` 列，由 `src-tauri/src/api_gateway/usage_log.rs` 中基于 `PRAGMA table_info` 的幂等迁移添加；升级前的数据库保留每一行、按终止且无消息读取，重开不会产生进一步变化，因此历史行的统计与其迁移前数字一致。
- 每个已完成的上游尝试一行，外加每个正常完成请求恰好一个终止行；尝试缓冲由连接处理器持有、转发 future 仅借用。success、failure、重试恢复或无候选请求维持该行为，但探测属于例外：存在合格半开探测的零候选请求写入该探测的正常尝试行加其终止行，而不是合成无候选行；按取代取消语义的新决策，下游 transport 关闭或响应不可交付时丢弃整个缓冲。
- 用量统计（totals、分桶、按模型与按服务商明细、`unpriced_count`）只计入终止行，按服务商明细另排除 `provider_id=''`；无候选 failure 仍计入 totals/models。请求日志页、总数、页数、模型面、按模型/按天分组和时间戳绝不包含 `cancelled` 行——遗留行在旧数据库首次打开时被一次性删除——正常完成 success/failure 请求的已完成尝试行仍可见；耗尽的非流式终止行记录最后一次观测到的上游状态（网络失败记 0），合成无候选行——仅在不存在合格半开探测时写入——保持 502，调用方可见的响应字节不变。
- `error_message` 在正文是合法 JSON 错误信封时保存标准 `error.message`，否则保存可读摘要；传输与流失败保存其描述，无上游与清洗后为空的失败保存 `null`，上游密钥池中每个非空 key 值变为 `[redacted]`，`Bearer` 与 `sk-` 形状凭据被掩蔽，文本在 Unicode 标量边界封顶 4096 字符且仅在截断时追加 `…`；请求头、请求正文与完整原始正文绝不保存。整池范围来自 [Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](2026-09-24-gateway-key-pool-rotation.md) 记录的密钥池决策。
- `UsageLogStore::append_batch` 通过一个连接写入同一请求的全部行并执行保留清理，`append` 仍是单行路径；迁移或日志写入失败仍只是被吞掉的 `log::warn!`，由后续打开重试，绝不影响响应。
- 请求日志记录类型暴露带 serde 缺省的可选 `error_message` 与 `terminal` 字段；不分组列表把可见失败行存储的消息显示为单行截断原因，悬停与键盘聚焦的 tooltip 展示完整消息及 HTTP 状态上下文；无消息时保留通用状态码原因，成功行不显示错误消息，非终止行带轻量 attempt 标签，中英文一致。请求日志记录类型中不再保留任何 `cancelled` 表示，遗留行在旧数据库首次打开时被一次性删除，两者同属取代取消语义的新决策。
- 部分取代：[API Gateway Usage Stats and Request Logs](2026-09-17-ai-gateway-usage-logs.md) 保留并由本记录交叉链接，本记录只取代其每请求一行的记录粒度与不保存错误文本的边界；其 SQLite 日志、记录时价格冻结、保留、`group_by` 契约与模型面决策仍然有效，其 `unpriced_count` 资格由 [Gateway Unpriced Hint Counts Only Billable Usage](../bug-fix/2026-09-21-gateway-unpriced-billable-usage.md) 部分取代。
- 部分取代：[Gateway Single-Candidate Fast Fail and Standard Error Responses](2026-09-18-gateway-fast-fail-and-standard-errors.md) 保留并由本记录交叉链接，本记录只取代其「请求日志失败状态语义不变」的陈述——耗尽的非流式终止行现在与流式路径一样记录最后一次观测到的上游状态——其无候选 502 规则、fallback-first 调度与标准错误信封决策仍然有效。
- 部分取代：[Gateway Cancelled Requests Are Not Logs](../simplification/2026-09-21-gateway-cancelled-requests-are-not-logs.md) 只替换本记录的取消行为：下游 transport 关闭或响应不可交付时不再保留已完成尝试，也不写合成终止行；历史 `cancelled` 行在旧数据库首次打开时被一次性删除且兼容表示被移除（[Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md)）。正常完成的 success、failure、重试恢复与无候选请求的按尝试日志、已存错误文本、迁移缺省值与隐私边界仍然有效。
- 部分取代：[Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast](2026-09-23-gateway-auto-disable-recovery.md) 只替换本记录「无法被任何候选服务的请求总是写入合成无候选行」的陈述；存在合格半开探测的零候选请求改为写入正常的探测尝试行加其终止行。按尝试粒度、仅终止行统计、已存错误文本、迁移缺省值与隐私边界仍然有效。
- `MEMORY.md` 在同一变更中描述该粒度、仅终止行统计规则、终止行状态规则、新增列与错误文本隐私边界；请求日志记录类型新增可选的 `error_message` 与 `terminal` 字段，导航索引已按交付的后端日志行为同步并据此重新生成 `navigation.md`，未新增任何索引符号名或文件路径。
