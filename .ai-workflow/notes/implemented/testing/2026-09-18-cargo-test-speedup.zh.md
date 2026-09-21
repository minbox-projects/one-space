# Agent Note: Cargo test speedup

Status: implemented

[English](2026-09-18-cargo-test-speedup.md) | 中文

## Problem

本地 Rust 暖缓存测试的主要耗时来自固定 sleep、共享 HOME 串行化、真实重试等待及未优化的密码学计算。冻结计划 `20260918-cargo-test-speedup` 涵盖等价测试加速及 macOS arm64 系统库替换的证据决策。原计划的 573 测试假设和 stable `--report-time` 命令与执行环境不符。

本记录依据提供的收尾证据，描述已合并至 main `29b31c0` 的实现；owned worktree `.worktrees/20260918-cargo-test-speedup` 在本次文档更新前具有相同 HEAD。一次 Spec Review 与一次 Standards Review 已完成，不再要求重复审查。root 最终验证通过，但不是全部原始验收条件通过。文档维护者未重跑 Rust 检查，也未独立检查提交。冻结 spec/plan 保持不变，不生成 run records。

## Decision

保留测试隔离与计时改动，使用所选 test profile 优先改善测试运行时间，并保留 vendored OpenSSL 与 bundled SQLite。记录实测限制，不把运行时间改善当作增量编译改善。

- 将 skills/subagents 两个 `hash_dir_ignores_file_mtime_when_content_is_unchanged` 中的 sleep 替换为 `File::set_modified` 显式修改文件 mtime；保留固定 mtime 相等断言及内容哈希相等断言。下文 AC-001 证据区分 Cargo 命令 wall 时间与直接运行热 binary 的 wall 时间。
- 在 `get_app_dir()` 后使用线程局部的 `cfg(test)` 应用目录 guard，为 72 个测试提供隔离，并新增并发隔离回归。不修改生产函数签名或生产路径解析。直接修改 HOME 或依赖全局 server 状态的测试仍串行；线程局部状态不能代替进程全局资源隔离。
- 将五个计时测试迁至 Tokio paused 时间，使用三个到达间隔断言消除 auto-advance/I/O 计时波动，不弱化重试或退避语义。被测行为涉及真实 socket 时保留真实计时。12 个 file_sharing 测试已在 0.03 s 内完成，其 25 ms 真实 socket header guard 无需修改。约 5.5 s 的 blackhole 测试保留真实时间，因为 OS connect 行为不受 Tokio 虚拟时钟控制。
- 按用户随后明确提出的要求，删除冗余慢测试 `end_to_end_below_threshold...`（约 7.9 s）。保留阈值单元覆盖及包含三次请求、18 次尝试的 E2E。为 MCP mock 响应补充 `Connection: close`。变更独立基线阶段共 581 例：新增一个并发回归、删除一个冗余测试，579 通过、两个既有 ignored；未新增 ignore。同期 main 变更新增 30 例，最终交付快照共 611 例，609 通过且保留相同的两个 ignored。这些数量是特定时点的证据，不是永久测试数量要求。
- 保留 `[profile.test]` 的 `opt-level = 1`、`debug = 0`、`split-debuginfo = "off"`，以及 `[profile.test.package.sha2]` 的 `opt-level = 2`。密码学参数不变，包括 PBKDF2 的 100,000 次迭代，已核对 `src-tauri/src/crypto.rs` 中的 `PBKDF2_ITERATIONS`；优化不通过降低密码学工作因子实现。保持 `[lib] crate-type = ["staticlib", "cdylib", "rlib"]`。

### Authorization record

主编排提供了以下用户明确授权，包括双轴审查后的最终同意。此处补齐本次实现的授权证据，不改写冻结 spec/plan，也不声称未达标指标通过。

