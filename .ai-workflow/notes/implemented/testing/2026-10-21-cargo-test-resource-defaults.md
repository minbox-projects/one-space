# Agent Note: Cargo test resource defaults

Status: implemented

English | [中文](2026-10-21-cargo-test-resource-defaults.zh.md)

## Problem

Local Cargo test runs used machine-wide concurrency defaults, and the test profile optimized only the `sha2` dependency while the repository's own test code stayed at `opt-level = 1`. Warm suite runs on a 10-core Apple M1 Max were slow enough to discourage full validation. The earlier [Cargo test speedup](2026-09-18-cargo-test-speedup.md) record selected runtime-oriented settings for that stage and accepted that `[profile.test]` applies `opt-level = 1` to the root package; it did not set build jobs or harness thread defaults. Cargo and libtest consequently chose machine-wide values that varied with host load and concurrent builds.

## Decision

Default local Cargo builds and tests to bounded parallelism, and raise optimization for the repository's own test code without rebuilding dependencies at a higher level.

- `.cargo/config.toml` sets `[build] jobs = 4` and `[env] RUST_TEST_THREADS = { value = "4", force = false }`. Cargo applies `CARGO_BUILD_JOBS` and `cargo -j` before the config default, and `force = false` leaves an existing `RUST_TEST_THREADS` value or a harness `--test-threads` override in control, so the compiler and test harness defaults are independently overridable.
- `src-tauri/Cargo.toml` keeps `[profile.test]` at `opt-level = 1`, `debug = 0` and `split-debuginfo = "off"` for dependencies, retains `[profile.test.package.sha2] opt-level = 2`, and adds `[profile.test.package.OneSpace] opt-level = 2` so the root package's monomorphized PBKDF2/HMAC test code is optimized without raising dependency optimization. The production PBKDF2 work factor remains 100,000 iterations in `src-tauri/src/crypto.rs`; optimization is not achieved by lowering it.
- The library keeps `crate-type = ["staticlib", "cdylib", "rlib"]`. `[lib] doctest = false` and `[[bin]] name = "OneSpace", test = false` remain: backend behavior tests live in the library, and the desktop binary has no unit tests, so the default run currently has zero empty binary and documentation targets.
- Test fixtures in `src-tauri/src/ai_sessions/tests.rs` add the `parent_id` column to the four `session` and `session_v2` schema definitions used by the history tests, matching the production query, and the history assertions are unchanged. Two launcher sync tests replace fixed sleeps by setting an older file's modification time to 60 seconds before the current time with `File::set_modified`; they still verify the copy direction, so the copy-direction assertion is unchanged.

### Measurement evidence

A single warm CLI comparison against the previous `main` baseline `7635afe` measured harness 227.89 s with 1,649 passed, 3 failed and 11 ignored; the candidate with `OneSpace` at `opt-level = 2` and four test threads measured harness 160.60 s with 1,650 passed, 2 failed and 11 ignored. That is a 29.53% harness reduction in one comparison, but both sides contain failures, so it does not establish a green baseline or a passing candidate. Candidate compilation took 253 s and the command's real time was 416.02 s; an earlier 3,026.02 s raw-wall reading is contaminated by host sleep and must not be compared as a wall-time result.

Source advanced by 18 commits to `6863fa9`; the current tree contains 104 more test cases than the controlled 1,663-case run, so the later full run is not the same test set and does not replace the controlled comparison. On the delivered source at `6863fa92b91d4512fb1186d175f1599ab6a7070a`, the ordinary `cargo test --manifest-path src-tauri/Cargo.toml` run with no CLI overrides exited 0: 1,712 passed, 0 failed and 55 ignored, with harness 172.76 s, raw wall 175.90 s, user 127.33 s, sys 8.13 s, max RSS 152,354,816 bytes and warm Cargo startup 1.32 s. A separate `cargo test --no-run` compile exited 0 with real 263.24 s, user 904.88 s, sys 19.88 s and max RSS 9,272,983,552 bytes (8.64 GiB); it produced exactly one `src/lib.rs` test artifact and no empty binary or documentation target. Sampling the harness process tree every two seconds observed a 158.9% CPU peak and 155,472 KB (151.8 MiB) aggregate RSS, so four jobs and four threads bound local contention rather than enforcing a hard ceiling or 100% use of the 10-core host. The runner excluded 77 s of wait for an unrelated root compile before its separate timed commands. The 55 ignored cases describe this commit, not a permanent standard: the current main tree adds 44 ignored performance cases and 60 active cases relative to the controlled run, while these changes add no ignore annotation and remove no test.

The green full-suite run establishes correctness, not a general warm-suite speedup: the package-scoped `OneSpace` optimization can make the one-time test recompile slower, and cold-build speed is not proven.

## Alternatives considered

- Keep Cargo and libtest at host-wide defaults: declined because concurrent builds and host load made harness times unstable and starved interactive work; four jobs and four threads bound local contention while remaining overridable.
- Raise `[profile.test] opt-level` for all dependencies: declined because it rebuilds the dependency graph at a higher level and lengthens one-time compilation; package-scoped `OneSpace` and `sha2` optimization keeps the hot root code optimized without that cost.
- Enforce fixed four-thread execution with `force = true`: declined so that existing `RUST_TEST_THREADS` values and harness `--test-threads` flags keep precedence.
- Compare the original raw wall reading directly with the new command: declined because host sleep contaminated it; only harness values from the same incremental comparison are reported.

## Consequences

Local test runs now start with deterministic, bounded parallelism and more optimized root test code; the single controlled comparison measured a shorter harness, while the one-time test recompile may be slower than the previous `opt-level = 1` build and no general warm-suite speedup is guaranteed. The defaults are not a CI guarantee: a machine may override them through `CARGO_BUILD_JOBS`, `cargo -j`, `RUST_TEST_THREADS` or `--test-threads`.

Keep production cryptography, crate type and vendored OpenSSL / bundled SQLite unchanged. Preserve the fixture assertions when maintaining the history tests, and prefer explicit metadata changes over sleeps for mtime-sensitive checks. Report harness and raw wall time separately and do not treat a failing comparison as green.

This record partially supersedes the test-profile selection of [Cargo test speedup](2026-09-18-cargo-test-speedup.md): that record still owns the isolation, paused-clock, test-deletion and system-library decisions, while this record owns the current resource defaults and the root-package optimization. The prior record is retained with its historical measurements and evidence.

### References

- [Notes governance](../../README.md)
