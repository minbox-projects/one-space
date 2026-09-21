# Agent Note: Cargo test speedup

Status: implemented

English | [中文](2026-09-18-cargo-test-speedup.zh.md)

## Problem

Warm local Rust tests were dominated by fixed sleeps, shared HOME serialization, real retry waits and unoptimized cryptographic work. The frozen plan `20260918-cargo-test-speedup` covers equivalent test acceleration and an evidence-based decision on system libraries on macOS arm64. Its original 573-test assumption and stable `--report-time` command do not match the execution environment.

This record documents implementation merged into main at `29b31c0`, according to the supplied closeout evidence; the owned worktree `.worktrees/20260918-cargo-test-speedup` had the same HEAD before this documentation update. One Spec Review and one Standards Review have completed; no repeat review is requested. Final root validation passed, but not all original acceptance criteria passed. The documentation maintainer did not rerun Rust checks or independently inspect commits. Frozen spec/plan remain unchanged; no run records are created.

## Decision

Keep the test isolation and timing changes, prioritize test runtime with the selected test profile, and retain vendored OpenSSL and bundled SQLite. Record the measured limitations rather than treating runtime improvement as incremental compilation improvement.

- Replace the two `hash_dir_ignores_file_mtime_when_content_is_unchanged` sleeps in skills/subagents with explicit file mtime changes using `File::set_modified`; retain both the fixed-mtime equality assertions and content-hash equality assertions. Distinguish Cargo command wall time from direct warm-binary wall time in the AC-001 evidence below.
- Use a thread-local `cfg(test)` app-directory guard behind `get_app_dir()` for 72 isolated tests, with an added concurrent isolation regression. It does not alter production signatures or production path resolution. Tests that directly mutate HOME or depend on global server state retain serialization; thread-local state is not a substitute for isolating process-global resources.
- Migrate five timing tests to paused Tokio time and use three arrival-interval assertions to remove auto-advance/I/O timing variability without weakening retry or backoff semantics. Keep real socket timing where that is the behavior under test. The 12 file_sharing tests already complete in 0.03 s; their 25 ms real-socket header guard needs no change. Keep the approximately 5.5 s blackhole test on real time because OS connect behavior is outside Tokio's virtual clock.
- Remove the redundant `end_to_end_below_threshold...` slow test (about 7.9 s) following the user's subsequent explicit request. Threshold unit coverage and the E2E covering three requests and 18 attempts remain. Fix the MCP mock response with `Connection: close`. The isolated-change baseline stage had 581 tests: one concurrent regression added and one redundant test removed, with 579 passing and two existing ignored tests; no new ignores were introduced. Concurrent main changes added 30 tests, bringing the final delivery snapshot to 611 tests, with 609 passing and the same two ignored tests. These counts are dated evidence, not permanent test-count requirements.
- Retain `[profile.test]` with `opt-level = 1`, `debug = 0`, `split-debuginfo = "off"`, and `[profile.test.package.sha2]` with `opt-level = 2`. Cryptographic parameters remain unchanged, including PBKDF2 iterations of 100,000, verified against `PBKDF2_ITERATIONS` in `src-tauri/src/crypto.rs`; optimization is not achieved by reducing cryptographic work factors. Keep `[lib] crate-type = ["staticlib", "cdylib", "rlib"]`.

### Authorization record

The primary orchestrator supplied the following explicit user authorizations, including the final approval after both reviews. This completes the authorization evidence for this implementation; it does not rewrite the frozen spec/plan or assert that unmet metrics passed.

