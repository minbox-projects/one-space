# Agent Note: 网关在下游晚关闭后仍记录已完成的用量

Status: implemented

[English](2026-11-26-gateway-completed-usage-after-client-close.md) | 中文

## Problem

流式调用方在上游自身终态事件到达后立即关闭 socket 时会丢失整条请求日志。网关把每次下游关闭都视为取消，丢弃该入站请求的整个缓冲 usage-log 集合，包括已经完成的尝试。OpenCode 这类流式客户端常常在最后一个助手增量或 `[DONE]` 标记到达后立即关闭连接，因此对调用方而言明显成功的一轮仍可能从请求日志、用量合计与服务商日用量历史中消失。报告的症状是对调用方成功返回完整答案的流式轮次没有产生任何日志行。

## Decision

- 边界是上游自身的终态结果，而不是下游写入。对 OpenAI Chat Completions 是非空 `finish_reason` 或 `[DONE]`；对 Responses 是 `response.completed`、`response.incomplete`、`response.failed` 或 `error`；对原生 Anthropic Messages 是无带内 error 的 `message_stop`。`src-tauri/src/ai_gateway/usage_log/tokens.rs` 在 `SseUsageAccumulator` 上记录 Chat/Responses 信号，`src-tauri/src/ai_gateway/runtime_http/streaming.rs` 判定原生 Messages 信号，并在某次尝试到达完成结果后置起该请求的 `upstream_completed` 标记。
- 在该终态信号之后发生下游写入或 flush 失败时，流式路径抑制后续客户端写入（`src-tauri/src/ai_gateway/runtime_http/http_io.rs` 的 `SuppressibleWriter` 在不触碰已死 socket 的情况下报告成功）并继续消费上游，使已完成的尝试按正常路径追加。随后 `src-tauri/src/ai_gateway/runtime_http/routing.rs` 的 `handle_connection` 至多等待 `DOWNSTREAM_CLOSE_GRACE = 2` 秒让在途转发结束，并以缓冲的 attempts 和恰好一个终止行持久化。
- 宽限窗口短且有界，因为它只需覆盖客户端关闭后跟随的最后终态字节；更长的窗口会把上游仍在缓慢输出的真实取消流误读为完成。
- late-close 路径只写日志。它既不结算会话亲和，也不发布模型池同轮延续偏好，因为调用方未收到已完成的交付。
- 在上游终态之前的关闭仍是真实取消，包括响应头写出前的关闭或写失败：它会及时取消未完成的转发、重试等待与后续尝试，并写入零行。无法交付的非流式响应保持同样的零行规则。

## Alternatives considered

- 在观察到终态事件时于流式循环内部记录日志：未采纳，因为循环负责字节转发而非持久化；在此写入会让尽力而为的存储与转发交错，重复 `handle_connection` 对所有路径施加的终止行规则，并且仍需要请求级的尝试缓冲。
- 无论终态信号如何，对所有被取消的上游流都排空宽限窗口：未采纳，因为这会为调用方真实取消的请求伪造完成；从未到达自身终态事件的流不是已完成的业务结果。
- 对所有下游关闭保留零行规则：未采纳，因为这会丢弃已经产出完整答案与用量的请求行，而这正是报告的症状，并会让用量合计与服务商日用量历史少计真实花费。
- 把 late-close 持久化扩展到非流式响应：未采纳，因为非流式响应以单个缓冲写出，交付失败意味着调用方什么都没收到、响应从未被交付；不存在客户端已观察到的终态事件来支持持久化。

## Consequences

- 上游已到达终态的流式请求，即使客户端随后立即关闭，也会以按尝试行和恰好一个终止行记录；用量合计与服务商日用量历史包含这些行，且没有 schema、DTO 或存储格式变更。
- 在上游终态之前的客户端关闭、任何响应字节之前的关闭，以及无法交付的非流式响应仍写入零行，且不产生合成 `cancelled` 行。
- late-close 路径不执行会话亲和结算，也不发布池延续偏好，因此路由状态仍只反映调用方实际收到的响应。
- 部分取代：[Gateway Cancelled Requests Are Not Logs](../simplification/2026-09-21-gateway-cancelled-requests-are-not-logs.md) 保留其真实取消规则、移除合成 `cancelled` 行以及其余全部决定，但其「每次下游关闭都丢弃缓冲的已完成 attempts」的规则由本 late-close 边界取代。
