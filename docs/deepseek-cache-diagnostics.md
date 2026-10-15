# DeepSeek cache accounting and diagnostics

This report tracks the `20261015-deepseek-cache-diagnostics` plan. It is maintained incrementally: it records delivered facts at each accepted boundary and states the remaining work and any blocked evidence explicitly.

Delivered: Step 1 native DeepSeek cache accounting and the existing Chat forwarding characterization; Step 2 the standalone, content-free observation and bounded comparison tool with its local behavior tests; the aggregate repair from the simultaneous Spec Review and Standards Review batch, which hardened the observer's transport handling, split-SSE normalization and overlap tracking without changing its CLI contract or output allowlist; and, in this follow-up, a bounded per-request timeout for `compare` with its regression test. The plan's engineering delivery (Steps 1-2, the aggregate repair and this documentation) is merged into `main` at commit `5d452bdf7fab0ba85e126b17b23188b059695594` and its implementation record is `completed`; the earlier "integration and the merge decision remain pending" sentence was stale and is corrected here. Blocked: Step 3, the controlled same-account live comparison, remains **blocked / unverified** after two operator-authorized live attempts, and its effectiveness measurement is the only unverified item. The run is incomplete: the direct-upstream measurement series is absent, so there is no two-path comparison and no effectiveness verdict, and the 98% token-weighted effectiveness target is still unmeasured and is not claimed as met.

## Scope

The plan separates an accounting correction from a measured improvement in upstream cache reuse. It delivers truthful native DeepSeek cache accounting, a characterization of the existing Chat forwarding contract, and a temporary single-conversation diagnostic tool with a bounded comparison runner. It does not change routing, forwarding, persistent configuration or usage schemas, and it installs no permanent proxy.

## Delivered: native DeepSeek cache accounting (REQ-001 / AC-001)

`canonical_usage_from_value` in `src-tauri/src/ai_gateway/usage_log.rs` now normalizes the native DeepSeek cache fields into the existing four exclusive tiers, reusing the checked canonical decomposition instead of adding a second path.

- A present `prompt_cache_hit_tokens` / `prompt_cache_miss_tokens` must be a non-negative integer or the already supported integer-string form; `null` and an absent key mean absent; an explicit `0` is present.
- When nested cache details are absent, a present native hit is the cache-read tier.
- A native hit and a nested `cached_tokens` that are both present must be equal and are counted once.
- A present native miss must equal the reported inclusive input minus the selected cache hit, so a miss-only response never invents a cached count.
- A negative, fractional or malformed native value, a native/nested hit mismatch, a miss inconsistent with `input - hit`, and a native or nested hit that coexists with the independent Anthropic `cache_read_input_tokens` tier all produce the existing conservative fallback: the reported input is billed once as ordinary with no cache tiers and the request stays cache-statistics-ineligible. No failing shape is repaired by saturating subtraction.
- `input_tokens` still wins over `prompt_tokens` for the inclusive total and the output-count validation stays independent of cache validity.
- The result is identical for a buffered JSON response and a fragmented SSE stream; missing usage remains `usage_present = false` rather than a fabricated `0%`.
- No schema changed, no stored row, historical token, amount or price snapshot was rewritten, and nothing was backfilled. No public symbol was added; the native helpers stay private to `usage_log.rs`.

Synthetic native example: an inclusive input of `100` with a hit of `98` and a miss of `2` is stored as `2` ordinary input tokens and `98` cache-read tokens, never as `100`/`0`.

## Delivered: Chat forwarding contract characterization (REQ-002 / AC-002)

The existing forwarding behavior was characterized without changing it. Model-only rewriting, native messages, ordered tool definitions, `reasoning_content` including empty strings and explicit nulls, tool calls and results, a supplied `user_id`/`metadata` and the recognized session headers survive gateway mapping; downstream bytes and cancellation behavior are preserved; and upstream authentication retains its existing treatment. `src-tauri/src/ai_gateway/forwarding.rs` and `src-tauri/src/ai_gateway/selection.rs` needed no fix. A newly discovered defect would have required a bounded follow-up rather than an unapproved edit.

