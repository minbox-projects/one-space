# 01 - 网关密钥数据迁移与领域工作流

- plan_id: `ai-routing-gateway-key-refactor`
- plan_digest: `5786f22264c847e5c50d7f8fc897e89089f443e2d1dcbadb29b911105e9bf55a`
- preview_revision: `2`
- task_id: `gateway-key-domain-workflows`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `5786f22264c847e5c50d7f8fc897e89089f443e2d1dcbadb29b911105e9bf55a`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/shared_sqlite/migrations/V5__gateway_key_display_groups_and_conversions.sql`
  - `src-tauri/src/shared_sqlite/migrations.rs`
  - `src-tauri/src/shared_sqlite/mod.rs`
  - `src-tauri/src/ai_routing_gateway/gateway_key.rs`
  - `src-tauri/src/ai_routing_gateway/key_display_group.rs`
  - `src-tauri/src/ai_routing_gateway/mod.rs`
  - `src-tauri/src/ai_routing_gateway/tests.rs`

## 预期结果

新增并注册 V5 SQLite 迁移，建立唯一默认展示分组、存量密钥回填、非空单归属外键、转换关系及唯一索引；实现展示分组 CRUD、默认组保护、删除分组时原子回迁，以及密钥创建、编辑、启停、软删除、筛选、状态和今日及近 30 日用量聚合。迁移与领域测试验证旧库和空库升级、外键及唯一约束、事务回滚、RootKey 和一次性明文边界、状态优先级、组合筛选、费用不可计算语义及编辑不改密钥材料。

## 范围与依赖

- 本任务只建立 SQLite 数据契约和网关密钥领域工作流，不新增 Tauri command、TypeScript wrapper 或页面 UI。
- 新展示分组必须与既有路由账号授权分组保持独立命名和语义；不得改变认证协议、RootKey 加密格式、请求日志结构或既有授权模型。
- 转换关系表在本任务创建；跨 SQLite 与服务商 JSON 状态的协调由 `gateway-key-provider-conversion` 实现。
- 无前置任务；完成后为 `gateway-key-provider-conversion` 提供 schema、分组和密钥领域能力。

## 实施清单

- [ ] 新增并注册 V5 migration：创建 `ai_gateway_key_display_groups`，写入稳定默认组，以局部唯一索引保证恰有一个默认组；按 SQLite 兼容顺序重建或扩展 `ai_gateway_api_keys`，回填存量密钥并施加非空单归属外键，保留原有外键和索引。
- [ ] 在同一 migration 创建转换关系表，包含 `gateway_key_id`、固定工具值、逻辑引用的 `service_provider_id` 和时间戳；配置密钥物理删除级联、`(gateway_key_id, tool)` 唯一约束及服务商 ID 索引，并在迁移结束验证 foreign keys。
- [ ] 实现展示分组查询、创建、重命名和删除；拒绝空名、重名、默认组改删和未知组，删除自定义组时在同一 SQLite transaction 内将所属密钥迁回默认组后再删除。
- [ ] 扩展密钥创建和编辑契约为单个 `display_group_id`；编辑仅允许名称、展示组、过期时间及路由账号/模型授权变化，不重新生成或改写密钥材料。
- [ ] 实现启停、软删除和后端状态校验；软删除后立即拒绝认证并默认不出现在列表中，撤销或到期密钥不得被重新启用、复制或重新生成。
- [ ] 实现按当前展示组限制的分页、排序、文本与状态组合筛选；文本覆盖名称、masked、prefix 和 suffix，状态按后端统一时钟计算，`expires_at <= now` 为到期边界且优先于 enabled。
- [ ] 复用请求日志聚合今日及近 30 个本地自然日的 token 和费用；任一窗口含费用缺失或 `cost_calculable=false` 记录时，该窗口返回 `estimatedCostUsd=null` 和 `costCalculable=false`。
- [ ] 补齐 migration 与领域测试，覆盖空库、V4 存量库、重复打开、约束、事务回滚、筛选、状态、用量和密钥材料不变。

## 验收标准

- [ ] 空库及 V4 存量库升级后恰有一个不可改删的默认展示分组，全部存量密钥归属默认组，所有密钥只能归属一个存在的展示分组。
- [ ] `PRAGMA foreign_key_check` 无结果；未知展示组、重复默认组及同一密钥同一工具的重复转换关系均被数据库或领域层拒绝。
- [ ] 删除自定义组要么完整迁回其密钥并删除组，要么不产生任何变更；默认组改名或删除稳定返回 `invalid_input`。
- [ ] 创建只经一次性结果暴露明文；编辑不改变密钥值；列表、日志、错误和持久状态均不包含明文。
- [ ] 软删除、撤销、到期和 enabled 的状态优先级符合计划，组、文本、状态筛选以 AND 组合且不越过当前展示组。
- [ ] 今日及近 30 日窗口边界和 token 聚合正确，费用不可计算时返回 null 而非零或部分和。

## 验证步骤

- [ ] 在 `src-tauri` 运行 `cargo test shared_sqlite`，确认空库、V4 升级、重复启动、回填、外键及唯一约束测试通过。
- [ ] 在 `src-tauri` 运行 `cargo test ai_routing_gateway`，确认展示分组、密钥生命周期、筛选、状态、事务、RootKey 和用量测试通过。
- [ ] 对迁移后的临时库执行 `PRAGMA foreign_key_check`，确认结果为空，并查询默认组数量、未归属密钥数量及重复转换键数量均符合约束。
- [ ] 比较编辑前后的加密密钥列和 prefix/suffix，确认允许字段更新不改写密钥材料；检查测试输出和错误内容不含测试明文。

## 安全与回滚注意事项

- migration 必须由既有迁移事务完整提交或完整回滚；默认组创建、回填、表替换、索引和外键验证不得留下中间 schema。
- RootKey 仅进入显式复制及后续转换所需的后端解密路径；领域列表、搜索条件、审计和 debug 输出不得接触明文。
- 升级失败沿用既有 migration 失败语义阻止应用继续启动，不允许降级为无外键或可空归属的 schema。
- 回滚实现改动时不得反向改写已迁移用户数据或删除转换关系；发布前以 V4 快照演练恢复策略。

## 范围外事项

- 服务商记录生成、active 集合协调、服务商删除补偿、IPC 注册、前端 typed wrapper 和 React 页面不在本任务内。
