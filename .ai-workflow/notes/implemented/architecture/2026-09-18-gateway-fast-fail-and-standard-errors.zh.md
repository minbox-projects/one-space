# Agent Note: Gateway Single-Candidate Fast Fail and Standard Error Responses

Status: implemented

[English](2026-09-18-gateway-fast-fail-and-standard-errors.md) | 中文

## Problem

中继此前总把可重试的服务商失败排入冷却/退避队列，即使本次请求只有一个可服务候选也一样。既然没有可回退的其他上游，这些额外尝试只会与 AI 工具客户端自身的重试和退避相乘，使一个普通的上游 500 或 429 长时间被隐藏而不是以错误暴露出来。错误响应同样不统一：网关自身生成的错误按各自位置使用不同形状（all-unavailable 载荷没有 `param`，配置读取失败没有 `type`）；`stream: true` 请求在写出任何字节前确定的失败被返回为 HTTP 200 SSE 加错误事件和 `[DONE]`，于是兼容 OpenAI 的客户端会把失败请求当成一个正常打开的流；非标准的上游 4xx 正文（HTML 或纯文本）被原样透传，期望 `error.message` 的客户端无法阅读；首个字节写出后的上游流读取失败则在没有任何截断信号的情况下直接关闭流。

## Decision

解析结果恰好为一个可服务候选的请求只进行唯一一次初轮尝试。`attempt_non_streaming` 与 `attempt_streaming` 仅在 `ordered.len() > 1` 时把可重试失败加入重试队列，因此单个候选绝不进入冷却/退避队列、不应用 `Retry-After` 等待、不消耗 120 秒等待预算，失败立即走既有终止路径。该规则对非流式与流式一致；多候选调度——fallback-first 初轮、按最早冷却截止时间串行重试、`Retry-After` 优先级、每服务商最多 `MAX_RETRIES_PER_PROVIDER`（5）次重试、120 秒预算——保持不变。

网关自身生成的每个错误现在共用由 `error_envelope` 构建的信封 `{"error": {"message": ..., "type": ..., "code": ..., "param": null}}`，其中 `message`、`type`、`code` 均非空，`param` 恒为 `null`。它覆盖请求解析失败、配置读取失败、未知路径、未授权、`/v1/models` 或转发路径上的方法错误、请求体非法与无候选，其中 `all_unavailable_payload` 保持 `code: all_providers_unavailable`。

对 `stream: true` 请求，在写出任何字节前确定的失败（无可服务候选，或所有候选已耗尽）写为 HTTP 502 加 `content-type: application/json` 加标准信封，而不是 HTTP 200 SSE，因此该情形的传输不再发送带 `[DONE]` 的错误事件。请求日志语义刻意保持不变：无候选仍记录状态 502，耗尽的流仍记录最后一个可确定的上游状态（无法确定上游 HTTP 状态时记 `0`，例如网络错误），绝不因为传输是 502 就改写为固定 502。

对 `selection::classify_failure` 归类为 `ReturnToClient` 的上游 4xx（例如 400、413、422），由 `is_standard_error_body` 决定正文：是含 `error` 对象的合法 JSON 时按字节原样透传；其他正文保留原上游状态并由 `upstream_error_payload` 包装进标准信封，message 指明上游状态并携带可读正文，绝不含本地或上游凭据或请求头。非流式与流式分支使用同一规则。

当上游流在首个字节已转发后读取失败，`attempt_streaming` 先补全 SSE 事件边界——仅当最后一个转发字节不是换行时补一个换行——再追加一个独立的 `data: {"error": {...}}\n\n` 分片（`type: server_error`、`code: upstream_stream_error`）并关闭。它绝不发送 `data: [DONE]`、绝不重试、绝不切换候选，且请求仍只记录一次状态 502 的失败日志并保留已累积的 usage。

## Alternatives considered

