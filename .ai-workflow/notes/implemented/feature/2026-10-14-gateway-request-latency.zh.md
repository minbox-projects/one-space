# Agent Note: Gateway Request Latency: Observed First-Token and Generation TPS

Status: implemented

[English](2026-10-14-gateway-request-latency.md) | 中文

## Problem

请求日志列表此前每行只显示一个整次尝试的 `duration_ms` 与一种耗时颜色。这个单一数值无法区分「首字慢」的模型与「首字快但后续出字慢」的模型，也没有给出输出速率。用显示时四舍五入后的耗时反推速率会失真，而为缓冲的非流式响应估计首字时间、或从文本长度估计 token，都会捏造网关从未观测到的数据。变更还必须兼容旧数据库与旧代码写入，并保持「未测量」与「测量到零」可区分。

## Decision

首字延迟与平均 TPS 由只读的 `OutputTimingObserver`（`src-tauri/src/ai_gateway/output_timing.rs`）按每次上游尝试测量。它在尝试的 `tokio::time::Instant` 起点新建，喂入与 usage 采集相同的原始上游分片、且在等待的下游写出之前，因此观察的是转换前的真实上游协议；它在健康结算、持久化与下游完成之前结束，因此观测间隔不包含记账时间。首字延迟是尝试起点到首个有意义上游输出的单调时钟间隔；平均 TPS 是该尝试有效且被显式上报的累计输出 token 数除以未取整的整次尝试间隔，已由 [Gateway Whole-Attempt Average TPS](../bug-fix/2026-10-16-gateway-whole-attempt-tps.md) 在同一交付中修订。

有意义输出为非空文本、推理/思考、生成的工具或函数名，或生成的工具参数，并按真实上游协议解释：Chat Completions（含已弃用的 `delta.function_call` 名称/参数）、Responses 的输出文本/推理/函数参数增量以及已带名称或参数的 function_call 条目，以及 Anthropic Messages 的文本/思考/input-JSON 增量再加初始即带内容的 `content_block_start` text、thinking 或 `tool_use` 块。心跳或注释、空增量、仅角色或其他元数据事件、仅 usage 事件、错误事件、完成标记与仅标识符的工具信封都不是输出。非空字符串内的空白属于输出，绝不裁剪。增量 SSE 缓冲处理 LF 与 CRLF 分帧、多行 `data:`、分片事件以及没有空行终止符的最后一个有效事件（在 finish 时识别）。

上报的输出计数经独立于输入/缓存核算的校验：`CanonicalUsage.reported_output_tokens` 优先选 `output_tokens`、其次 `completion_tokens`，只接受非负整数或其受支持的数字字符串形式。缺失、null、负数、小数与畸形值保持未知、绝不变成零，而合法的非负整数即使输入/缓存分解无效也被接受。计费 token 档位、费用与缓存有效性不变。显式零计数配测量到的正间隔得到 `0.00 t/s`；缺失计数、缺失首观测或非正生成间隔不产生 TPS；测量到的零首字偏移仍是有效观测。

`UsageLogRecord` 新增 serde 默认的可选字段 `first_token_ms` 与 `avg_tps`，`usage_logs` 经既有幂等加列迁移新增可空 `first_token_ms REAL` 与 `avg_tps REAL` 列；两列在单行与批量插入以及筛选、分页查询中都被投影。历史值绝不回填。非流式、合成无上游、无输出与输出前失败的行，以及旧代码写入的行，两列保持 null。`duration_ms` 保持既有毫秒分辨率。

不分组请求列表的列标签为 Latency / 延迟，显示三行对齐值——Total / 总耗时、First token / 首字与 Average TPS / 平均 TPS——秒保留三位小数、TPS 保留两位小数，`s` 与 `t/s` 单位以弱化样式单独显示，数值右对齐并使用表格数字，标签弱化；只有数值带颜色，不可用值渲染单个弱化的 `—`。颜色按显示取整前的原始值使用相互独立的分段：

