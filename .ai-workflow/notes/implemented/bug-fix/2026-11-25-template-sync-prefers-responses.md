# Agent Note: Template Model-List Sync Prefers Responses Endpoints

Status: implemented

English | [中文](2026-11-25-template-sync-prefers-responses.zh.md)

## Problem

Template model-list sync derives each model's upstream protocol from the declared `supported_endpoints`, and it preferred Chat Completions over Responses. A catalog entry that advertises both `/responses` and `/chat/completions` therefore synced as `chat_completions`, so the Responses capability the source declared could not be used by a Responses caller such as Codex through the template-derived provider, and the derived mapping's protocol contradicted its source. The official CommandCode catalog carries such entries, for example `gpt-5.6-luna` and `gpt-5.6-sol`.

## Decision

- `src-tauri/src/ai_gateway/templates/source.rs` resolves a declared `supported_endpoints` array as Responses > Chat Completions > Anthropic Messages: `/responses` wins over `/chat/completions` and `/chat/completions` wins over `/messages` and `/v1/messages`, so a model declaring both `/responses` and `/chat/completions` now syncs as `responses`.
- The rest of the recognition is unchanged: a chat-only entry stays `chat_completions`, a responses-only entry stays `responses`, `/messages` and `/v1/messages` stay `anthropic_messages`, a missing or non-array field keeps the entry with no protocol so it inherits the template protocol, and an array naming none of these endpoints still drops the entry.
- The preference puts the wider advertised capability first because a declared Responses endpoint must stay usable by Responses callers such as Codex, while the OpenCode terminal sync already gives a genuinely Responses-only local model the `@ai-sdk/openai` npm override, so a model that ends up served over Responses only keeps its terminal override.

## Alternatives considered

- Keep Chat Completions first: declined because it hides support the source advertises and blocks a Responses caller the upstream can serve.
- Choose the protocol from a GPT or model-name heuristic: declined because a model name is not protocol authority; only the declared endpoints are.
- Keep new syncs on Chat and honor Responses only for a provider already configured as `responses`: declined because a newly synced model would still start as `chat_completions` and the advertised Responses support would stay unreachable until an operator edited the row by hand.

## Consequences

- A model declaring both `/responses` and `/chat/completions` now syncs as `responses`, while a chat-only model keeps `chat_completions`; the examples `gpt-5.6-luna` and `gpt-5.6-sol` now sync as Responses.
- The change reaches an existing derived provider on its next template sync, where a protocol still equal to the previous template value is updated; a row whose protocol an operator edited no longer matches and keeps its own value.
- The OpenCode terminal's existing per-model `@ai-sdk/openai` npm override keeps a local model that is genuinely Responses-only usable, so this preference adds no new terminal-side rule.
- Manual per-row protocol overrides keep their existing propagation semantics: `local_model` is never touched, a mapping stores no protocol when its effective protocol equals the provider protocol, and an edited value is never overwritten.
- This record partially supersedes the endpoint preference of [API Gateway Provider Templates and Incremental Model Sync](../architecture/2026-09-18-api-gateway-provider-templates.md), [Provider Templates Drop Built-in Model Catalogs and Prices](../architecture/2026-09-19-provider-template-manual-model-sync.md) and [Native Anthropic Messages Entry, Protocol Recognition and OpenAI Entry Conversion](../architecture/2026-10-11-ai-gateway-anthropic-messages.md) only for that order; those records are retained and cross-linked, and all their other decisions remain in force.