- Use stable-compatible equivalent timing and the actual 581-test baseline instead of the original 573-test assumption and unsupported `--report-time`.
- Extend the implementation write scope to test isolation in `src-tauri/src/config.rs`.
- Delete the duplicate slow test as a subsequent explicit scope revision, not as a means of meeting the original criteria or complying with their prohibition on deleting slow tests; retain the coverage described above.
- Continue the mock repair and runtime optimization. The four-line MCP mock test repair is also explicitly approved for merge.
- The final user response, “同意” (agree), explicitly accepts the original AC-003 individual-test <100 ms shortfall (0.18–0.37 s) and AC-004 incremental improvement ≥10% shortfall (5.94→6.54 s, 0.60 s slower), and authorizes fixing all documentation findings before proceeding with merge. These deviations remain accepted, not reclassified as passing, after integration into main at `29b31c0`.

### Measurement evidence

The following are supplied implementation results, not checks run by this documentation task. Stable `--report-time` is unsupported; the user explicitly authorized equivalent timing and the 581-test baseline, then requested slow-test deletion and continuation. The profile prioritizes runtime with the user's explicit acceptance of the remaining deviations. That acceptance does not establish that AC-003 or AC-004 passed.

AC-001 command-level evidence (commands run from `src-tauri/`): the original skills and subagents medians were 1.21 s and 1.20 s respectively, each from three runs. The actual post-change commands were `cargo test skills::tests::hash_dir_ignores_file_mtime_when_content_is_unchanged -- --exact` and `cargo test subagents::tests::hash_dir_ignores_file_mtime_when_content_is_unchanged -- --exact`, not `cargo test --lib`. Three Cargo real times were skills 0.37 / 0.39 / 0.39 s and subagents 0.37 / 0.38 / 0.37 s; libtest reported 0.00 s on every run, all exit 0. Running the warm test binary directly with the same full-name filter and `--exact` gave real 0.01 s in each of three runs per test, all exit 0. Cargo real time includes Cargo startup and is not individual-test execution time; direct-binary real time still includes process startup. Fixed-mtime equality assertions remain intact.

AC-003 module evidence: under the new profile before module migration, `cargo test --lib api_fusion::tests` recorded real 73.19 s / harness 72.60 s, exit 101. One existing paused `cooling_provider...` case failed with elapsed 2.037 s > 1.9 s. Within the same scope, three existing paused cases replaced whole-operation elapsed assertions with the interval between two upstream arrivals while preserving the bounds. After repair and migration, the same command passed all 167 tests on three runs, exit 0 each, with real 16.20 / 16.23 / 16.11 s, median 16.20 s, approximately 77.9% faster than 73.19 s. The baseline includes a failure: this is not an all-green baseline comparison and does not establish the individual-test <100 ms target.

The table below preserves historical measurements from the isolated-change baseline stage, including the 581-test count and 27.68 s harness median; it does not describe the final integrated suite.

| Measurement | Observed result | Interpretation |
| --- | --- | --- |
| Three consecutive full library harness runs | 27.68 / 28.09 / 26.85 s; median 27.68 s | Each run: 579 passed, two existing ignored |
| Corresponding raw wall times | 28.26 / 42.97 / 27.26 s; median 28.26 s | Second run included 14.80 s of external shared-target contention; do not report a subtraction estimate as measured wall time |
| Serial library control | Harness 46.49 s; wall 46.89 s | Default parallel raw wall median is lower |
| Historical harness comparisons | 690.24 → 27.68 s; 421.35 → 27.68 s | Approximately 96.0% and 93.4% reductions respectively; these are not wall-time comparisons |
| Default `cargo test` across default targets | 579 passed, two existing ignored | Successful supplied smoke result |
| `cargo check` | Passed | Successful supplied compile check |
| Profile incremental rebuild comparison | 5.94 → 6.54 s | 0.60 s slower; original AC-004 improvement of at least 10% was not met |
| Migrated individual test timing | 0.18–0.37 s | Original AC-003 limit below 100 ms was not met |

Final delivery snapshot on 2026-09-18, supplied for main `29b31c0`: `cargo test --manifest-path src-tauri/Cargo.toml` from the project root exited 0. The library reported 609 passed, zero failed and two existing ignored tests (611 total), with harness time 29.57 s. Raw wall time was 107.18 s, including recompilation after switching to the root checkout; this is not a warm-run wall measurement. Binary and documentation targets each ran zero tests successfully. Representative frontend validation passed 55 tests across two files, and the integration check passed. The additional 30 tests came from concurrent main changes; the earlier 581-test measurements remain historical evidence for this change in isolation.

