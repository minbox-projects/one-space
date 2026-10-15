# DeepSeek cache accounting and diagnostics

This report tracks the `20261015-deepseek-cache-diagnostics` plan. It is maintained incrementally: it records delivered facts at each accepted boundary and states the remaining work and any blocked evidence explicitly. Only Step 1 is delivered; Steps 2 and 3 are not.

## Scope

The plan separates an accounting correction from a measured improvement in upstream cache reuse. Truthful native DeepSeek cache accounting and a characterization of the existing Chat forwarding contract are delivered. A temporary single-conversation diagnostic tool and a controlled upstream-versus-gateway comparison are still pending.

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

## Evidence (Step 1)

The behavior tests live in `src-tauri/src/ai_gateway/tests/deepseek_cache.rs`, registered as `mod deepseek_cache;` in `src-tauri/src/ai_gateway/tests.rs`.

| Check | Result |
| --- | --- |
| `deepseek_cache` RED | 18 selected tests: 14 fail on the old numeric result, 4 existing-behavior tests already pass |
| `deepseek_cache` GREEN | exit 0, 18 pass |
| `canonical_` | exit 0, 16 selected: 15 pass, 1 pre-existing ignored |
| `cachehit_` | exit 0, 9 pass |
| `session_affinity_` | exit 0, 24 pass |
| `streaming_forward_preserves_bytes_captures_usage_and_fails_all_unavailable` | exit 0, 1 pass |
| `forwarded_logs_never_contain_keys_headers_or_bodies` | exit 0, 1 pass |

Bounded numeric finding: for the synthetic native fixture described above, the buffered and fragmented SSE paths both yield ordinary input `2`, cache read `98`, an output count of `7` and an independent charge of approximately `79e-6` (the earlier input-tier figure was approximately `128e-6`), and the per-request total and the aggregate do not double count the cached tokens.

## Not yet delivered (Steps 2 and 3)

- The temporary observation and comparison tool is not delivered; the standalone on-demand entry does not exist yet, and none of the planned `CACHE_DIAG_*` credential environment variables are defined.
- The bounded same-account live comparison has not run: attempts `0`, no isolated single-source test environment was provided, and no post-run usage rows were verified.
- Consequently the measured upstream-versus-gateway effectiveness rate is not available.

## Effectiveness target

The 98% token-weighted cache-hit target for warmed requests with a stable long prefix is **not met, not measured and not claimed**. It cannot be established from the accounting tests alone, and a corrected accounting percentage is not evidence of improved physical cache reuse. The engineering accounting correction (Step 1) is complete; the effectiveness result stays unverified until the bounded comparison runs with verified account equivalence.

## Interpretation decision table

| Observation after a completed comparison | Conclusion | Required follow-up |
| --- | --- | --- |
| An unchanged prefix with low direct-upstream hits | Upstream evidence | Bounded provider-side follow-up |
| A client-side rewrite of the previous fields | Client evidence | Bounded client follow-up |
| A newly evidenced gateway defect outside the delivered scope | Gateway defect | Bounded follow-up decision before any edit |
| Fresh-input growth with a growing prefix | Separate from cache reuse | Reported separately, not as a cache verdict |
| Missing account evidence, rejected options, failed calls or absent/invalid measured usage | Not comparable | Remains unverified or blocked; never a passing comparison |

## References

- [DeepSeek context caching](https://api-docs.deepseek.com/zh-cn/guides/kv_cache)
- [DeepSeek Chat API](https://api-docs.deepseek.com/api/create-chat-completion)