## Delivered: content-free observation (REQ-003 / AC-003)

`tools/diagnose-deepseek-cache.mjs` is a standalone on-demand Node tool that uses built-in modules only. Its `observe` entry binds to loopback and transparently forwards to an explicitly selected loopback gateway:

```
node tools/diagnose-deepseek-cache.mjs observe \
  --listen 17690 --gateway http://127.0.0.1:17688 --model deepseek-v4.1-flash
```

- `--gateway` must be a loopback host; the tool rejects a non-loopback target. `--listen` binds `127.0.0.1` at the given port.
- It is an opt-in temporary path only: it is not installed into the normal gateway runtime and never edits the installed client. The operator points one chosen client connection at the observer for the duration of the observation and restores the original connection choice afterwards.
- It observes the native `/v1/chat/completions` and `/chat/completions` POST routes only. Every other request (and every non-POST request) is an unobserved pass-through.
- It selects the first non-empty recognized session identity for the requested model using the existing header precedence (`x-session-affinity`, `x-opencode-session`, `session-id`, `session_id`, `conversation_id`, `thread-id`, `x-session-id`) and observes only that conversation; a missing identity or another conversation passes through without an inferred identity.
- It retains exactly one preceding completed selected request. Overlapping selected requests are marked comparison-unavailable; it never serializes them or invents a predecessor. It compares message-prefix values, ordered tools, `metadata` and `user_id`, and reports whether the input is append-only or the first changed JSON path.
- It emits one JSONL `observation` event per completed selected request: `comparison` is `append-only`, `changed` or `unavailable`, `first_changed_path` is present only when changed, `http_status` carries the upstream status, and `usage` carries the numeric result.
- `usage.availability` is `available` or `unavailable`; `usage.reason` distinguishes `missing-usage`, `malformed-usage`, `native-nested-disagree` and `unsupported-format`. `usage.fields.native` / `usage.fields.nested` record which source fields were present. When available, `usage` carries `inclusive_input_tokens`, `cache_hit_tokens`, `cache_miss_tokens`, `output_tokens` and `cache_hit_rate` (`null` when the inclusive input is 0).
- Output is a metadata allowlist. Bodies, message/tool values, header values, keys, session identities, URLs, error text and content hashes are never printed or persisted, and no request/response sample is saved.

JSON/character comparison reports which fields changed; it never claims to measure the tokenizer's actual cache-unit boundary, and the reported counts are the upstream's own numbers, not a character-derived token estimate.

### Observer robustness from the Spec/Standards aggregate repair

The review's aggregate repair hardened the observer without changing its CLI contract or output allowlist:

- `--gateway` accepts either an `http:` or an `https:` URL, and the upstream transport follows the configured protocol. A synchronous transport-construction failure or an asynchronous TLS handshake failure now becomes a bounded `HTTP 502` response, and the observer process and its listener stay alive, so the client is answered instead of being dropped or crashing the process. The output allowlist and the CLI contract are unchanged.
- SSE parsing normalizes `\r\n` and `\r` to `\n` on the accumulated buffer and holds a dangling trailing `\r`, so an event boundary split across chunks (for example `...\r\n\r` then `\n...`) still collapses to a real blank line and a split event's `usage` is still parsed. `SSE_EVENT_LIMIT` is unchanged and forwarded bytes are never affected.
- Overlap is tracked from the moment a same-session chat request starts rather than when its body ends. The comparison verdict is deferred until every concurrent peer's selected or unselected state is known, while forwarding, piping and backpressure are never delayed. Overlap is symmetric: both concurrently selected requests are reported `unavailable` with reason `overlap`, neither is retained as a predecessor, and the next sequential request reports `no-predecessor`. Different sessions, models or routes never contribute a peer. Retained body samples stay bounded to at most one previous request plus the in-flight selected requests; non-selected, aborted and overlapped captures are released.

Natural-traffic observation through `observe` was **not selected and not run** for this plan, so no live observation evidence exists; the tool is delivered and its local behavior is verified (see Evidence).

## Delivered: bounded comparison runner (REQ-004 / AC-004)

