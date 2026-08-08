# 02 - 实现环境管理切换后端及测试

- task_id: `ai-work-flow-environment-backend`
- order: `02`
- blocked_by: `ai-work-flow-install-backend`
- source_plan: `../plan.md`
- source_plan_digest: `873365474b78842c6e754f75e07240f5043a4d3d944dd53ce4fed3aa5b882c34`
- plan_id: `integrate-ai-work-flow`
- preview_revision: `1`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/ai_work_flow.rs`
  - `src-tauri/src/app_runtime/run_app.rs`

## 预期结果

在独立后端边界内实现环境 list、create、read、update、delete、use 和 status 命令，仅操作 ~/.config/ai-work-flow；校验环境名称、路径、普通文件、符号链接、完整 JSON 和 AI Work Flow 配置，并以原子方式保存。实现无标记时的 default 语义、删除当前环境回退、切换前有效性检查及既有 env use 能力的安全调用，失败时保持原文件、环境标记和 Agents 状态。补充该切片的 Rust 单元与命令测试，覆盖路径穿越和文件类型拒绝、校验失败不落盘、原子写入、状态回退、切换调用及与 OneSpace AI Environments 的数据和事件隔离。

## 实施清单

- [ ] 在既有 `ai_work_flow` 模块内增加环境名称、完整 JSON 文本/值、有效性和当前状态的数据模型，保持其与 OneSpace `ai_env`、`app_store` 服务商模型完全独立。
- [ ] 将唯一环境根固定为 `~/.config/ai-work-flow`，仅解析 `environments/<name>.json` 和 `.environment`；名称只允许 1 至 64 位字母、数字、点、下划线和连字符，并拒绝控制字符、路径分隔符和路径穿越。
- [ ] 对环境根、目录、文件和标记逐层校验，拒绝符号链接、非普通文件及根目录外目标；所有测试通过注入临时 home/config 根运行，不接触用户真实配置。
- [ ] 实现名单式 `list`、`create`、`read`、`update`、`delete`、`use`、`status` Tauri 命令；命令不接受任意路径或任意外部命令，并仅返回该域结构化结果和稳定错误。
- [ ] create/update 接收并保留完整 JSON，先解析 JSON，再通过受限 AI Work Flow 校验能力验证配置；验证成功后在同目录临时文件中完整写入、同步并原子替换，失败时保持原文件字节不变。
- [ ] 实现 `.environment` 缺失即 `default`；删除当前环境时移除标记或恢复等价安全默认状态并返回 `default`，删除非当前环境不改变当前状态。
- [ ] use 在任何状态变化前确认目标存在、是普通文件且 JSON/配置有效，再通过受限固定参数调用已安装 AI Work Flow 的 `env use` 能力；调用失败时保持环境文件、原 `.environment` 标记和已生成 Agents 状态。
- [ ] 在 `src-tauri/src/app_runtime/run_app.rs` 补齐七个环境命令注册，并用命令块断言保证不注册任意文件/进程能力、不复用 OneSpace AI Environments 命令。
- [ ] 补充 Rust 单元与命令测试，覆盖名称正反例、控制字符、穿越、符号链接、目录/设备等非普通文件、完整 JSON 往返、解析/配置校验失败不落盘、原子替换、default 状态、删除回退、切换前校验、切换成功/失败，以及数据和事件域隔离。

## 验收标准

- [ ] 七个环境命令只操作 `~/.config/ai-work-flow` 的规定文件，拒绝任意路径、非法名称、控制字符、符号链接、非普通文件和越界目标。
- [ ] create/read/update 保持完整 JSON；JSON 解析或 AI Work Flow 配置校验失败时不产生新文件且不改变已有文件，成功保存为原子替换。
- [ ] `.environment` 缺失稳定返回 `default`；删除当前环境后返回 `default`；删除非当前环境不改变当前选择。
- [ ] use 仅对存在且有效的环境调用固定 AI Work Flow `env use` 能力，失败时环境文件、标记和 Agents 状态均保持原样。
- [ ] 环境 API 不查询、修改、镜像或同步 OneSpace AI Environments 数据，也不发出其刷新事件。
- [ ] Rust 单元和命令测试在隔离临时目录中覆盖全部安全边界、原子性和状态语义并通过。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_work_flow::tests::environment`（按实现中的环境测试模块过滤名调整），预期名称、路径、文件类型、JSON 校验、原子写入、default 回退和 use 成败测试全部通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml app_runtime::run_app::tests`，预期七个环境命令各注册一次，且隔离断言通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期完整 Rust 测试通过，测试期间不访问真实 `~/.config/ai-work-flow` 或网络。

## 范围外事项

- 不改变任务 1 已建立的安装、更新、日志、锁和取消语义，除非为复用受限运行器而进行该模块内的必要扩展。
- 不实现前端环境列表或 JSON 编辑器；由 `ai-work-flow-tool-integration` 负责。
- 不迁移、转换或同步 OneSpace AI Environments 服务商配置，也不定义历史环境格式迁移。
