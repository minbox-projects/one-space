# DeepSeek cache accounting and diagnostics

This report tracks the `20261015-deepseek-cache-diagnostics` plan. It is maintained incrementally: it records delivered facts at each accepted boundary and states the remaining work and any blocked evidence explicitly.

Delivered: Step 1 native DeepSeek cache accounting and the existing Chat forwarding characterization; Step 2 the standalone, content-free observation and bounded comparison tool with its local behavior tests. Blocked: Step 3, the controlled same-account live comparison, was attempted exactly once under explicit operator authorization and produced no valid measurement, so it remains **blocked / unverified**. Consequently the plan is **not complete**: the later simultaneous Spec Review and Standards Review batch has not been executed, so no merge or "all engineering complete" claim is made here, and the 98% token-weighted effectiveness target is still unmeasured and is not claimed as met.

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
| `deepseek_cache` RED | 18 selected tests: 14 fail on the old numeric result, 4 existing-behavior tests already pass |
| `deepseek_cache` GREEN | exit 0, 18 pass |
| `canonical_` | exit 0, 16 selected: 15 pass, 1 pre-existing ignored |
| `cachehit_` | exit 0, 9 pass |
| `session_affinity_` | exit 0, 24 pass |
| `streaming_forward_preserves_bytes_captures_usage_and_fails_all_unavailable` | exit 0, 1 pass |
| `forwarded_logs_never_contain_keys_headers_or_bodies` | exit 0, 1 pass |
| `node --test tools/diagnose-deepseek-cache.test.mjs` | exit 0, 38 pass, 0 skipped, ~7.18s |
| `node tools/diagnose-deepseek-cache.mjs compare` (single authorized batch, operator-supplied environment) | exit 1 (nonzero); 1/40 attempts; `status: incomplete`, `reason: request-rejected`; gateway path 1 attempt with 0 valid usage, upstream path 0 requests, no rate |
| read-only `onespace-dev` usage query (run window, target provider/upstream) | 1 row: `status 429`, `result failure`, `usage_present 0`, `cache_accounting_valid 0`, all token tiers `0` |
| post-run dev comparison reload | failed with a fixed missing-source-key reason; credential-unchanged re-check not completed |

The behavior tests for Step 1 live in `src-tauri/src/ai_gateway/tests/deepseek_cache.rs`, registered as `mod deepseek_cache;` in `src-tauri/src/ai_gateway/tests.rs`. The Step 2 behavior tests live in `tools/diagnose-deepseek-cache.test.mjs`; they use local mocks and make no external provider calls, covering unchanged bytes/headers, split SSE and JSON usage, append/change/unavailable classification, identities, overlap, cancellation, exit cleanup, echoed-secret suppression, weighting, zero-versus-missing usage, warmup exclusion and the 40-attempt cap.

Bounded numeric finding (Step 1): for the synthetic native fixture described above, the buffered and fragmented SSE paths both yield ordinary input `2`, cache read `98`, an output count of `7` and an independent charge of approximately `79e-6` (the earlier input-tier figure was approximately `128e-6`), and the per-request total and the aggregate do not double count the cached tokens.

## Blocked: controlled same-account live comparison (REQ-004 / AC-004)

Step 3 ran its single authorized live batch exactly once and produced no valid measurement, so AC-004 remains **blocked / unverified** rather than passed.

Preparation was operator-authorized and plan-scoped. A temporary preparation helper (a plan artifact, not a product feature) read only the `onespace-dev` profile's `.local_key` and `ai_gateway.json` and supplied the six `CACHE_DIAG_*` values to the child process in memory; it made no network call and wrote no project artifact. The delivered tool was unchanged: `compare` still reads only its process environment and performs no configuration discovery or mutation. For the batch the selected environment was the `onespace-dev` profile with one usable-source-key provider `gw-52efa67c8afb4687b251132fe12c1e72` mapping local `deepseek-v4.1-flash` to upstream `deepseek/deepseek-v4.1-flash` over `chat_completions`; the direct upstream credential was that same configured source key, so the account under test was the one the gateway serves. The single authorized non-target provider was temporarily disabled for the window and restored afterwards (the enabled count returned from `1` to `2` on restore and the latest safe check reports it restored); the target provider stayed enabled throughout, no service was restarted and no release configuration changed.