`compare` takes no arguments and reads gateway/upstream coordinates and credentials only from six process environment variables:

| Variable | Purpose |
| --- | --- |
| `CACHE_DIAG_GATEWAY_BASE_URL` | Gateway base URL |
| `CACHE_DIAG_GATEWAY_MODEL` | Gateway model id |
| `CACHE_DIAG_GATEWAY_KEY` | Gateway credential |
| `CACHE_DIAG_UPSTREAM_BASE_URL` | Direct upstream base URL |
| `CACHE_DIAG_UPSTREAM_MODEL` | Direct upstream model id |
| `CACHE_DIAG_UPSTREAM_KEY` | Direct upstream credential |

All six are required. Credentials must never enter command arguments, the request body echoed to the report or the output; set them in the process environment with values that are not shown, for example through an un-echoed interactive read (placeholders shown here, never real values):

```
read -s CACHE_DIAG_GATEWAY_KEY; export CACHE_DIAG_GATEWAY_KEY   # repeat for each variable
node tools/diagnose-deepseek-cache.mjs compare
```

The command builds each chat-completions endpoint from the configured base URL without duplicating a version segment: a base ending in `/v1` gets `/chat/completions`, and any other base gets `/v1/chat/completions`. Pass the base URL (for example `http://127.0.0.1:17688/v1`) and let the tool append the route; do not compose a double-`/v1` path yourself. The tool performs no configuration discovery or mutation and reads no encrypted or client configuration.

The fixture is fixed and public: a 256 KiB ASCII prefix with ten deterministic append-only request bodies per series, each carrying `thinking: {"type":"disabled"}` and `max_tokens: 32`. Each path runs two rounds; each completed series is ten requests with the first two as unreported warmup and the last eight measured. The run is capped at 40 model POST attempts with no retries. A rejected request, a transport failure or missing/invalid measured usage stops that series immediately with its true attempt count.

Every `compare` POST attempt is bounded by a per-request timeout whose default is `600000` ms; the process environment variable `CACHE_DIAG_COMPARE_REQUEST_TIMEOUT_MS` optionally overrides it with a positive integer number of milliseconds, and an absent, invalid, zero or negative value falls back to the default without failing the run. On expiry the attempt resolves through the existing transport outcome exactly like any other transport failure and the socket is destroyed; the attempt still counts in its phase coverage, the series stops, the run reports `status: "incomplete"` with `reason: "transport-failure"`, later rounds and paths are not attempted, and no retry is made. This bound is `compare`-only: `observe`, its streaming and forwarding, the six `CACHE_DIAG_*` coordinate and credential variables, the fixtures, the 40-attempt cap, the output fields and the metadata allowlist are unchanged.

`compare` prints one `comparison` JSON line:

- `attempts` — total attempts actually made; `status` is `completed` or `incomplete`; `started_at` / `completed_at` are ISO timestamps.
- `comparable` is always `false` and `effectiveness` is always `unverified`: the tool never self-certifies account equivalence or model identity.
- `target_met` is `true` / `false` only for a completed run and `null` otherwise; `cost` is `{ "availability": "unknown" }` (the tool computes no cost).
- `series[]` entries carry `path`, `round`, `attempts` and the per-phase `warmup` and `measurement` coverage.

Each coverage object reports `requests` (the actual attempts made in that phase), `valid_usage`, the token sums `inclusive_input_tokens` and `cache_hit_tokens`, `cache_hit_rate` (`null` when `inclusive_input_tokens` is 0), and the per-source presence counts `fields.native` / `fields.nested`. The measured rate is a fraction of summed cache-hit input over summed inclusive input; the effectiveness target is `.98`.

`status: completed` means only that every planned series completed with valid measured usage. It does **not** mean the two paths serve the same underlying account, that the models are equivalent, or that effectiveness reached the target.

When any required variable is absent or unusable, `compare` still prints a `comparison` object with `attempts: 0`, `status: "incomplete"` and `reason: "missing-config"`, and exits nonzero.

## Evidence

