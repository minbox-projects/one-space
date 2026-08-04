# 03 - 建立应用启动迁移门禁

- task_id: `startup-migration-gate`
- order: `03`
- blocked_by: `refinery-migration-coordinator`
- source_plan: `../plan.md`
- source_plan_digest: `060dc325029ae1736a7ed8c01a8e12901bcc2ce9d5b304c611e43b6b4425dcd6`
- write_scope: `src-tauri/src/app_runtime/run_app.rs, src-tauri/src/ai_routing_gateway/commands/mod.rs, src-tauri/src/ai_routing_gateway/storage.rs`

## 预期结果

应用启动时先完成唯一的受控数据库迁移门禁；迁移失败会在任何 IPC 关联服务、HTTP runtime、scheduler 或后台任务可用前记录原因、显示原生阻塞错误并退出。

## 实施清单

- [ ] 在 `run_app::setup` 中同步或受控等待共享数据库启动迁移入口，并将其置于所有现有服务注册、初始化和 spawn 之前。
- [ ] 重排协议路由、网关初始化、会话同步、AI scheduler、SSH 监控及其他后台工作，确保仅在门禁成功后启动。
- [ ] 保持网关 `prepare_startup`、`initialize_managed` 和 maintenance scheduler 的业务 API 与成功顺序，避免后续普通连接重复执行迁移。
- [ ] 为启动失败建立可测试的报告边界：记录结构化错误类别及底层上下文，显示不含敏感信息的原生阻塞对话框，然后调用应用退出。
- [ ] 确保门禁失败不会遗留部分启动的 runtime、scheduler、协议路由或后台任务。
- [ ] 添加启动顺序测试，覆盖成功顺序、失败短路、对话框报告和退出调用，不依赖图形环境。
- [ ] 完成本任务 checklist 更新。

## 验收标准

- [ ] 数据库迁移成功是 IPC 关联服务、HTTP runtime、scheduler 和所有后台任务启动的前置条件。
- [ ] 迁移失败时服务启动和后台 spawn 的观察计数均为零，并触发一次日志、一次原生错误报告和退出。
- [ ] 对话框内容明确说明数据库迁移阻塞启动，但不包含 SQL、密钥或业务数据。
- [ ] 日志包含可检索的错误类别及用于诊断的底层原因。
- [ ] 成功路径保持既有初始化相对顺序，网关 runtime 和 scheduler 仅启动一次。
- [ ] 后续网关及 HTTP runtime 打开的普通连接不重新执行已完成迁移。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml app_runtime -- --nocapture`，预期门禁成功和失败顺序测试通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway -- --nocapture`，预期网关生命周期和 scheduler 测试通过。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`，预期 Tauri dialog、日志和退出路径编译通过。

## 范围外事项

不改变网关业务 API、不新增前端错误页面，也不提供数据库自动备份、逆迁移或恢复功能。