| Metric | Green | Orange | Red |
| --- | --- | --- | --- |
| Total duration (`s`) | `< 3` | `3 <= value <= 15` | `> 15` |
| First token (`s`) | `< 1` | `1 <= value <= 3` | `> 3` |
| Average TPS (`t/s`) | `>= 60` | `20 <= value < 60` | `< 20` |

前端 helper 位于 `src/lib/aiGateway.ts`：`formatGatewayDuration`（毫秒转三位小数秒）、`formatGatewayTps`、`getGatewayDurationColorClass`、`getGatewayFirstTokenColorClass` 与 `getGatewayTpsColorClass`；每个颜色 helper 对 null、undefined、非有限或负值返回弱化前景色，否则返回可读的浅色 `*-600` 加深色 `*-400` 变体。`UsageLogsPanel.tsx` 的 `LatencyMetricRow` 渲染单行对齐值，双语键 `aiGatewayLogsLatencyTotal`、`aiGatewayLogsLatencyFirstToken` 与 `aiGatewayLogsLatencyAverageTps` 承载标签，列键沿用 `aiGatewayLogsDurationColumn`。

## Alternatives considered

- 把尝试的首个传输字节当作首字观测：未采纳，因为注释心跳、仅角色或空增量、仅 usage 事件或生命周期信封都会早于真实输出到达，因此早期分片会抬高表观速率；观测等待首个完整、有效、有意义事件。
- 用整次尝试时长、或用取整或钳制后的 `duration_ms` 作被除数：最初未采纳，而选用未取整的首输出到结束间隔，独立算术示例为 `200 / (5 - 1) = 50`、`200 / 3.5` 与 `200 / 0.0005 = 400000`；该选择后被推翻，[Gateway Whole-Attempt Average TPS](../bug-fix/2026-10-16-gateway-whole-attempt-tps.md) 现以整次尝试间隔作分母。
- 为历史、非流式、合成无上游或无输出的行臆造首字时间或 TPS，或从文本长度估计 token：未采纳，因为缓冲响应无法重建流式时序、文本长度也不是上报计数；不可用保持 null 并渲染弱化的 `—`。

## Consequences

- 不分组请求列表以中英双语、明暗主题显示三行对齐的 Total / First token / Average TPS 与独立颜色分段；分组视图、其他列、筛选与分页不变。
- `first_token_ms` 与 `avg_tps` 是可空列与 serde 默认可选字段；重新打开旧数据库会加列并保留每一既有行，旧投影插入仍成功且两列为 null，测量到的零首字偏移或零 TPS 在前端仍与不可用值区分。
- 速率是由 [Gateway Whole-Attempt Average TPS](../bug-fix/2026-10-16-gateway-whole-attempt-tps.md) 现定义的整次尝试输出速率；它排除输入与缓存 token、重试等待与更早的尝试，且绝不改变 token 档位、费用或缓存命中率算术。
- 已产生输出的失败流保留其首字测量，以及基于它实际观测到的累计输出用量得到的 TPS；下游取消仍丢弃整个缓冲尝试集合、不写任何行。
- 部分取代：[Gateway Whole-Attempt Average TPS](../bug-fix/2026-10-16-gateway-whole-attempt-tps.md) 仅取代本记录的平均 TPS 分母与读取复投影预期，本记录其余部分仍然有效。[API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) 与 [Gateway Per-Attempt Request Logging and Stored Error Text](../architecture/2026-09-20-gateway-per-attempt-logging-and-error-text.md) 保留并交叉链接；本记录以首字与计时测量扩展它们的尝试行、可空列与隐私契约，两者都不被改写或归档。
- `MEMORY.md` 与 `ai-gateway`、`ai-gateway-backend` 两个导航条目记录观察者路径、新测试路径与公共 helper，`navigation.md` 在同一变更中按权威 JSON 重新生成。