| Check | Result |
| --- | --- |
| `deepseek_cache` RED (Step 1 baseline) | 18 selected tests: 14 fail on the old numeric result, 4 existing-behavior tests already pass |
| `deepseek_cache` GREEN (Step 1 baseline) | exit 0, 18 pass |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::tests::deepseek_cache -- --test-threads=1` (repair round) | exit 0, 20 pass, 0 fail, 0 ignored; adds a fragmented nested-only and a fragmented consistent-dual stream characterization |
| `canonical_` | exit 0, 16 selected: 15 pass, 1 pre-existing ignored |
| `cachehit_` | exit 0, 9 pass |
| `session_affinity_` | exit 0, 24 pass |
| `streaming_forward_preserves_bytes_captures_usage_and_fails_all_unavailable` | exit 0, 1 pass |
| `forwarded_logs_never_contain_keys_headers_or_bodies` | exit 0, 1 pass |
| `node --test tools/diagnose-deepseek-cache.test.mjs` (pre-repair baseline) | exit 0, 38 pass, 0 skipped, ~7.18s |
| `node --test tools/diagnose-deepseek-cache.test.mjs` (repair round) | exit 0, 41 pass, 0 fail, 0 skip (3 new RED cases turned GREEN: https construction/handshake `502`, split CRLF event boundary, transport-start overlap) |
| `node tools/diagnose-deepseek-cache.mjs compare` (single authorized batch, operator-supplied environment) | exit 1 (nonzero); 1/40 attempts; `status: incomplete`, `reason: request-rejected`; gateway path 1 attempt with 0 valid usage, upstream path 0 requests, no rate |
| read-only `onespace-dev` usage query (run window, target provider/upstream) | 1 row: `status 429`, `result failure`, `usage_present 0`, `cache_accounting_valid 0`, all token tiers `0` |
| post-run dev comparison reload | failed with a fixed missing-source-key reason; credential-unchanged re-check not completed |
| `node --test tools/diagnose-deepseek-cache.test.mjs` (this follow-up, after the `compare` request-timeout fix) | exit 0, 42 pass, 0 fail, 0 skip, ~7.85s |
| focused `compare bounds a stalled response with CACHE_DIAG_COMPARE_REQUEST_TIMEOUT_MS and stops as a transport failure` | RED on the pre-fix tool (unbounded hang); GREEN after the fix, ~0.47s |
| `node tools/diagnose-deepseek-cache.mjs compare` (second authorized live batch, operator-supplied environment, pre-fix tool) | exit 1 (nonzero); 23/40 attempts; `status: incomplete`, `reason: transport-failure`; gateway 16/16 measured coverage, token-weighted ≈93.684%; upstream path 1 attempt with 0 valid usage; no rate |
| read-only `onespace-dev` usage query (second run window, target provider/upstream) | 20 rows, all `result success` / `status 200` / `terminal 1` / `usage_present 1` / `cache_accounting_valid 1`; sums input 94364, cache_read 843136, cache_write 0, output 640, total 938140, amount 0.017068; 0 rows in the preceding 6 h baseline |

The behavior tests for Step 1 live in `src-tauri/src/ai_gateway/tests/deepseek_cache.rs`, registered as `mod deepseek_cache;` in `src-tauri/src/ai_gateway/tests.rs`. The Step 2 behavior tests live in `tools/diagnose-deepseek-cache.test.mjs`; they use local mocks and make no external provider calls, covering unchanged bytes/headers, split SSE and JSON usage, append/change/unavailable classification, identities, overlap, cancellation, exit cleanup, echoed-secret suppression, weighting, zero-versus-missing usage, warmup exclusion and the 40-attempt cap.

Bounded numeric finding (Step 1): for the synthetic native fixture described above, the buffered and fragmented SSE paths both yield ordinary input `2`, cache read `98`, an output count of `7` and an independent charge of approximately `79e-6` (the earlier input-tier figure was approximately `128e-6`), and the per-request total and the aggregate do not double count the cached tokens.

## Controlled same-account live comparison: two authorized attempts, still unverified (REQ-004 / AC-004)

Step 3 has now run two operator-authorized live attempts. The first produced no valid measurement (a single `429` rejection); the second, run in this follow-up on the merged tool, produced a gateway measurement but no direct-upstream measurement, because the direct path stalled and the run ended as a transport failure. AC-004 therefore remains **blocked / unverified**: the run is incomplete, so there is no two-path comparison and no effectiveness verdict. Never describe Step 3 as passed.

### Second authorized attempt (this follow-up)

Preparation was operator-authorized and plan-scoped, and the tool was unchanged: `compare` still reads only its process environment and performs no configuration discovery or mutation. A temporary plan helper (a plan artifact, not a product feature) read only the `onespace-dev` profile's `.local_key` and `ai_gateway.json`, injected the six `CACHE_DIAG_*` values into the child in memory, spawned the merged tool exactly once and restored the state afterwards, in the order `inspect` → `isolate` → runner → `restore` → `inspect`.

`isolate` temporarily disabled exactly the non-target providers (`enabled` `3` → `1`; the target `gw-7b95d9e4223a42deb9ad59b6a4e54412` stayed enabled, exactly one usable source key), and `restore` returned the count to `3` after the batch; the post-`restore` `inspect` reported `state_present: false`, three enabled providers and `same_source_key_assigned: true`. The runner's identical pre/post verification objects confirmed `profile: onespace-dev`, `expected_provider_id: gw-7b95d9e4223a42deb9ad59b6a4e54412`, `expected_local_model: deepseek-v4.1-flash`, `expected_upstream_model: deepseek/deepseek-v4.1-flash`, `global_enabled_count: 1`, `same_source_key_assigned: true` and `same_source_credential_unchanged: true`, so the gateway path and the direct path used the same single configured source key/account. No service was restarted, only the dev profile was touched, and the batch made no config write other than the temporary `enabled` isolation/restore. The gateway under test was a dev-profile app instance on loopback `127.0.0.1:17689`; the running build's `src-tauri/src/ai_gateway` sources were byte-identical to the project working tree (empty directory diff), so the forwarding implementation under test matched the merged code.

The single `node tools/diagnose-deepseek-cache.mjs compare` invocation (the merged pre-fix tool, run exactly once) ran `2026-10-15T12:16:02.608Z`–`2026-10-15T13:27:14.150Z` (`20:16:02.608`–`21:27:14.150` UTC+08:00) and exited `1` with:

- `attempts: 23`, `status: "incomplete"`, `reason: "transport-failure"`, `comparable: false`, `effectiveness: "unverified"`, `target_met: null`, `cost.availability: "unknown"`.
- one `series` entry `gateway` round 1 with `attempts: 10`: `warmup` 2/2 valid usage, inclusive `93702`, hit `46720`, rate `0.4986019508655098`, `fields.native` 0 / `fields.nested` 2; `measurement` 8/8, inclusive `375048`, hit `374784`, rate `0.9992960901004672`, `fields.native` 0 / `fields.nested` 8.
- one `series` entry `gateway` round 2 with `attempts: 10`: `warmup` 2/2, inclusive `93702`, hit `93696`, rate `0.9999359672152142`, `fields.native` 0 / `fields.nested` 2; `measurement` 8/8, inclusive `375048`, hit `327936`, rate `0.8743840788379087`, `fields.native` 0 / `fields.nested` 8.
- one `series` entry `upstream` round 1 with `attempts: 3`: `warmup` 2/2, inclusive `93702`, hit `93696`, rate `0.9999359672152142`, `fields.native` 0 / `fields.nested` 2; `measurement` `requests: 1` / `valid_usage: 0`, every token sum `0`, rate `null`, `fields.native` / `fields.nested` both `0`. There was no upstream round 2 and no other series.

Derived across the 16 gateway measurement requests of both rounds, the token-weighted gateway rate is (374784 + 327936) / (375048 + 375048) = 702720 / 750096 ≈ 0.93684, i.e. **≈93.684%, below the 98% target as measured**; round 1 measured 99.9296% and round 2 measured 87.4384%. The round-2 drop is one full-miss measured request (0 cache-read, 46890 ordinary input); the other seven round-2 measured requests hit 46848 tokens each with 12–54 ordinary tokens, and round 1's first warmup was the single deliberate cold request (46848 ordinary, 0 hit). The gateway-path rate is stated numerically but is not converted into a completed below-target verdict or an effectiveness conclusion, because the run itself is incomplete.

The direct-upstream path stalled. Its first measured request was accepted — the body was sent on one TLS connection to the upstream edge — but produced no response; repeated read-only operator socket observations during roughly `20:47`–`21:25` UTC+08:00 showed zero response bytes and empty send/receive queues on that single connection. The upstream closed it and the tool recorded the transport failure at `21:27:14.150`. A later operator termination attempt at `21:48:28` targeted an already-exited process and had no effect, so the run ended by the upstream transport failure rather than by operator termination; the elapsed stall on that connection was roughly one hour.

The read-only `onespace-dev` usage check for the batch window (`2026-10-15T12:16:02.608Z`–`13:27:14.150Z` plus a 15 s margin), filtered to `provider_id = 'gw-7b95d9e4223a42deb9ad59b6a4e54412' AND local_model = 'deepseek-v4.1-flash' AND upstream_model = 'deepseek/deepseek-v4.1-flash'`, found exactly **20** rows (4 warmups + 16 measurements, no hidden retries): all `result = success`, `status = 200`, `terminal = 1`, `usage_semantics = canonical_v1`, `usage_present = 1`, `cache_accounting_valid = 1`; sums were `input_tokens` `94364`, `cache_read_tokens` `843136`, `cache_write_tokens` `0`, `output_tokens` `640`, `total_tokens` `938140` and `amount` `0.017068`; `duration_ms` ranged `2313`–`8929` and `first_token_ms` / `avg_tps` were `null` on all 20. The rows span `12:16:07.130Z`–`12:17:35.820Z`, so the whole gateway series completed in ≈89.7 s. Cross-check: 94364 + 843136 = 937500, the tool's total gateway inclusive input, and 938140 is that plus 640 output. The provider had 0 rows in the preceding 6 h baseline, so no other traffic polluted the window.

Per the decision table this is "Missing account evidence, rejected options, failed calls or absent/invalid measured usage" — the upstream measurement is absent due to the transport failure — so it is **not comparable** and remains unverified/blocked. The direct-upstream warmup pair hit 93696/93702 ≈ 99.994%, which shows the account/upstream could serve this synthetic prefix with near-total cache hits at that moment; that is a reported fact, not a verdict. No gateway-defect claim is possible or made: 15 of the 16 measured gateway requests hit their full prefix, and the one full miss is unexplained.

No retry, second batch, extra warmup or live `observe` ran in this follow-up, and the batch ran on the pre-fix tool. This follow-up's single authorized batch is spent and further measurement needs a new explicit authorization; the fixture must not be changed. The `compare` per-request timeout described above was authored after the batch and is delivered in the same change with its regression test.

### First authorized attempt (historical)

The first batch ran `2026-10-15T09:15:18.078Z`–`09:15:21.111Z` (`17:15:18.078`–`17:15:21.111` UTC+08:00) and exited nonzero with `attempts: 1` of 40, `status: incomplete`, `reason: request-rejected`: the gateway path's first request was rejected with HTTP 429, no warmup or measurement produced valid usage, and the direct upstream path sent no request. The read-only `onespace-dev` usage check for that window and the target identity found exactly **one** row — the rejected attempt (`result: failure`, `status: 429`, `usage_present: 0`, `cache_accounting_valid: 0`, every token tier `0`, `amount 0.0`) — so the stored fee is not a measured comparison cost. A post-run reload of the comparison state failed with a fixed missing-source-key reason, so the credential-unchanged re-check did not complete and no credential-value change is claimed; the gateway had independently set its own runtime quota flag on the target key from the `429`, which is outside this plan's authorization to clear.

### Read-only usage verification template

Read-only `sqlite3 -readonly` queries may inspect only the selected application profile's database and run window, filtered to the comparison's provider/model. Do not read whole databases, encrypted configuration, client configuration or real conversation bodies. The usage table is `usage_logs` with (among others) the columns `timestamp_ms`, `local_model`, `upstream_model`, `provider_id`, `provider_name`, `result`, `input_tokens`, `cache_read_tokens`, `cache_write_tokens`, `total_tokens`, `terminal`, `usage_semantics`, `usage_present`, `cache_accounting_valid`:

```
DB="$HOME/.config/onespace/ai_gateway_usage.db"   # or ~/.config/onespace-dev/ai_gateway_usage.db
sqlite3 -readonly "$DB" "
  SELECT provider_id, provider_name, upstream_model, usage_semantics,
         usage_present, cache_accounting_valid,
         COUNT(*) AS terminal_rows,
         SUM(input_tokens) AS ordinary,
         SUM(cache_read_tokens) AS cache_read,
         SUM(cache_write_tokens) AS cache_write
  FROM usage_logs
  WHERE terminal = 1 AND result = 'success'
    AND timestamp_ms >= :window_start AND timestamp_ms < :window_end
    AND provider_id = :provider_id AND upstream_model = :upstream_model
  GROUP BY provider_id, provider_name, upstream_model,
           usage_semantics, usage_present, cache_accounting_valid;"