The original full acceptance criteria therefore remain only partially satisfied. The revised count and timing method are explicitly authorized departures; the removed redundant test is a later user-directed scope adjustment, not evidence of satisfying the frozen prohibition on deleting slow tests. Both reviews have completed and the user explicitly accepted the AC-003/AC-004 deviations; integration does not change that distinction.

### System-library evaluation

Step 5 ran on Apple M1 Max arm64, macOS 27, rustc/cargo 1.93.1. Homebrew OpenSSL 3.6.4 and SDK SQLite were already available; no packages were installed. A macOS-scoped candidate removing vendored/bundled features compiled successfully.

| Measurement | Baseline | System-library candidate |
| --- | --- | --- |
| Warm `cargo test --no-run`, same `lib.rs` mtime trigger, three runs | 6.23 / 6.06 / 7.27 s | 6.03 / 6.26 / 6.18 s |
| Median | 6.23 s | 6.18 s |
| Reduction | Reference | 0.80%, below the 15% adoption gate |

The approximately 77 s cold conversion is excluded from this warm comparison. `Cargo.toml` and `Cargo.lock` were restored to their pre-candidate dependency state while preserving the selected test profile. The system-library replacement is not delivered. The candidate failed the benefit gate, so a full candidate test run was unnecessary; successful compilation alone is not evidence of candidate full-suite success. This fulfills the evidence-based retention decision in AC-005, without claiming candidate adoption or cross-platform results.

## Alternatives considered

- Keep global HOME locking everywhere: declined for tests served solely by the isolated app directory because it serialized independent work. Retain locking for direct HOME access and global server state.
- Convert every socket timeout to paused time: declined for real OS connect and header behavior; the measured file_sharing suite already has negligible cost.
- Retain the redundant slow threshold E2E: declined after the user's deletion request; threshold unit and three-request/18-attempt E2E coverage remain.
- Select the profile only on incremental compilation benefit: not the final orchestrator choice. The selected profile favors runtime despite the measured 0.60 s incremental regression; AC-004 remains unmet.
- Replace vendored OpenSSL and bundled SQLite on macOS: declined because the measured warm median benefit is only 0.80%, below 15%, despite available libraries and successful candidate compilation.

## Consequences

Future tests should change mtime explicitly when checking content-hash independence, isolate app-directory access only where thread-local scope fits execution, preserve serialization for global state, and distinguish virtual-clock assertions from real I/O timing. Preserve the cryptographic work factor and the remaining behavioral coverage. Test profile changes trade runtime against rebuild time and debug information; `debug = 0` limits test debugging information.

Report harness and raw wall time separately, retain contention in raw measurements, and compare warm incremental builds under equivalent triggers and cache conditions. The completed dual-axis review and final user approval resolve the disposition of the original AC-003/AC-004 gaps as accepted deviations, not passing metrics. Integration into main at `29b31c0` and final root validation are complete. Linux/Windows and cold-build speedups are not established. The existing two ignored tests remain coverage gaps, not new exclusions introduced for speed.

Supersession assessment: the prior authorized proposed/implemented search found no related active testing-strategy note; this record supersedes none. This closeout updates delivery facts without changing the decision or its rationale and introduces no new supersession. No module boundary, public production symbol, indexed path or navigation workflow changes require a new feature entry. This closeout updates only the owned worktree's `MEMORY.md` and this root single-source note triplet; root `MEMORY.md` is left for the subsequent worktree commit and integration.

### References

- [Frozen specification](../../../plans/20260918-cargo-test-speedup/spec.md)
- [Frozen implementation plan](../../../plans/20260918-cargo-test-speedup/plan.md)
- [Notes governance](../../README.md)
