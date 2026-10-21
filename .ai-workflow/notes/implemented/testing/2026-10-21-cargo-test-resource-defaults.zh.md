# Agent Note: Cargo test resource defaults

Status: implemented

[English](2026-10-21-cargo-test-resource-defaults.md) | 中文

## Problem

本地 Cargo 测试运行采用整机范围的并发默认值，test profile 仅优化 `sha2` 依赖，而仓库自身测试代码停留在 `opt-level = 1`。在 10 核 Apple M1 Max 上，暖缓存测试套件慢到阻碍完整验证。此前的 [Cargo test speedup](2026-09-18-cargo-test-speedup.md) 记录为该阶段选择了偏向运行时间的设置，并接受 `[profile.test]` 对根包应用 `opt-level = 1`；它没有设置构建 job 或 harness 线程默认值。Cargo 与 libtest 因此采用整机范围的值，随主机负载和并发构建波动。

## Decision

将本地 Cargo 构建与测试的并行度默认限制在有界范围，并在不提高依赖优化级别的前提下提升仓库自身测试代码的优化。

- `.cargo/config.toml` 设置 `[build] jobs = 4` 与 `[env] RUST_TEST_THREADS = { value = "4", force = false }`。Cargo 在配置默认值之前应用 `CARGO_BUILD_JOBS` 与 `cargo -j`，而 `force = false` 让既有的 `RUST_TEST_THREADS` 值或 harness 的 `--test-threads` 覆盖继续生效，因此编译器与测试 harness 的默认值可独立覆盖。
- `src-tauri/Cargo.toml` 保持依赖的 `[profile.test]` 为 `opt-level = 1`、`debug = 0`、`split-debuginfo = "off"`，保留 `[profile.test.package.sha2] opt-level = 2`，并新增 `[profile.test.package.OneSpace] opt-level = 2`，在不提高依赖优化级别的情况下优化根包中单态化的 PBKDF2/HMAC 测试代码。生产 PBKDF2 工作因子仍为 `src-tauri/src/crypto.rs` 中的 100,000 次迭代；优化不通过降低该值实现。
- 库保持 `crate-type = ["staticlib", "cdylib", "rlib"]`。`[lib] doctest = false` 与 `[[bin]] name = "OneSpace", test = false` 保持：后端行为测试位于库中，桌面 binary 没有单元测试，因此默认运行当前没有空的 binary 与 documentation 目标。
- `src-tauri/src/ai_sessions/tests.rs` 的测试 fixture 为历史测试使用的四个 `session` 与 `session_v2` schema 定义增加 `parent_id` 列，与生产查询匹配，其历史断言保持不变。两个 launcher 同步测试不再使用固定 sleep，而用 `File::set_modified` 把较旧文件的修改时间设为当前时间前 60 秒；它们仍验证复制方向，因此复制方向断言保持不变。

### Measurement evidence

针对此前 `main` 基线 `7635afe` 的单次暖缓存 CLI 对比测得 harness 227.89 s，1,649 通过、3 失败、11 ignored；将 `OneSpace` 设为 `opt-level = 2` 并使用四个测试线程的候选测得 harness 160.60 s，1,650 通过、2 失败、11 ignored。这是单次对比中 29.53% 的 harness 降幅，但两侧都包含失败，因此不构成绿色基线或通过的候选。候选编译耗时 253 s，命令 real 时间为 416.02 s；更早的 3,026.02 s 原始 wall 读数受主机睡眠污染，不得作为 wall 时间结果对比。

源码前进 18 个提交至 `6863fa9`；当前树比受控的 1,663 用例运行多 104 个测试用例，因此后续全量运行不是同一测试集，不能替代该对比。在 `6863fa92b91d4512fb1186d175f1599ab6a7070a` 的交付源码上，不带任何 CLI 覆盖的普通 `cargo test --manifest-path src-tauri/Cargo.toml` 运行退出码 0：1,712 通过、0 失败、55 ignored，harness 172.76 s，原始 wall 175.90 s，user 127.33 s，sys 8.13 s，max RSS 152,354,816 字节，暖 Cargo 启动 1.32 s。另一次 `cargo test --no-run` 编译退出码 0，real 263.24 s，user 904.88 s，sys 19.88 s，max RSS 9,272,983,552 字节（8.64 GiB）；它恰好产出一个 `src/lib.rs` 测试产物，且没有空的 binary 或 documentation 目标。每两秒对 harness 进程树采样，观察到 CPU 峰值 158.9% 与 155,472 KB（151.8 MiB）聚合 RSS，因此四个 job 与四个线程限制本地争用，而不是强制硬上限或占满 10 核主机的 100%。runner 在各自计时命令之前排除了 77 s 的无关根编译等待。55 个 ignored 用例描述当前提交，不是永久标准：当前 main 相对受控运行新增 44 个 ignored 性能用例与 60 个活动用例，而本次改动没有新增 ignore 标注，也没有删除测试。

绿色全量运行确立正确性，而非普遍的暖缓存套件提速：包级 `OneSpace` 优化可能使一次性测试重编更慢，冷构建速度未被证明。

## Alternatives considered

- 保持 Cargo 与 libtest 的整机默认值：不采用，因为并发构建与主机负载使 harness 时间不稳定并挤占交互工作；四个 job 与四个线程限制本地争用且仍可覆盖。
- 为所有依赖提高 `[profile.test] opt-level`：不采用，因为它会以更高级别重建依赖图并延长一次性编译；包级 `OneSpace` 与 `sha2` 优化在不付出该成本的情况下保持热点根代码优化。
- 通过 `force = true` 强制固定四线程执行：不采用，以便既有的 `RUST_TEST_THREADS` 值与 harness `--test-threads` 参数保持优先。
- 将原始 wall 读数直接与新命令对比：不采用，因为主机睡眠污染了该读数；仅报告同一增量对比中的 harness 值。

## Consequences

本地测试运行现在以确定且有界的并行度启动，根测试代码也得到更多优化；单次受控对比测得 harness 更短，但一次性测试重编可能比此前 `opt-level = 1` 构建更慢，且不保证普遍的暖缓存套件提速。这些默认值不是 CI 保证：机器可通过 `CARGO_BUILD_JOBS`、`cargo -j`、`RUST_TEST_THREADS` 或 `--test-threads` 覆盖。

保持生产密码学、crate 类型与 vendored OpenSSL / bundled SQLite 不变。维护历史测试时保留 fixture 断言，并对 mtime 敏感检查优先使用显式元数据修改而非 sleep。分别报告 harness 与原始 wall 时间，不得把失败的对比当作绿色。

本记录部分替代 [Cargo test speedup](2026-09-18-cargo-test-speedup.md) 的 test profile 选择：该记录仍拥有隔离、paused 时钟、测试删除与系统库决策，而本记录拥有当前资源默认值与根包优化。此前记录连同其历史测量与证据保留。

### References

- [Notes governance](../../README.md)
