# AI Gateway Performance

This report describes the measured performance work on the local AI Gateway and the final comparison against the frozen baseline. It covers request forwarding, database queries, management operations and the resident frontend work. It also records the one failed gate, the attribution investigation and the user-accepted bounded deviation.

## Summary

The final run completed the fixed inventory of 44 request, query and management cases plus 6 frontend cases. Every case ran once cold and five times warm, every HTTP or query case executed at least 200 operations per pass, and the append case executed 10,000 requests with 3 attempts per request per pass.

| Gate | Target | Final result | Status |
| --- | --- | --- | --- |
| Local request processing | Median business time down at least 20% | 85.56% lower (`forwarding_chat_stream_64_c1`, 27.2953125 ms to 3.9410835 ms) | PASS |
| Database querying | Median business time down at least 20% | 69.25% lower (`usage_stats_1k`, 0.5727085 ms to 0.176104 ms) | PASS |
| UI duplicate work | Exact duplicate count down at least 50% | 100% lower (`ui_usage_ticks_visible`, `request_logs_calls` 30 to 0) | PASS |
| Tail latency | Final P95 minus baseline P95 at most `max(10% of baseline P95, 1 ms)` | Every case inside the bound, including `forwarding_chat_sync_32k_c32` at 36.65475 ms versus 39.4042913 ms | PASS |
| Worker peak RSS | Increase at most `max(10% of baseline RSS, 8 MiB)` | Frontend aggregate +10.94% (2,249,867,264 B versus 2,230,794,649.6 B bound) | FAIL, accepted bounded deviation |
| Worker CPU | Recorded separately, no substitute estimate | Rust 927,480 ms to 508,060 ms; frontend 57,645.321 ms to 46,159.194 ms | Recorded |

The final report status is `incomplete` with exit code 1. The only recorded failure is the frontend aggregate peak-RSS gate. The request, query, UI and P95 gates passed. The RSS target is not claimed as met, and the threshold was not lowered.

## Workloads and fixtures

The fixed profile is `ai-gateway-performance-v1`. The inventory enumerates representative supported cases rather than a redundant Cartesian product.

| Family | Fixed workload |
| --- | --- |
| Configuration and discovery | 4 providers x 20 mappings x 2 keys and 100 providers x 100 mappings x 4 keys; status and model discovery |
| Ordinary forwarding | Chat, Responses and Messages, synchronous and streaming, 32 KiB and 256 KiB requests, concurrency 1, 8 and 32 |
| Conversion | Chat to Messages and Responses to Messages, synchronous and streaming, with paired direct-mock controls |
| Streams | 1,000 SSE events with 64-byte and 4,096-byte fragments |
| Pools and continuation | 10 pools x 20 candidates, six ingress/upstream combinations, reasoning, weighted order, cooldown and tool continuation |
| Log append | 10,000 requests x 3 attempts per pass, 30,000 detail rows |
| Log queries and statistics | 1,000 and 100,000 detail rows, 50 providers, 200 models, 365 days, ranges from today through all time, grouping, filters, facets and first/last pages |
| Provider history | 366-day and 3,650-day calendars with zero, unavailable, deleted and ongoing coverage |
| Management and lifecycle | Template views, quota and Go-usage reads, template refresh, terminal sync, daily reset and listener lifecycle |
| Frontend | All tabs in both languages, large lists, drafts, navigation, 30 five-second ticks, event bursts, hidden/visible transitions and a complete 3,650-day calendar |

All fixtures are synthetic and local. Upstreams are local mock servers. No live account, production cache or external service participates in the measured numbers.

## Measured scope

- **Preparation:** fixture construction and schema setup are outside business wall time. The process-cold pass resets process-owned state; it does not claim an operating-system cold disk cache.
- **Build:** compilation is reported separately as `build_ms`.
- **Business wall time:** per-operation samples for the measured operation only.
- **Harness wall time:** the complete child-process and collection time, including preparation.
- **Worker CPU and RSS:** the Rust scope is the aggregate of the 44 test worker processes, including preparation. The frontend scope is the Vitest worker, including preparation. These are the actual measured worker scopes, not an invented hot-path slice.
- **Frontend timing:** the frontend runs in jsdom. Its elapsed timings are proxy measurements only. They are neither native GUI timings nor UI acceptance evidence; the UI gate uses deterministic duplicate-work counters.

Final timings were 12,659.81 ms build, 491,763.653 ms Rust worker wall, 38,615.317 ms frontend wall and 543,353.007 ms harness. Baseline timings were 712.468 ms build, 923,312.384 ms Rust worker wall, 50,561.226 ms frontend wall and 974,983.704 ms harness.

## Environment and source revisions