```

Substitute `:window_start`, `:window_end`, `:provider_id` and `:upstream_model` for the verified run; do not broaden the query beyond the selected profile and window.

## Effectiveness target

The 98% token-weighted cache-hit target for warmed requests with a stable long prefix is **not established and not claimed as met**. The second authorized comparison produced a gateway-path measurement of **≈93.684%** (16/16 measurement coverage, token-weighted) — **below the 98% target as measured** — but the run was incomplete because the direct-upstream series stalled, so there is no two-path comparison and no effectiveness verdict. A gateway-path rate below target on an incomplete run is not a completed below-target verdict, and it does not establish that the original production cache problem is resolved. The target cannot be established from the accounting or local mock tests alone, and a corrected accounting percentage is not evidence of improved physical cache reuse.

- Engineering: the Step 1 accounting correction, the Step 2 tooling and local behavior tests, the aggregate repair, and this follow-up's `compare` request-timeout fix are delivered; the engineering delivery is merged into `main` at `5d452bdf7fab0ba85e126b17b23188b059695594`.
- Effectiveness: unverified. Two authorized comparison batches produced no complete two-path measurement; the gateway-path rate was ≈93.684% across the second batch's 16 measured requests, below the 98% target as measured, and the direct-upstream series is absent.

## Interpretation limits

- The comparison fixture is synthetic and public; it is not a production traffic sample and does not establish that the original production issue is fixed. A synthetic pass alone is never reported as resolving the cache problem.
- Both paths in a single comparison may share upstream warming, so an inference about the gateway must be bounded by the paired measurement and the account-identity evidence, not by one path's absolute hit rate.
- The historical sample of the 2026-10-15 00:00–11:29 UTC+08:00 window (3,139 eligible requests matched to 111 OpenCode conversations, token-weighted 93.1207%) is an existing observation recorded before this investigation's agents. It is **not** a before/after metric for this change and is not a permanent benchmark fixture.

## Interpretation decision table

| Observation after a completed comparison | Conclusion | Required follow-up |
| --- | --- | --- |
| An unchanged prefix with low direct-upstream hits | Upstream evidence | Bounded provider-side follow-up |
| A client-side rewrite of the previous fields | Client evidence, needing empirical confirmation | Bounded client follow-up |
| A newly evidenced gateway mutation outside the delivered scope | Gateway mutation | Scoped repair decision before any edit |
| Fresh-input growth with a growing prefix | Reported separately from cache reuse | Not a cache verdict |
| Missing account evidence, rejected options, failed calls or absent/invalid measured usage | Not comparable | Remains unverified or blocked; never a passing comparison |
| A first request rejected before any valid usage (the observed `429`) | Unmeasured, not a miss and not below target | Needs a new authorization and a usable account before any re-measurement |

## References

- [DeepSeek context caching](https://api-docs.deepseek.com/zh-cn/guides/kv_cache)
- [DeepSeek Chat API](https://api-docs.deepseek.com/api/create-chat-completion)
