# Agent Note: AI Gateway Hot Reads Share Immutable Configuration and Reuse Parsed State

Status: implemented

English | [中文](2026-10-19-ai-gateway-performance.zh.md)

## Problem

A warm gateway read avoided decryption through the file-identity cache, but each read still returned an owned `GatewayConfig` deep clone. Request handling, listener status/start/stop, model discovery, autostart and the per-attempt key-mark reread all paid that copy, so local work grew with the complete stored provider, mapping, key, price and pool set even though those consumers only read. Model-pool planning had a second multiplier: it cloned a complete provider to build a one-mapping routing view for every stored candidate, including disabled providers and disabled mappings that could never serve or be probed. Same-turn continuation parsing copied the caller history array into a `Value`, and recording a delivered hold cloned that history again, so a large conversation was copied twice before its existing prefix, tool-identity and byte-budget checks ran.

## Decision

`storage` keeps the process read cache keyed by path, byte length and modification time but stores an `Arc<GatewayConfig>`. `read_config_shared()` returns the shared immutable value for read-only hot consumers: the request boundary, listener start/stop/status, autostart and the per-attempt key-mark refresh. `read_config()` remains the owned API for callers that need ownership and clones only when it must hand out an owned value; `modify_config()` takes an owned copy under the existing write lock so it can mutate, writes atomically, and publishes the normalized result as a new cached `Arc` only after a successful write. A direct on-disk rewrite that changes file identity is still observed by the next read; no TTL, result cache, content hash or serialization field is added. The only copy counters are test-only `cfg(test)` seams.

Model-pool planning now builds the one-mapping routing clone only for a candidate whose provider and mapping are enabled. An auto-disabled mapping is still prepared because it can be the eligible half-open probe; disabled providers and disabled mappings stay borrowed and are never prepared. The healthy candidate list holds references to the resolved candidates, and weighted ordering clones only the prepared providers it actually returns, plus the single probe it appends. Candidate set, eligibility, weights, independent SWRR accounting, fallback order, reasoning filtering, cooldown and probe placement are unchanged.

`CallerHistory` stores its history as `Arc<Value>` and a `HoldEntry` shares that value instead of cloning it. Parsing allocates the array once inside the `Arc`; recording a delivered hold increments the reference count. Prefix comparison, instruction/tool-context equality, tool-call identity, admission-token ordering, entry count, per-entry and total byte budgets, expiry and eviction are unchanged.

## Alternatives considered

- Add a TTL or result cache, or keep a separate configuration snapshot per route: declined because an immutable `Arc` handed from the existing identity-checked cache preserves freshness without a staleness window or another invalidation surface.
- Hash configuration bytes to validate the cache: declined because path, byte length and modification time already invalidate every real change, including a direct on-disk rewrite, without hashing secrets or adding cryptographic work.
- Keep preparing a complete provider clone for every stored candidate: declined because disabled providers and disabled mappings cannot be selected or probed, so their copy work must not scale with the stored set.
- Clone the caller history again when recording a delivered hold: declined because the parsed history is immutable; prefix and byte-budget checks can read the same shared value.
- Move shared state into serializable transport types: declined because this sharing is process-internal; public commands, DTOs, serialized fields and schema stay unchanged.

## Consequences

- Warm hot reads share the cached configuration: eight repeated model-discovery reads after warm-up observed zero owned configuration copies, where the previous implementation observed eight.
- The next read observes a direct configuration rewrite with changed file identity, and the next attempt observes a key-state publication made while an earlier attempt was in flight; neither a request-start snapshot nor the shared cache freezes key marks.
- Planning a pool whose stored candidate count grows from two to twelve kept the provider-copy count flat, where the previous implementation copied three and thirteen respectively. Only the candidates that can actually serve or probe are prepared, and the delivered order and probe remain equivalent.
- Parsing and recording one delivered continuation copies the history at most once, where the previous implementation copied it twice; the existing continuation limits and decisions are unchanged.
- Verification: the five new regressions pass, and the focused suites passed 119 tests with `--test-threads=1` (routing 15, pools 25, continuation 43, attempt policy 15, reasoning 21). The routing module's 15 assertions passed, but the outer shell wrapper's trailing status assignment to a read-only shell variable failed after the test command, so no clean wrapper exit is claimed. No end-to-end latency, throughput, cache-hit, cost or resident-memory improvement is claimed; comparative performance remains a separate measurement.
- Supersession: [Gateway Configuration Writes Serialize on One Lock and Reads Cache by File Identity](2026-09-25-gateway-config-write-serialization-and-read-cache.md) is partially superseded. Its serialized write lock, file-identity cache, atomic publication, migration and no-op suppression remain; this record replaces its per-read owned clone with a shared `Arc` handoff. [Core Workflows Cleanup and Optimization](2026-10-09-core-workflows-cleanup-and-optimization.md) is partially superseded only for the configuration reread and pool/history copy facts: its attempt-policy extraction, serialized settlement and no-freeze per-attempt reread remain. Both records are retained and cross-linked; no full consolidation is made.