| Item | Value |
| --- | --- |
| OS | Darwin 27.2.0 |
| Architecture | arm64 |
| CPU | Apple M1 Max, 10 cores |
| Node | v24.18.0 |
| npm | 11.16.0 |
| Rust | rustc 1.93.1 (01f6ddf75 2026-02-11) |
| Cargo | cargo 1.93.1 (083ac5135 2025-12-15) |
| Build profile | test |
| Resource measurement | `/usr/bin/time -l` |
| Baseline source revision | `32a4c2f2f4ad05522cb0036a6cfe482c56969fa7` |
| Final source revision | `0aad087a761240ca0b2d7d2884c0aba49ddd3ebf` |
| Attribution comparison BASE | `855baa46b0cf24d2271fc2d47fa8d37ad782a169` |

Both the baseline and final JSON record the source revision as supplied by the environment. The frozen baseline JSON remains byte-identical to its delivered form. The final JSON is a byte-preserving copy of the representative merged-tree runner stdout (3,046,694 bytes, SHA-256 `023e1c778144c870e99bb028a6bb202d5b7c143a7812fd5ce25b12807911743c`), including its `incomplete` status; the runner exited 1.

## Exact commands

Baseline measurement:

```bash
node tools/measure-ai-gateway.mjs --phase baseline
```

Official final measurement, with an isolated target directory and the final revision supplied to the runner:

```bash
CARGO_TARGET_DIR=.agw-final-target-merged \
AI_GATEWAY_MEASUREMENT_REVISION=0aad087a761240ca0b2d7d2884c0aba49ddd3ebf \
node tools/measure-ai-gateway.mjs --phase final \
  --baseline docs/ai-gateway-performance-baseline.json \
  --timeout-ms 3600000
```

