# 04 - 网关密钥重构跨层回归与集成验收

- plan_id: `ai-routing-gateway-key-refactor`
- plan_digest: `5786f22264c847e5c50d7f8fc897e89089f443e2d1dcbadb29b911105e9bf55a`
- preview_revision: `2`
- task_id: `gateway-key-cross-layer-regression`
- order: `04`
- blocked_by: `gateway-key-domain-workflows, gateway-key-provider-conversion, gateway-key-management-ui`
- source_plan: `../plan.md`
- source_plan_digest: `5786f22264c847e5c50d7f8fc897e89089f443e2d1dcbadb29b911105e9bf55a`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/shared_sqlite/mod.rs`
  - `src-tauri/src/shared_sqlite/migrations.rs`
  - `src-tauri/src/ai_routing_gateway/gateway_key.rs`
  - `src-tauri/src/ai_routing_gateway/key_display_group.rs`
  - `src-tauri/src/ai_routing_gateway/key_conversion.rs`
  - `src-tauri/src/ai_routing_gateway/commands/mod.rs`
  - `src-tauri/src/ai_routing_gateway/tests.rs`
  - `src-tauri/src/app_store/service_provider_commands.rs`
  - `src-tauri/src/app_store/tests.rs`
  - `src-tauri/src/app_store/tests/`
  - `src/lib/aiRoutingGateway.test.ts`
  - `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`
  - `src/i18n.test.ts`

## 预期结果

补齐迁移、Rust 领域及跨存储协调、IPC wrapper 和 React 的跨层回归，在真实迁移后的临时配置中验证分组建密钥、一次性复制、编辑及状态操作、组合筛选、多工具转换、服务商删除解除关系和同工具重转。运行仓库既有 cargo、前端 test、lint 和 typecheck 命令，确认 SQLite 关系、服务商文件、active 集合、页面状态、失败恢复及无明文泄露保持一致。

## 范围与依赖

- 依赖前三个任务全部完成；本任务以计划内完整用户流程做回归和集成验收，不新增产品能力或扩大支持工具集合。
- 写入范围仅用于补齐或修正计划所列测试、fixture 及被回归直接暴露的同职责实现问题；`src-tauri/src/app_store/tests/` 仅用于文件名无法预先确定的 app-store 集成测试或 fixture。
- 使用真实 V5 migration 后的临时 SQLite 与隔离服务商配置目录，不读写用户实际配置、密钥库或 active 文件。
- 验证命令以仓库现有 `Cargo.toml` 和 `package.json` scripts 为准，不引入新的测试框架或发布流程。

## 实施清单

- [ ] 建立从 V4 快照和空库启动的临时测试配置，运行真实 migration，并断言默认展示组、存量归属、外键、唯一索引和转换关系 schema。
- [ ] 覆盖完整密钥流程：在展示分组中创建密钥、仅经一次性路径复制、编辑允许字段且密钥材料不变、启停/到期/撤销/软删除状态正确、组内文本与状态筛选 AND 组合。
- [ ] 覆盖今日及近 30 个本地自然日用量聚合，验证 token、完整费用和任一缺失/不可计算记录令对应窗口费用为 null。
- [ ] 覆盖四工具批量转换、默认非激活、前三工具 active 替换、OpenCode active 追加、已转换防重及并发唯一冲突。
- [ ] 在每个跨存储阶段注入失败，确认 SQLite transaction 回滚、服务商/active 快照恢复、投影一致且错误稳定；覆盖 RootKey 缺失和解密失败无新增服务商。
- [ ] 覆盖服务商删除成功解除关系并允许同工具重转，以及解除失败恢复服务商、active 和关系可见状态。
- [ ] 覆盖 Tauri command 名称、camelCase invoke 参数、DTO 脱敏、稳定错误透传及 TypeScript wrapper 不提交可信派生字段。
- [ ] 覆盖 React 从分组加载到表格、创建/编辑/一次性结果、组合筛选、转换、busy 防重、失败重试和操作后刷新的一致流程。
- [ ] 执行完整 Rust、前端 test、lint、typecheck/build，并记录任何环境限制；只修复由本计划变更引起且落在本任务 write scope 内的问题。
- [ ] 检查 SQLite、服务商状态文件、active 集合、页面状态、测试日志和快照，确认成功路径一致、失败路径恢复且无明文泄露。

## 验收标准

- [ ] V4 和空库均可升级并重复启动，foreign key check 通过，默认组唯一、存量归属完整且重复转换受约束。
- [ ] 完整密钥生命周期保留既有认证、加密和授权行为；编辑不改密钥材料，软删除立即失效，状态边界和费用 null 语义正确。
- [ ] 多工具转换和服务商删除/解除在 SQLite、服务商文件、active 集合与页面中没有半完成状态，删除后同工具能够重转。
- [ ] IPC 与 wrapper 的命令、camelCase 字段和错误契约一致，客户端不提交服务端派生字段，所有非一次性响应均为脱敏数据。
- [ ] React 工作流在桌面和窄宽布局下完成分组、筛选、操作和转换，busy、防重、错误恢复与刷新行为符合计划。
- [ ] `cargo test`、前端 test、lint 和 typecheck/build 全部通过；测试、日志、错误、DTO、快照和持久化状态中不存在完整网关密钥。

## 验证步骤

- [ ] 在 `src-tauri` 运行 `cargo test`，确认 migration、领域、app-store、IPC 和集成测试全部通过。
- [ ] 在仓库根运行 `npm test -- --runInBand`；若 Vitest 不接受该兼容参数，则按 `package.json` 运行等效的 `npm test`，确认 wrapper、React 和 i18n 测试通过。
- [ ] 运行 `npm run lint` 与 `npm run build`，以 ESLint 和 `tsc -b` 完成 lint/typecheck 验证并确认 Vite 构建成功。
- [ ] 在隔离临时配置中执行创建到多工具转换、删除一个转换服务商、再转换同一工具的集成场景，逐步核对 SQLite 关系、服务商文件、active 集合和页面状态。
- [ ] 对服务商保存、active 更新、关系写入和删除解除分别注入失败，确认每次失败后的持久状态与操作前快照一致。
- [ ] 扫描测试日志、错误输出、序列化 fixture 和前端快照中的测试明文，确认仅一次性响应断言可在受控局部持有且不会被快照持久化。

## 安全与回滚注意事项

- 所有集成测试必须使用临时 HOME、临时 SQLite 和隔离 app-store 路径，测试结束清理临时敏感数据，不接触开发者真实服务商配置。
- 故障注入后同时验证 SQLite rollback 和服务商/active 补偿；仅检查单一存储成功不足以判定回滚有效。
- 明文断言使用短生命周期测试变量并避免打印；失败消息、snapshot、fixture 和 CI artifact 不得包含完整密钥或 RootKey。
- 若完整回归发现需要修改当前 write scope 外的路径，应停止并修订任务范围，不得以跳过测试或放宽断言完成验收。

## 范围外事项

- 不新增工具、认证协议、服务商数据模型、自动转换、发布迁移工具或与本计划无关的重构。