- 继续重试单个候选：未采纳，因为只有一个候选的请求没有可回退的其他上游，额外尝试只会推迟错误，而且会与 AI 工具客户端自身的重试和退避相乘。
- 保留流式预写出失败为 HTTP 200 SSE 加错误事件和 `[DONE]`：未采纳，因为兼容 OpenAI 的流式客户端会把 200 读作已打开的流，无法呈现 `error.message`，而 502 JSON 信封才是它已经理解的信号。
- 原样返回每个上游 4xx 正文：未采纳，因为期望 OpenAI 错误形状的客户端无法阅读 HTML 或纯文本正文；只有已携带 `error` 对象的正文才透传。
- 标准化每个上游 4xx 正文，包括标准正文：未采纳，因为重新编码一个合法的标准错误可能改变其字节并丢失上游细节，标准正文必须按字节原样保留。
- 静默关闭被截断的流：未采纳，因为客户端无法区分正常结束与被截断的流；在补全事件边界后追加一个独立可解析的错误分片才能让截断可见。
- 在中途错误分片之后发送 `[DONE]`：未采纳，因为 `[DONE]` 标记的是成功结束，客户端会把失败的流记为完整。
- 保留此前各处不同的错误形状：未采纳，因为兼容 OpenAI 的客户端只读取一种信封形状，所以 `param` 与 `type` 必须始终存在，且每个网关自身生成的错误都使用相同字段。

## Consequences

- 单候选的 500 或 429 只产生一次上游请求并立即返回标准错误；多候选的 fallback-first 调度、`Retry-After` 优先级、每服务商上限与 120 秒预算均不变，原单候选调度用例已迁移为两个候选以保留覆盖。
- 在写出首字节前失败的 `stream: true` 请求现在返回 HTTP 502 加 `application/json` 且 `error.code == "all_providers_unavailable"`；而首字节写出后失败的请求仍返回已打开的 SSE 流，但现在以一个 `type: server_error` / `code: upstream_stream_error` 分片结束且不含 `[DONE]`。
- 标准上游 4xx 正文与上游正文按字节完全一致；非标准正文保留上游状态并被包装进标准信封；两条路径都不会泄露凭据或请求头。
- 统一信封 `{"message","type","code","param":null}` 就是网关自身错误的契约，因此配置读取失败现在带 `type`，all-unavailable 载荷现在带 `param`。
- 请求日志的失败状态语义不变：无候选记录 502，耗尽的流记录最后一个可确定的上游状态（无法确定时记 `0`），绝不改写为固定 502。
- `MEMORY.md` 已在同一变更中更新失败分类、重试调度与错误响应标准；配置 schema、Tauri 命令签名、`api_gateway.json` 与 `api_gateway_usage.db` 均未改变，因此无需迁移，回滚只会还原行为。
- 部分取代：[API Gateway Usage Stats and Request Logs](2026-09-17-ai-gateway-usage-logs.md) 予以保留并交叉链接。其记录的备选前提「全部上游不可用的流式请求返回 HTTP 200 SSE」对预写出失败已不再成立，而其日志决策——结果取自网关最终状态并记为 `failure`，在未捕获用量时不产生金额贡献——仍然有效；其 `unpriced_count` 资格由 [Gateway Unpriced Hint Counts Only Billable Usage](../bug-fix/2026-09-21-gateway-unpriced-billable-usage.md) 部分取代，其余 `unpriced_count` 决策仍然有效。其他 active note 均不受影响：本地 Key、终端同步、聚合模型与双语文档记录互不相关，terminal-independent-provider 记录也不描述重试或错误传输。
- 部分取代：本记录由 [Gateway Per-Attempt Request Logging and Stored Error Text](2026-09-20-gateway-per-attempt-logging-and-error-text.md) 保留并交叉链接，后者只取代上文的请求日志失败状态表述——耗尽的非流式终止行现在与流式路径一样记录最后一次观测到的上游状态——而本文的无候选 502 规则、fallback-first 调度与标准错误信封决策仍然有效。
