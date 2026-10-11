# Agent Note: Native Anthropic Messages Entry and Protocol Recognition

Status: implemented

English | [中文](2026-10-11-ai-gateway-anthropic-messages.zh.md)

## Problem

The AI Gateway relayed only the OpenAI-compatible `/v1/chat/completions` and `/v1/responses` entries. A provider's persisted `protocol` knew only `chat_completions` and `responses`, the model-list source parser dropped any entry whose declared `supported_endpoints` named neither Chat Completions nor Responses, and the loopback server answered `/v1/messages` as an unknown path. A Messages-only upstream could therefore be neither configured nor called natively, and Claude Code, which speaks the native Messages protocol, had no entry through the gateway.

## Decision

- Persisted protocol: `UpstreamProtocol::AnthropicMessages` is added in `src-tauri/src/ai_gateway/types_config.rs` and serialized as `anthropic_messages`; it is accepted at the provider default, at each mapping row, and at the template default and each template model, and `endpoint_path()` returns `/messages`. The exhaustive protocol matches in `src-tauri/src/ai_gateway/migration.rs` handle the new variant as compatibility plumbing; the configuration schema stays 3 and no migration is introduced.
- Source recognition: `src-tauri/src/ai_gateway/templates.rs` recognizes `/messages` and `/v1/messages` in a declared `supported_endpoints` under the Chat Completions > Responses > Messages preference; an entry declaring none of the three is dropped, and an entry without endpoint information keeps no protocol and inherits the template. Generic inheritance and the Go declaration-aware path are preserved, a source-declared Messages endpoint now enters the compatible set, and disabled/ignored state and atomic synchronization are unchanged.
- Native route modules: `src-tauri/src/ai_gateway/anthropic.rs` provides `is_messages_path`, `DEFAULT_ANTHROPIC_VERSION` (`2023-06-01`), the Anthropic `error_payload` and the `stream_error_event` fragment; `src-tauri/src/ai_gateway/anthropic_sse.rs` provides the read-only `AnthropicSseObserver`.
- Native entry: `src-tauri/src/ai_gateway/runtime_http.rs` serves `POST /v1/messages` including query strings such as `?beta=true` (`/messages` is tolerated), authenticates locally with an `Authorization: Bearer` or `x-api-key` local key, answers gateway-generated errors with the Anthropic error envelope/event, substitutes only the mapped model, sends the selected upstream pool key as `x-api-key`, passes `anthropic-beta` and `anthropic-version` through, and supplies `anthropic-version: 2023-06-01` only when the caller omits it.
- Native stream and accounting: successful native bytes are relayed unchanged with no OpenAI `[DONE]`; a native stream succeeds only after `message_stop` and an in-band `error` event is never a success. `AnthropicSseObserver` reads the forwarded bytes only, merging `message_start.message.usage` with cumulative `message_delta.usage` field-wise (a present value replaces, an omitted value is retained) before the existing canonical normalization; the existing first-client-byte retry boundary, cancellation, idle-read timeout and health settlement are unchanged.

## Alternatives considered

- Keep an OpenAI-only protocol enum and give the Messages entry its own separate configuration: declined because the provider default and the mapping row already express the effective protocol, and a parallel configuration surface would duplicate template binding, key pools and selection.
- Convert Chat Completions and Responses input into Messages in this step: declined for Step 1 scope; the native entry is delivered first and conversion is deferred, so Step 1 keeps native passthrough only.
- Guess a protocol for undeclared or metadata-less model ids, or reuse the OpenAI entries for Messages: declined because guessing would misroute a model that never declared a Messages endpoint and no caller conversion exists yet.
- Rewrite or normalize native stream bytes, or inject an OpenAI terminator: declined because native callers and this gateway are byte-faithful relays, and injecting `[DONE]` or rewriting events would corrupt the Messages protocol.

## Consequences

- A provider default or mapping row can persist `anthropic_messages` and serve it through the native `POST /v1/messages` entry; the mapping is not selectable by the OpenAI entries until conversion is delivered, and a native Messages request never selects a Chat Completions or Responses upstream.
- Source synchronization recognizes `/messages` and `/v1/messages` after Chat Completions and Responses, so a Messages-declared model survives a sync while an entry declaring none of the three is still dropped and undeclared metadata-less ids stay excluded.
- Native SSE stays byte-exact and split usage merges field-wise: the registered fixture merges `message_start` input/cache-read/cache-write with the last cumulative output snapshot to `10/20/3/7`, an EOF without `message_stop` and an in-band error are recorded as failures, and the existing retry, cancellation, timeout and health boundaries are retained.
- Compatibility and permanent difference: the configuration schema stays 3 with no new migration, and existing OpenAI payloads keep their behavior apart from the existing model rewriting; no Chat/Responses-to-Messages conversion, Claude terminal sync, UI protocol selectors/dialog or terminal adapter is delivered by this step, and no curated model or price list is added.
- `MEMORY.md`, the authoritative `ai-gateway-backend` navigation entry and the regenerated `navigation.md` describe the new modules, symbols, tests and behavior in the same change.
- Partial supersession: [API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md) keeps its incremental merge, ignored-model lifecycle and naming decisions while its endpoint filtering and recognition gain Messages; [Provider Templates Own Closed Strategies and Bind Providers by Template Id](2026-10-10-provider-template-bound-strategies.md) keeps its closed strategy fields, binding validation and effective-policy resolution while its Go declaration baseline now admits a source-declared Messages id; [API Gateway Cache Hit Rate Normalizes Provider Usage Semantics](../bug-fix/2026-09-21-api-gateway-cache-hit-accounting.md) keeps its four-tier normalization, storage migration, eligibility predicate and coverage contract while native Messages usage is observed by a separate field-wise merge.