Scoped verification:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway:: -- --test-threads=1
npm test -- src/components/AiGateway src/lib/aiGateway.test.ts src/lib/runtimeStatus.test.ts src/App.runtimeOwnership.test.tsx
npm run build
npx --no-install eslint <exact changed frontend TypeScript and TSX files>
bash tools/check-ai-gateway-redaction.sh
```

The resource measurement scope and the fixed child command arguments are recorded in the baseline and final JSON. The runner invokes no Git command and records no request body, response body, header or credential.

## Representative cases

Baseline and final values are medians over the 1 cold plus 5 warm passes unless noted. Throughput is in operations per second.

| Case | P50 baseline to final | P95 baseline to final | Throughput baseline to final | Recorded counters |
| --- | --- | --- | --- | --- |
| `config_status_models_large` | 4.7472495 to 4.650479 ms | 5.820084 to 5.401625 ms | 319.534 to 378.501 | 100 status reads, 100 model-payload builds, 10,000 models advertised |
| `routing_large` | 3.7124585 to 2.155 ms | 4.784583 to 3.21025 ms | 259.980 to 436.008 | 200 requests, 1 candidate call |
| `forwarding_chat_stream_64_c1` | 27.2953125 to 3.9410835 ms | 30.253458 to 5.16725 ms | 36.934 to 249.254 | 200 requests, 1,000 events, 64-byte fragments |
| `usage_stats_1k` | 0.5727085 to 0.176104 ms | 4.261667 to 3.346584 ms | 858.110 to 1,391.588 | 200 range queries, 11,952 model rows returned |
| `usage_log_append_10000x3` | 0.7853545 to 0.482958 ms | 1.342583 to 0.655209 ms | 1,200.400 to 1,961.099 | 10,000 requests, 3 attempts each, 30,000 rows, 10,000 transactions, 1 database open |
| `provider_history_3650` | 3.6184375 to 3.0656045 ms | 4.734417 to 3.263625 ms | 259.217 to 322.231 | 200 history queries, 730,000 calendar cells |

The UI duplicate-work case `ui_usage_ticks_visible` ran 30 five-second ticks. Its `request_logs_calls` counter moved from 30 to 0, `invoke_calls` moved from 120 to 90, and the case remained a jsdom proxy measurement. The 3,650-day history case rendered all 3,650 dates with 8 unavailable rows in both languages.

## Behavior coverage

The final JSON carries the named behavior inventory under `behavior.inventory`. The measurement runner references those cases but does not execute them; the behavior suites are the execution evidence.

| Family | Representative behavior |
| --- | --- |
| Configuration and discovery | Default-key preference and disable advance; auto-disabled state separated from user intent; protocol-specific model resolution |
| Forwarding and conversion | Chat and Responses entries converted to Anthropic Messages; incremental converted stream with tools and exactly one completion marker |
| Pools and continuation | Non-streaming tool chain prefers the preceding candidate; hold stays eligible at the 30-minute boundary and expires beyond it |
| Stream fragmentation | Fragmented CRLF and multiline events; final unterminated event at finish; converted stream without a trailing blank line |
| HTTP and streams | Mid-stream disconnect publishes no hold and starts no replay; oversized streaming request still forwards with normal routing |
| Usage storage | One request batch stores all attempts and exactly one terminal row; retention keeps boundary rows and daily summaries; provider calendars classify retained, unavailable and covered zero days; every date survives in multi-year history |
| Management and lifecycle | Quota miss, cache, force and read-only command path; Go-usage cache and force; sequential template batch, single-flight skip, daily-reset scanner and listener restart on the same port |
| Terminal sync | Claude template refresh keeps OpenCode and Codex; pool membership changes mark OpenCode pending |
| Frontend drafts and navigation | Runtime provider merge preserves unsaved edits; disappearing source preserves a pool draft and surfaces an error; log return keeps provider history; long history keeps every date |

The continuation limits remain exactly 30 minutes idle validity, 1,024 completed entries, at most 1 MiB per retained payload and 16 MiB total.

## Compatibility and migration

The change keeps configuration schema version 3 and usage database `user_version` 2. Existing tables, columns, stored historical values, command names, DTOs and serialization are unchanged. No index was added; the optional-index rollback path therefore has nothing to retain. Journal mode, durability and busy timeout are unchanged.

The migration suite passed all 30 tests on the identical rerun. Its first invocation reached the 600,000 ms tool timeout during compile-only preparation without producing a test result; the rerun with the same command then completed with 30/30 migration tests passing. That timeout is a tooling boundary, not a migration failure.

## Verification evidence

| Check | Command | Exit | Result |
| --- | --- | --- | --- |
| Final measurement | Fixed runner command above | 1 | `incomplete`, sole failure `rss_regression:frontend` |
| Gateway behavior | `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway:: -- --test-threads=1` | 0 | 1,074 passed, 0 failed, 46 ignored, 646 filtered |
| Frontend behavior | `npm test -- src/components/AiGateway src/lib/aiGateway.test.ts src/lib/runtimeStatus.test.ts src/App.runtimeOwnership.test.tsx` | 0 | 27 files, 770 tests passed |
| Production build | `npm run build` | 0 | 2,669 modules transformed |
| Changed frontend lint | `npx --no-install eslint <18 changed paths>` | 0 | 0 errors, 44 warnings |
| Redaction | `bash tools/check-ai-gateway-redaction.sh` | 1 | Not PASS; same single known finding as baseline |

The redaction scan covered 920 baseline files and 925 final files. Both runs reported zero private keys, zero credential literals and the same single header-value finding, a known BLE authorization type-name false positive. No new scoped finding was introduced. The script's nonzero exit is retained honestly and is not reported as a passing check.

Native macOS GUI smoke and the Windows platform were not executed. No native GUI or Windows performance claim is made. Live upstream latency, cache-hit rate and account charges are also outside the measured scope.

## Rollback

Revert the cohesive code commits for the affected gateway paths and restart the application. Process-owned shared configuration, in-flight quota and Go-usage maps, query handles and prepared statements rebuild on restart. Preserve the encrypted configuration, local keys, request detail rows, provider daily summaries and stored historical amounts. Do not delete the databases or restore an old data snapshot. No optional indexes were added. Re-run the affected configuration, forwarding, accounting and query characterizations after rollback to confirm readability and unchanged behavior.

## Accepted bounded RSS deviation

> Accepted bounded deviation: the representative merged-tree measurement remains `incomplete` with exit 1 because the frontend aggregate peak-RSS gate failed (2,249,867,264 B versus the 2,230,794,649.6 B bound, +10.94% over the 2,027,995,136 B frozen baseline, 19,072,614.4 B over the bound). The user accepted this bounded deviation on the merged controlled attribution evidence: two same-session full-file pairs measured +15.594% and +18.336% against the current BASE (bound +10%; both over), the repeated concurrency case stayed inside its P95 bound in 3/3 runs and below the baseline, and the direct-mock control was not elevated. The official merged2 deviation is smaller than the earlier pre-merge +20.32% that informed the acceptance. No reproducible product regression attributable to the change was established. The RSS target is not met, the threshold was not lowered, and no acceptance PASS is claimed.

The repeated concurrency case was `forwarding_chat_sync_32k_c32`. The official merged2 P95 was 36.65475 ms against the 35.822083 ms baseline and the 39.4042913 ms bound, so it passed. Three controlled merged-tree runs measured P95 at 30.953042 ms, 31.693458 ms and 31.783500 ms, all inside the bound and below the baseline. The direct-mock stream control measured P95 of 2.938500 ms and 2.956250 ms across two runs, against a 27.217666 ms baseline. The control is not elevated.

The earlier pre-merge run (+20.32% RSS) and the first merged run (+39.7% RSS with a c32 outlier) are investigation history only. They are not the official artifact, are not used in the official table, and are not used to claim acceptance.
