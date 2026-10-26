# Agent Note: Model Pool Candidate Cooldown Is Bounded and Cleared by Exact-Source Success

Status: implemented

English | [中文](2026-10-26-model-pool-cooldown-recovery.zh.md)

## Problem

A model-scoped failure cooled only the affected model-pool candidate, but eligibility followed the upstream's `Retry-After` value exactly. A 429 `FreeUsageLimitError` response could therefore carry a full-day hint and exclude an otherwise healthy candidate for that entire day, even though the gateway should judge eligibility from the requests it observes. The same cooldown had no success path: after another pool or an ordinary mapping proved the exact source could serve again, the candidate stayed excluded in every pool until the deadline expired.

## Decision

- `model_pools::cool_pool_candidate` bounds every candidate cooldown at `POOL_CANDIDATE_COOLDOWN_SECS = 60`. An absent or invalid delay falls back to 60 seconds; a valid `retry-after-ms` or `Retry-After` zero, subsecond or shorter duration is preserved exactly; a longer delay is capped at 60 seconds. The deadline remains a process-memory monotonic instant and is shared across ingresses for the same candidate.
- `attempt_policy::settle_success` releases the cooldowns of the exact source when a normally completed successful attempt matches the original provider id, the trimmed local/upstream model pair and the effective upstream protocol. The clear matches by source identity rather than by pool, so regular, key-probe and fallback references to that source recover together.
- Only a normally completed successful attempt clears the cooldown. Ordinary direct mappings, pool regular candidates and pool fallbacks all settle through the same success path; a failed, cancelled or aborted stream does not clear, a sibling model or a different effective protocol stays cooling, and a provider-wide action never clears unrelated identities.
- A half-open probe success keeps the existing probe settlement and does not change the candidate cooldown policy. Candidate cooldown stays separate from key/account health and mapping auto-disable: clearing it never marks or exhausts a shared key, never registers mapping health, and the existing credential, account-exhaustion and probe rules remain unchanged.
- Eligibility is request-driven: the gateway can clear a cooldown only from a success it observes. Requests that an external client such as OpenCode sends directly to its own provider connection, outside the gateway, are invisible and cannot shorten a cooldown.

## Alternatives considered

- Honor every upstream retry hint exactly: declined because a stale or model-scoped full-day hint would exclude a candidate long after the observed request, while the gateway cannot verify that the hint is still current.
- Keep the cooldown until its deadline and rely on expiry only: declined because a source already proven successful by another pool or an ordinary route would remain excluded in every pool.
- Clear cooldowns on any attempt, sibling model or provider-wide action: declined because it would broaden recovery beyond the exact source and could mask real failures.
- Persist the cooldown or probe upstream to learn health: declined because the cooldown stays in process memory and the existing per-request probe policy and persisted key/mapping marks remain independent.

## Consequences

- A model-scoped failure excludes a candidate for at most 60 seconds; zero, subsecond and shorter hints still apply exactly, and a full-day hint no longer removes a healthy source for a day.
- A success from the exact source through any gateway route makes it immediately eligible in every pool, while sibling models and other protocols keep their own cooldowns.
- The gateway cannot observe direct external client requests, so a cooldown can end only through a gateway-observed success or its 60-second expiry; this visibility limit is not a promise of external health.
- Verification: the scoped behavior tests in `src-tauri/src/ai_gateway/tests/model_pools.rs` cover the 60-second cap with 59/60-second eligibility and a real routed retry, clearing across pools for both streaming and non-streaming success, unchanged sibling and protocol cooldowns, and the failed-stream non-clear case. The four focused cooldown tests, the full model-pool suite, the pool terminal-sync suite, and both OpenAI and Anthropic conversion suites passed with no failures.
- This record refines only the candidate-cooldown bullet of [Model Pools Route One Public Alias Across Explicit Source Candidates](../feature/2026-10-13-free-model-pool.md); every other decision in that record remains in force.
