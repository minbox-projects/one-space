# 01 - 实现 AI Work Flow 安装更新后端及测试

- task_id: `ai-work-flow-install-backend`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `873365474b78842c6e754f75e07240f5043a4d3d944dd53ce4fed3aa5b882c34`
- plan_id: `integrate-ai-work-flow`
- preview_revision: `1`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/ai_work_flow.rs`
  - `src-tauri/src/lib.rs`
  - `src-tauri/src/app_runtime/run_app.rs`

## 预期结果

建立独立 AI Work Flow Rust 模块并注册受限 Tauri 命令，定义固定仓库 URL 与受管理路径、命令白名单、安全路径校验、稳定错误映射、运行状态、结构化日志、取消信号和全局单任务锁；实现首次临时目录安全克隆与原子替换、既有仓库固定 origin 更新，以及严格串行的 npm ci、安装脚本和 validate 流程。补充该切片的 Rust 单元与命令测试，验证路径和命令边界、固定阶段顺序、版本与状态转换、并发去重、取消、失败终止及日志保留，并确保不接入 OneSpace AI Environments。

## 实施清单

- [x] 在 `src-tauri/src/ai_work_flow.rs` 建立独立领域类型：安装/更新操作、idle/running/succeeded/failed/cancelled 状态、阶段、版本、稳定错误代码、带序号和 stdout/stderr 来源的结构化日志，以及可序列化的命令返回模型。
- [x] 通过 `config::get_app_dir()` 固定解析 `<app-data>/ai-work-flow/repository`，拒绝应用数据目录外路径、符号链接和不符合预期的文件类型；不得探测、读取、写入或迁移 `~/AiHistorys/ai-work-flow`。
- [x] 固定远端为 `https://github.com/hengboy/ai-work-flow.git`，建立仅允许既定 git、npm、node 可执行文件和参数数组的内部运行器；公开命令不得接受 URL、任意命令、工作目录或 dry-run 参数。
- [x] 首次安装时 clone 到受管理仓库同级的唯一临时目录，校验 clone 结果后原子替换目标，并在失败或取消时清理临时产物且不留下半安装仓库。
- [x] 更新时校验目标是预期普通目录和 Git 工作树，将 origin 约束到固定远端后执行固定拉取；首次安装和更新随后都严格串行执行 `npm ci`、`node agent-build/install.mjs`、`node agent-build/install.mjs validate`，任一阶段失败立即停止。
- [x] 实现进程输出采集、阶段/时间/结果记录、安装版本解析和稳定错误映射；所有成功、失败和取消路径均保留完整有序日志。
- [x] 实现进程级全局单任务锁与取消信号：重复启动返回当前运行状态且不创建新进程，取消仅终止当前受管理子进程，无活动任务时返回可辨识结果。
- [x] 在 `src-tauri/src/lib.rs` 声明模块，并在 `src-tauri/src/app_runtime/run_app.rs` 仅注册安装状态/版本、安装或更新、取消和日志查询命令；不得导入或调用 `ai_env`、`app_store` 及 OneSpace AI Environments 的模型、存储或事件。
- [x] 在模块内 `#[cfg(test)]` 及运行时命令注册测试中使用临时目录和模拟进程覆盖固定常量、路径/文件类型边界、命令白名单、完整阶段顺序、首次 clone、既有仓库 pull、状态与版本转换、并发去重、取消和逐阶段失败日志；测试不得访问网络或用户真实目录。

## 验收标准

- [x] 首次安装仅从固定 GitHub URL clone 到应用数据目录内的临时目录，经校验后原子发布；更新仅操作该独立受管理副本并约束固定 origin。
- [x] 两条路径均按固定顺序完成 npm ci、安装脚本和 validate，不使用 dry-run 或二次确认；任何失败或取消都会停止后续阶段并保留日志。
- [x] 任意 URL、命令、参数、越界路径、符号链接或非预期文件类型均在启动进程前被拒绝，并返回稳定、可辨识错误。
- [x] 状态完整覆盖 idle、running、succeeded、failed、cancelled，包含操作、阶段、时间、版本/错误和有序日志；并发请求不会产生重叠子进程。
- [x] Tauri 命令恰好注册在独立 AI Work Flow 域，安装切片不读取、写入、同步或触发 OneSpace AI Environments。
- [x] Rust 单元和命令测试覆盖成功、失败、并发、取消与安全边界，且全部通过。

## 验证步骤

- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_work_flow`，预期安装后端的路径、命令、顺序、状态、锁、取消和失败日志测试全部通过，且无网络及真实用户目录访问。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml app_runtime::run_app::tests`，预期 AI Work Flow 安装相关命令注册断言通过且未混入 AI Environments 命令块。
- [x] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`，预期新模块、状态和 Tauri 命令编译成功且无错误。

## 范围外事项

- 不实现环境文件 list/create/read/update/delete/use/status；由 `ai-work-flow-environment-backend` 负责。
- 不实现更多工具导航、页面和前端交互；由 `ai-work-flow-tool-integration` 负责。
- 不迁移旧仓库，不提供可配置远端、任意 shell、dry-run 或额外确认流程。
