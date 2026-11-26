# Agent Note: Gateway Logs Completed Usage After a Late Client Close

Status: implemented

English | [中文](2026-11-26-gateway-completed-usage-after-client-close.zh.md)

## Problem

A streaming caller that closed its socket immediately after receiving the upstream's own terminal event lost its entire request-log entry. The gateway treated every downstream close as a cancellation and discarded the inbound request's whole buffered usage-log set, including attempts that had already completed. Streaming clients such as OpenCode routinely close the connection as soon as the last assistant delta or the `[DONE]` marker arrives, so a turn that visibly succeeded for its caller could still be absent from the request log, from usage totals and from provider daily history. The reported symptom was an apparently successful streaming turn that produced no log row even though the caller had received the complete answer.

## Decision

- The boundary is the upstream's own terminal outcome, not the downstream write. For OpenAI Chat Completions that is a non-null `finish_reason` or `[DONE]`; for Responses it is `response.completed`, `response.incomplete`, `response.failed` or `error`; for native Anthropic Messages it is `message_stop` without an in-band error. `src-tauri/src/ai_gateway/usage_log/tokens.rs` records the Chat/Responses signal on `SseUsageAccumulator` and `src-tauri/src/ai_gateway/runtime_http/streaming.rs` decides the native Messages signal, then raises the request's `upstream_completed` flag once an attempt reaches a completed outcome.
- On a downstream write or flush failure after that terminal signal, the streaming path suppresses further client writes (`SuppressibleWriter` in `src-tauri/src/ai_gateway/runtime_http/http_io.rs`, which reports success without touching the dead socket) and keeps consuming the upstream, so the finished attempt is appended normally. `handle_connection` in `src-tauri/src/ai_gateway/runtime_http/routing.rs` then waits at most `DOWNSTREAM_CLOSE_GRACE = 2` seconds for the in-flight forward to finish and persists the buffered attempts with exactly one terminal row.
- The grace window is short and bounded because it only has to cover the last terminal bytes that follow the client's close; a longer window would misread a genuinely cancelled stream whose upstream keeps trickling as a completion.
- The late-close path logs only. It neither settles session affinity nor publishes the model-pool same-turn continuation preference, because the caller did not receive the completed delivery.
- A close before the upstream terminal outcome is still a true cancellation, including a close or write failure before the response header is written: it cancels unfinished forwarding, retry waits and later attempts promptly and writes zero rows. A non-streaming response that cannot be delivered keeps the same zero-row rule.

## Alternatives considered

- Log inside the streaming loop at the moment the terminal event is observed: declined because the loop owns byte forwarding rather than persistence; writing there would interleave best-effort storage with forwarding, duplicate the terminal-row rule that `handle_connection` applies to every path, and still need the request-level attempt buffer.
- Drain every cancelled upstream stream for the grace window regardless of the terminal signal: declined because it would fabricate a completion for a request the caller genuinely cancelled; a stream that never reached its own terminal event is not a completed business outcome.
- Keep the zero-row rule for every downstream close: declined because it discards rows for requests that already produced their complete answer and usage, which is exactly the reported symptom, and makes usage totals and provider daily history undercount real spend.
- Extend late-close persistence to non-streaming responses: declined because a non-streaming response is written as a single buffer, so a delivery failure means the caller received nothing and the response was never delivered; there is no terminal event the client already observed that would justify persistence.

## Consequences

- A streaming request whose upstream reached its terminal outcome is logged with its per-attempt rows and exactly one terminal row even when the client closes immediately afterwards; usage totals and provider daily history include those rows, and no schema, DTO or storage format changed.
- A client close before the upstream terminal outcome, a close before any response byte, and a non-streaming undeliverable response still write zero rows and produce no synthetic `cancelled` row.
- The late-close path performs no session-affinity settlement and no pool continuation publication, so routing state still reflects only responses a caller actually received.
- Partial supersession: [Gateway Cancelled Requests Are Not Logs](../simplification/2026-09-21-gateway-cancelled-requests-are-not-logs.md) keeps its true-cancellation rule, the removal of the synthetic `cancelled` row and every other decision, but its rule that every downstream close discards the buffered completed attempts is replaced by this late-close boundary.