- 使用 stable 支持的等价计时与实际 581 测试基线，替代原 573 测试假设及不支持的 `--report-time`。
- 将实现写入范围扩展至 `src-tauri/src/config.rs` 的测试隔离。
- 删除重复慢测试属于后续明确范围修订，不作为原条件达标或符合其禁止删除慢测试条款的手段；保留上文所述覆盖。
- 继续推进 mock 修复与运行期优化。四行 MCP mock 测试修复也已明确同意合并。
- 用户最终回复“同意”，明确接受原 AC-003 单例 <100 ms 未达标（0.18–0.37 s）及 AC-004 增量改善 ≥10% 未达标（5.94→6.54 s，慢 0.60 s），并授权修复全部文档问题后继续合并。合并至 main `29b31c0` 后，上述偏差仍为已接受，不改称达标。

### Measurement evidence

以下为提供给本次任务的实现结果，不是本次文档任务运行的检查。Stable 不支持 `--report-time`；用户明确授权等价计时和 581 测试基线，随后要求删除慢测试并继续。该 profile 优先改善运行时间，用户已明确接受剩余偏差。接受偏差不代表 AC-003 或 AC-004 已通过。

AC-001 命令级证据（命令在 `src-tauri/` 下运行）：原 skills 与 subagents 中位数分别为 1.21 s 和 1.20 s，各运行三次。改后实际命令为 `cargo test skills::tests::hash_dir_ignores_file_mtime_when_content_is_unchanged -- --exact` 和 `cargo test subagents::tests::hash_dir_ignores_file_mtime_when_content_is_unchanged -- --exact`，不是 `cargo test --lib`。三次 Cargo real 时间为 skills 0.37 / 0.39 / 0.39 s、subagents 0.37 / 0.38 / 0.37 s；libtest 每次报告 0.00 s，全部 exit 0。以相同完整名称过滤器和 `--exact` 直接运行热测试 binary，各测试三次 real 均为 0.01 s，全部 exit 0。Cargo real 时间包含 Cargo 启动，不是单例执行时间；直接 binary 的 real 时间仍包含进程启动。固定 mtime 相等断言保留。

AC-003 模块证据：新 profile 下、模块迁移前，`cargo test --lib api_fusion::tests` 记录 real 73.19 s / harness 72.60 s，exit 101。一个既有 paused `cooling_provider...` 用例以 elapsed 2.037 s > 1.9 s 失败。在同一范围内，三个既有 paused 用例将整体操作 elapsed 断言改为两次 upstream 到达间隔，保持原界限。修复与迁移后，相同命令三次均为 167 测试通过、各 exit 0，real 16.20 / 16.23 / 16.11 s，中位数 16.20 s，相对 73.19 s 约快 77.9%。基线包含失败：这不是全绿基线对比，也不证明单例 <100 ms 目标达标。

下表保留变更独立基线阶段的历史测量，包括 581 例和 27.68 s harness 中位数；它不描述最终集成后的测试集。

| Measurement | Observed result | Interpretation |
| --- | --- | --- |
| 连续三次全量 library harness | 27.68 / 28.09 / 26.85 s；中位数 27.68 s | 每轮：579 通过、两个既有 ignored |
| 对应原始 wall 时间 | 28.26 / 42.97 / 27.26 s；中位数 28.26 s | 第二轮含 14.80 s 外部共享 target 争用；不得将扣除后的估算报告为实测 wall 时间 |
| 串行 library 对照 | Harness 46.49 s；wall 46.89 s | 默认并行原始 wall 中位数更低 |
| 历史 harness 对比 | 690.24 → 27.68 s；421.35 → 27.68 s | 分别下降约 96.0% 和 93.4%；这不是 wall 时间对比 |
| 默认 `cargo test` 的默认全目标 | 579 通过、两个既有 ignored | 提供的冒烟结果成功 |
| `cargo check` | 通过 | 提供的编译检查成功 |
| Profile 增量重编对比 | 5.94 → 6.54 s | 慢 0.60 s；未达到原 AC-004 至少改善 10% 的要求 |
| 迁移测试的单例耗时 | 0.18–0.37 s | 未达到原 AC-003 小于 100 ms 的要求 |

2026-09-18 最终交付快照，所提供证据对应 main `29b31c0`：在项目 root 执行 `cargo test --manifest-path src-tauri/Cargo.toml`，exit 0。Library 报告 609 通过、零失败、两个既有 ignored（共 611 例），harness 29.57 s。原始 wall 为 107.18 s，包含切换到 root checkout 后的重编；这不是热运行 wall 测量。Binary 与 documentation 目标各运行零测试且成功。前端代表验证共 2 文件、55 测试通过，集成 check 通过。新增 30 例来自同期 main 变更；此前 581 例的测量保留为本次变更独立阶段的历史证据。