The sole `node tools/diagnose-deepseek-cache.mjs compare` invocation ran `2026-10-15T09:15:18.078Z`–`2026-10-15T09:15:21.111Z` (`17:15:18.078`–`17:15:21.111` UTC+08:00) and exited nonzero with:

- `attempts: 1` of the 40 allowed; `status: incomplete`, `reason: request-rejected`; `comparable: false`, `effectiveness: unverified`, `target_met: null`, `cost.availability: unknown`.
- one `series` entry — `path: gateway`, `round: 1`, `attempts: 1`; its `warmup` reports `requests: 1` with `valid_usage: 0`, its `measurement` reports `requests: 0`, every token sum is `0`, every `cache_hit_rate` is `null`, and `fields.native` / `fields.nested` are both `0`.
- the direct upstream path sent **no** request, because the gateway path stopped on its first attempt.

Because neither path yielded valid warmup or measured usage, there is no coverage from which to compute a rate. The batch therefore cannot be read as "the target was not met", and it is not a `0%` cold-cache observation either: a rejected request with no usage is unmeasured, not a miss.

The read-only `onespace-dev` usage check for the run window and the target provider/upstream identity found exactly **one** row — the rejected attempt: `timestamp_ms` `1792055721108`, `result: failure`, `status: 429`, `terminal: 1`, `usage_semantics: canonical_v1`, `usage_present: 0`, `cache_accounting_valid: 0`, every token tier `0`, `amount: 0.0`, `duration_ms` `1769`, and `first_token_ms` / `avg_tps` `null`. The window held one row, matched one expected identity and had no non-terminal row. This records the rejection truthfully but supplies no comparison cost or rate: the stored `0.0` fee is not a measured cost of the comparison, and the planned 4 warmups / 16 measurements were never reached.

A credential-unchanged re-check did not complete: reloading the dev comparison state after the run failed with a fixed missing-source-key reason, so the same-source-credential-unchanged verification could not be reconfirmed. A failed re-check is not evidence that a credential value changed. A later read-only inspection confirmed the target key was still present, enabled and value-bearing, but also that the gateway had set its own runtime quota flag on it from the `429` (`auto_marked: true`, `failure_kind: quota`, `marked_at` `1792055719`) without exposing a raw reason or key value. That mark is the gateway's natural runtime response to the rejection, not a diagnostic-tool leak and not a manual edit; clearing it is outside this plan's authorization, and the only state restored was the non-target provider's `enabled` flag.

No retry, second live batch, extra warmup or live `observe` ran. The plan's single authorized batch is spent: further measurement needs a new explicit authorization and a valid account or quota, and the remaining attempt budget must not be silently used up or the fixture changed. The same-account preconditions still hold for any future run — do not infer equivalence from a URL, a model name or a historical account name, and verify the gateway log rows for the run window against the expected provider/model identity. This plan's own natural-traffic observation also remains unrun: the operator did not select that temporary path, so the cause of the observed historical mutation is still unknown and the `>= 98%` cache effect remains **unproven**.

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

The 98% token-weighted cache-hit target for warmed requests with a stable long prefix is **not measured and not claimed as met**: the single authorized comparison batch produced no valid measurement, so the target was neither met nor missed. It cannot be established from the accounting or local mock tests alone, and a corrected accounting percentage is not evidence of improved physical cache reuse.

- Engineering: the Step 1 accounting correction and the Step 2 tooling and local behavior tests are delivered.
- Effectiveness: unverified. The one authorized comparison batch produced no valid measurement, so the target was neither met nor missed.

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
