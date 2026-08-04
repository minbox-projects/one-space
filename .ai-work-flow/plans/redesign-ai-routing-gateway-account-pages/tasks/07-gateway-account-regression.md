# 07 - 完成跨层安全与交互回归

- task_id: `gateway-account-regression`
- order: `07`
- blocked_by: `atomic-api-key-account-save, gateway-account-page-integration`
- source_plan: `../plan.md`
- source_plan_digest: `6f7f72cf85831fcd249370dc3a8fdcf7ecc5d3981bcc238427b969a3642dbbe1`
- write_scope: `AI 路由网关相关前后端实现与测试模块，非穷举`

## 预期结果
账号页面重设计通过完整前端质量检查和 Rust 定向回归，跨层证明原子保存、价格继承、明文隔离、OAuth 只读及列表/详情交互满足批准规格。

## 实施清单
- [ ] 审核前后端字段命名、命令注册、错误代码和 facade 参数是否完全一致，补齐发现的跨层测试缺口。
- [ ] 使用虚构 fixture secret 验证详情专用返回与列表、Bootstrap、事件、日志、错误之间的明文隔离。
- [ ] 回归已有账号数据兼容行为，包括缺失官方映射补齐、历史价格有效值、OAuth 只读和删除确认。
- [ ] 运行完整前端测试、lint、build 和 Rust AI 路由网关定向测试；共享 SQLite 契约受影响时扩大 Rust 测试范围。
- [ ] 仅修复本计划实现引起的失败，并记录无法在当前环境执行的检查及原因。

## 验收标准
- [ ] 前端完整测试、lint 和生产构建全部通过。
- [ ] Rust 账号、价格、命令及 AI 路由网关定向测试全部通过。
- [ ] 原子新增失败无残留，原子编辑失败保持旧值，价格恢复继续继承官方值。
- [ ] fixture secret 只出现在专用详情返回和页面受控草稿断言中，不出现在列表、Bootstrap、事件、日志、错误或测试失败输出中。
- [ ] API Key 与 OAuth 卡片、详情、筛选恢复、脏状态、错误聚焦和两处永久删除流程均有通过的自动化覆盖。
- [ ] 未新增 schema migration、URL 路由、OAuth 授权流程或 AI Environments 数据耦合。

## 验证步骤
- [ ] 运行 `npm run test`。
- [ ] 运行 `npm run lint`。
- [ ] 运行 `npm run build`。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway`。
- [ ] 若共享 SQLite 或 crate 契约受影响，运行对应 shared_sqlite 或完整 `cargo test --manifest-path src-tauri/Cargo.toml`。
- [ ] 检查测试和命令输出，确认没有 fixture secret 或敏感草稿回显。

## 范围外事项
不进行与本规格无关的重构、依赖升级、数据库迁移、发布自动化或 OAuth 功能扩展。
