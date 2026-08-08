# 02 - 服务商转换协调与 typed IPC 契约

- plan_id: `ai-routing-gateway-key-refactor`
- plan_digest: `5b4fb2aa88f441a06603231eb7bda9c0aae2158accd3764d818593e13a25fc22`
- preview_revision: `2`
- task_id: `gateway-key-provider-conversion`
- order: `02`
- blocked_by: `gateway-key-domain-workflows`
- source_plan: `../plan.md`
- source_plan_digest: `5b4fb2aa88f441a06603231eb7bda9c0aae2158accd3764d818593e13a25fc22`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/ai_routing_gateway/key_conversion.rs`
  - `src-tauri/src/ai_routing_gateway/mod.rs`
  - `src-tauri/src/ai_routing_gateway/commands/mod.rs`
  - `src-tauri/src/ai_routing_gateway/tests.rs`
  - `src-tauri/src/app_store.rs`
  - `src-tauri/src/app_store/service_provider_commands.rs`
  - `src-tauri/src/app_store/`
  - `src-tauri/src/app_runtime/run_app.rs`
  - `src/lib/aiRoutingGateway.ts`
  - `src/lib/aiRoutingGateway.test.ts`

## 预期结果

实现四种工具的可转换查询和原子批量转换，由后端生成服务商字段并协调 SQLite 关系、服务商记录及 active 状态；提供受共享锁和快照恢复保护的 app-store 内部接口，将服务商删除接入关系解除和失败补偿。同步新增 Tauri DTO、命令及运行时注册，并扩展 TypeScript typed wrapper；测试防重并发、激活替换或追加、中途失败回滚、删除后重转、camelCase 参数、稳定错误透传、脱敏和一次性明文边界，以及客户端不提交可信派生字段。

## 范围与依赖

- 依赖 `gateway-key-domain-workflows` 提供 V5 转换关系表、展示分组与密钥状态能力。
- 本任务负责 Rust 跨存储协调、app-store 内部接口、Tauri command 注册及 TypeScript typed wrapper，不实现 React 页面。
- `src-tauri/src/app_store/` 仅允许新增或修改转换协调所必需、文件名尚无法预先确定的内部锁、快照或恢复模块；不得借此重构无关 app-store 功能。
- 固定工具集合仅为 `claude`、`codex`、`gemini`、`opencode`，不扩展新的工具或服务商数据模型。

## 实施清单

- [ ] 实现四工具枚举和可转换查询：拒绝不存在、软删除、撤销或 `expires_at <= now` 的密钥，读取已有关系并返回稳定的可选/不可选集合。
- [ ] 实现原子批量转换，提交仅接受 `keyId`、合法工具数组和 `activate`；后端重新验证全部工具及关系唯一性，生成系统 base URL、冲突安全名称/标识、Claude code 和各工具专属字段。
- [ ] 仅在转换内部通过 RootKey 解密密钥；每种工具生成独立服务商，转换结果只返回关系、服务商摘要和最新不可转换集合，不返回明文或服务商敏感字段。
- [ ] 在 app-store 提供不经 Tauri command 的受共享操作锁内部接口，支持服务商/active 快照、保存、删除和完整恢复；SQLite transaction 与服务商 JSON 写入由同一协调流程控制。
- [ ] 实现 active 规则：Claude、Codex、Gemini 在请求激活时替换 active，OpenCode 在请求激活时追加 active；`activate` 默认 false 时不改变现有 active。
- [ ] 将 public 服务商删除接入转换关系解除：共享锁内保留删除前服务商及 active 快照，服务商持久化删除成功后解除关系；任一步失败恢复服务商、active 和可见投影。
- [ ] 将唯一冲突、非法输入、RootKey 缺失/解密失败和持久化失败映射为稳定错误码；并发或重复转换不得产生重复服务商或半完成关系。
- [ ] 新增展示分组、密钥编辑/删除/查询、可转换工具和批量转换的显式 DTO 与 Tauri commands，并在运行时 invoke handler 中逐项注册。
- [ ] 扩展 TypeScript 类型与 wrappers，保持 camelCase 调用参数和 DTO 联合类型；客户端接口不得接收或提交 base URL、服务商 ID、工具专属配置等可信派生字段。
- [ ] 补齐 Rust 和 wrapper 测试，覆盖四工具多选、默认非激活、激活规则、防重并发、中途失败、删除后重转、序列化参数、错误透传和脱敏边界。

## 验收标准

- [ ] 可转换查询和提交都由后端验证密钥状态及已有关系；前端传入重复、未知或已转换工具时返回稳定错误且不产生写入。
- [ ] 多工具转换全部成功时，每种工具各有一个关系和服务商记录；任意阶段失败时 SQLite、服务商文件、active 集合和投影恢复到调用前状态。
- [ ] Claude、Codex、Gemini 的 active 替换及 OpenCode 的 active 追加符合既有数据模型，默认关闭激活时不改变任何 active。
- [ ] 服务商删除与关系解除共同成功后同工具可再次转换；解除失败时被删服务商及 active 状态完整恢复。
- [ ] 所有新增命令已显式注册，Rust/TypeScript 字段保持 camelCase 契约，稳定错误可由 wrapper 原样传递。
- [ ] 列表、DTO、错误、日志及测试快照不含明文；完整密钥仅存在于转换函数局部解密期间和既有一次性返回路径。

## 验证步骤

- [ ] 在 `src-tauri` 运行 `cargo test ai_routing_gateway app_store`，确认转换、并发唯一冲突、快照恢复、删除解除和 RootKey 失败测试通过。
- [ ] 在 `src-tauri` 运行 `cargo test`，确认 Tauri DTO、命令注册和跨模块编译通过。
- [ ] 运行 `npm test -- --run src/lib/aiRoutingGateway.test.ts`，确认命令名称、camelCase 参数、typed DTO、错误透传和非可信派生字段测试通过。
- [ ] 使用故障注入分别中断服务商保存、active 更新、关系写入和关系解除，逐次比较调用前后的 SQLite 关系、服务商文件及 active 集合完全一致。
- [ ] 检查测试输出、错误对象和序列化 DTO，确认不出现完整网关密钥。

## 安全与回滚注意事项

- 跨 SQLite 与 JSON 状态域没有单一数据库事务，必须以共享锁、调用前快照、SQLite transaction 和反向补偿共同保证外部可见原子性。
- 补偿失败不得伪报成功；应返回稳定错误并保留足够的非敏感诊断信息，同时禁止记录明文、RootKey 或完整服务商密钥。
- 关系中的 `service_provider_id` 是逻辑引用；不得给既有未转换服务商补写关系，也不得在升级时自动创建服务商或改变 active。
- 回滚本功能时须先停止新增转换，再按关系索引确认服务商状态；不得直接删除关系以掩盖未恢复的服务商文件状态。

## 范围外事项

- 展示分组表格、转换弹框、页面刷新策略和本地化可见文案由 `gateway-key-management-ui` 实现。
