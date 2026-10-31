# Agent Note: Reflection/Beam Pool Chat Candidates Repair Missing Historical Reasoning

Status: implemented

English | [中文](2026-10-31-reflection-beam-historical-reasoning-repair.zh.md)

## Problem

Reflection/Beam's Chat Completions API requires a non-empty `reasoning_content` on every earlier assistant message and returns an HTTP 400 when any historical assistant turn carries a missing, `null`, empty or whitespace-only value. A client that replays a degraded turn with no recorded thinking therefore cannot continue the conversation, and a model pool whose selected Reflection/Beam candidates all fail this check spends an upstream attempt per candidate before falling back to unrelated sources. The previous gateway behavior only classified the rejection as a skip after the failure; it preserved pool continuity but never made the affected candidate serviceable.

## Decision

- A new module `src-tauri/src/ai_gateway/reasoning_history.rs` owns the bounded repair. `model_requires_historical_reasoning` recognizes an upstream model in the `reflection/` namespace after trimming and ASCII lowercasing, and `repair_historical_reasoning` walks the assistant messages of a Chat Completions JSON body.
- `forwarding.rs` applies the repair only to a prepared model-pool candidate (`provider.pool_candidate`) whose effective upstream protocol is Chat Completions and whose upstream model is in that namespace. The repair runs after protocol conversion and model rewrite and before pool-native reasoning injection.
- For each assistant message whose `reasoning_content` is absent, `null`, an empty string or whitespace-only, the repair writes the non-empty placeholder `[reasoning unavailable]`. Existing non-empty values and every other message field are preserved. Non-object bodies, bodies without a `messages` array and bodies that need no repair are returned with their original bytes.
- Ordinary routes, every other pool candidate and every other upstream protocol are untouched; in particular the OpenCode Zen Free candidates `beam-free`, `glm-5.3-flash-free` and `deepseek-v4.1-flash-free` still forward the caller history unchanged.
- The existing missing-history rejection classification remains as a fallback for a case where the repair does not apply or the upstream still rejects the request: the prepared pool candidate is skipped for the current request without retrying, cooling, key marking or mapping-health changes.

## Alternatives considered

- Keep the skip-only behavior and let the pool move to the next candidate: declined because every Reflection/Beam candidate still receives a request the upstream is known to reject, and a pool made only of such candidates always falls back to unrelated sources.
- Synthesize real reasoning text for the missing turns: declined because that would invent provider output and misrepresent what the model actually produced.
- Apply the repair to every pool candidate or every ordinary route: declined because only Reflection/Beam declares this history requirement, and rewriting other bodies would alter bytes and risk unrelated upstream contracts.
- Drop the affected assistant messages or convert them to a different role: declined because that would change the conversation history and the caller's observable context.
- Use an empty or whitespace-only placeholder: declined because Reflection/Beam treats a blank value as missing, so only a non-empty marker satisfies the check.

## Consequences

- A prepared Reflection/Beam Chat pool candidate now succeeds on its first attempt when earlier assistant turns lack recorded reasoning, while non-empty caller reasoning is preserved exactly and a body with no missing value stays byte-identical to its input.
- The repair is deliberately narrow: it does not fabricate reasoning content, does not touch ordinary routes, does not touch any other pool candidate and does not change any other protocol. The provider's own degraded missing-thinking mode renders an empty analysis block for the placeholder.
- The placeholder is visible to the upstream and to later history replay; it is a bounded marker rather than a reasoning-continuity guarantee, and the gateway still makes no general cross-model reasoning-continuity promise.
- Verification: the two mock behavior tests in `src-tauri/src/ai_gateway/tests/model_pools.rs` pass; the `ai_gateway::tests::model_pools::` suite reports 94 passed and `ai_gateway::tests::openai_conversion::` reports 44 passed. A temporary out-of-band check with a copied production profile, since deleted and with no production key modified, returned HTTP 200 for all four `reflection/beam:free` providers and for fallback `deepseek-v4.1-flash` and `glm-5.3-flash`; a full free-model pool sequence returned 5/5 HTTP 200 without attempting Zen. The three Zen free candidates were separately at their upstream free-tier quota limit (HTTP 429 / quota-exhausted key) during the same window, which is an upstream state rather than a gateway code failure; removing the reflection-failure load also removes the fallback pressure that reached them.
- Partial supersession: [Model Pool Portable Contract Recognizes the Assistant reasoning_content Trace](2026-10-14-model-pool-reasoning-content.md) is retained and cross-linked. This record narrows that record's previously absolute no-rewrite/no-fabrication statement for a missing-history rejection: the placeholder repair above is the single bounded exception, and that record's exact-JSON preservation for ordinary same-protocol Chat traffic and its omission without fabrication on the Responses and native Messages conversions still stand.