因此原始全部验收条件仅部分满足。修订数量和计时方法属于明确授权的偏离；删除冗余测试是用户后续指示的范围调整，不是满足冻结文件禁止删除慢测试条款的证据。双轴审查已完成，用户已明确接受 AC-003/AC-004 偏差；集成不改变这一区分。

### System-library evaluation

Step 5 在 Apple M1 Max arm64、macOS 27、rustc/cargo 1.93.1 上执行。Homebrew OpenSSL 3.6.4 与 SDK SQLite 已可用，未安装任何包。仅针对 macOS 移除 vendored/bundled features 的候选方案编译成功。

| Measurement | Baseline | System-library candidate |
| --- | --- | --- |
| 暖缓存 `cargo test --no-run`，相同 `lib.rs` mtime 触发，三轮 | 6.23 / 6.06 / 7.27 s | 6.03 / 6.26 / 6.18 s |
| 中位数 | 6.23 s | 6.18 s |
| 降幅 | 对照 | 0.80%，低于 15% 采用门槛 |

约 77 s 的冷转换不计入该暖缓存对比。`Cargo.toml` 与 `Cargo.lock` 已恢复为候选实验前的依赖状态，同时保留所选 test profile。系统库替换未落地。候选未通过收益门槛，因此无需再运行候选全量测试；编译成功本身不是候选全量测试成功的证据。这完成 AC-005 基于证据保留依赖的决策，不声称采用候选或获得跨平台结果。

## Alternatives considered

- 处处保留全局 HOME 锁：仅通过隔离应用目录访问数据的测试不采用此方案，因为它让独立工作串行。直接访问 HOME 和全局 server 状态仍保留锁。
- 将所有 socket 超时转换为 paused 时间：真实 OS connect 与 header 行为不采用此方案；实测 file_sharing 测试组耗时已很低。
- 保留冗余慢阈值 E2E：用户要求删除后不采用；保留阈值单元覆盖及三请求/18 尝试 E2E。
- 仅根据增量编译收益选择 profile：这不是主编排最终选择。所选 profile 优先改善运行时间，接受实测 0.60 s 增量回退；AC-004 仍未满足。
- 在 macOS 替换 vendored OpenSSL 和 bundled SQLite：虽然系统库可用且候选编译成功，但暖缓存中位数收益只有 0.80%，低于 15%，因此不采用。

## Consequences

后续内容哈希独立性测试应显式修改 mtime；应用目录访问仅在执行方式适合线程局部作用域时使用隔离；全局状态仍须串行；虚拟时钟断言与真实 I/O 计时须区分。保持密码学工作因子及剩余行为覆盖。Test profile 在运行时间、重编耗时与调试信息之间存在取舍；`debug = 0` 限制测试调试信息。

分别报告 harness 与原始 wall 时间，在原始测量中保留争用，并在等价触发方式与缓存条件下比较暖增量编译。已完成的双轴审查和用户最终同意将原 AC-003/AC-004 缺口明确处置为已接受偏差，而非指标达标。已完成合并至 main `29b31c0` 及 root 最终验证。未建立 Linux/Windows 或冷编译提速结论。两个既有 ignored 测试仍是覆盖缺口，不是本次为提速新增的排除项。

替代关系评估：此前授权的 proposed/implemented 检索未发现相关活动测试策略笔记；本记录不替代其他笔记。本次收尾仅更新交付事实，不改变决策或理由，也不引入新的替代关系。本次没有需要新增 feature 条目的模块边界、生产公共符号、索引路径或导航流程变化。本次收尾仅更新 owned worktree 的 `MEMORY.md` 与 root 单源 note 三件套；root `MEMORY.md` 留待后续 worktree 提交与集成。

### References

- [冻结规格](../../../plans/20260918-cargo-test-speedup/spec.md)
- [冻结实施计划](../../../plans/20260918-cargo-test-speedup/plan.md)
- [笔记治理](../../README.md)
