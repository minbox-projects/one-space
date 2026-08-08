# AI 路由网关密钥重构实施计划

## 计划元数据

- plan-id: `ai-routing-gateway-key-refactor`
- status: `ready-for-implementation`
- source_spec: `.ai-work-flow/plans/ai-routing-gateway-key-refactor/spec.md`
- source_spec_digest: `40d8109b76bfc9cd97b0c0a5a2b9dfe1e451c3ed39de6f36148c11dfaeba42b7`
- task_mode: `split`
- mode_selection: `selected=split, confirmed_by=user, user_response=split`

## 技术与代码上下文

- 网关 SQLite schema 位于 `src-tauri/src/shared_sqlite/migrations/`；当前基础表由 `V1__ai_routing_gateway.sql` 创建，密钥加密列由 `V4__encrypt_gateway_keys.sql` 扩展，迁移由 `src-tauri/src/shared_sqlite/mod.rs` 顺序执行。
- 网关密钥领域逻辑在 `src-tauri/src/ai_routing_gateway/gateway_key.rs`；现有 `create`、`regenerate`、`set_enabled`、`revoke`、`copy_plaintext` 已使用 `RootKey` 和 SQLite transaction。`commands/mod.rs` 的 `keys`、`key_usage` 和 `GatewayKeyDto` 是现有列表及统计入口。
- 网关 IPC 在 `src-tauri/src/ai_routing_gateway/commands/mod.rs`，前端 typed wrapper 在 `src/lib/aiRoutingGateway.ts`，运行时命令注册在 `src-tauri/src/app_runtime/run_app.rs`，页面在 `src/components/AiRoutingGateway/index.tsx`。
- AI 终端服务商是独立 JSON 状态域：`src-tauri/src/app_store/types/service_provider_types.rs` 定义 `ServiceProviderRecord`，`src-tauri/src/app_store/service_provider_commands.rs` 负责保存和 active 规则。转换关系的权威索引放 SQLite；服务商状态写入必须通过 app store 内部 API，并用共享操作锁与补偿恢复实现跨存储的一致结果。
- 现有 `GatewayGroup` 是路由账号分组，不能复用为密钥展示分组；新 DTO、表和命令均须使用明确的 key-display-group 命名，避免与授权用 `groupIds` 混淆。

## 实施方案

1. 新建后续版本 SQLite migration，先建 `ai_gateway_key_display_groups`（稳定 ID、唯一名称、`is_default`、时间戳），以局部唯一索引保证唯一默认组；插入默认组后向 `ai_gateway_api_keys` 增加非空 `display_group_id` 并回填。SQLite 需要重建表时，按复制数据、保留原 FK/索引、替换表的迁移顺序执行；迁移结束启用并验证 foreign keys。
2. 在同一 migration 建立转换关系表，字段为 `gateway_key_id`、`tool`、`service_provider_id`、时间戳；`gateway_key_id` 外键随物理密钥删除级联，`service_provider_id` 仅做逻辑引用，建立 `(gateway_key_id, tool)` 唯一约束和服务商 ID 索引。密钥软删除不删除关系，但转换读写必须拒绝该密钥；服务商删除路径在成功持久化删除后、同一受控操作中解除对应关系，解除失败时恢复服务商快照及 active 状态。
3. 将密钥展示分组、列表筛选/状态、编辑、软删除和转换封装在专用 Rust 模块，命令层只负责输入 DTO、RootKey 获取和错误码映射。所有影响授权、归属、服务商记录或 active 状态的多写操作均在 SQLite transaction 与 app-store 状态快照保护下执行；任一阶段失败回滚 SQLite，恢复服务商文件和 active 集合，并返回稳定错误码。
4. 扩展 IPC DTO 和 TypeScript wrapper，以显式输入输出维持 camelCase 前端契约。列表请求携带展示 `groupId`、文本、状态、分页和排序；服务端构造脱敏密钥、状态与费用可计算语义，客户端不推导安全或业务真值。
5. 将 KeysTab 改造为 SSH 隧道式展示分组 tabs、分组管理弹框、表格和密钥/转换弹框。复用现有 Dialog、Lucide、错误和 busy 状态模式；所有命令后重新读取列表和可转换工具集，禁止提交中重复操作。

## 顺序执行步骤

1. **迁移与基础数据契约**（无前置依赖）：新增 `src-tauri/src/shared_sqlite/migrations/V5__gateway_key_display_groups_and_conversions.sql`，更新 `src-tauri/src/shared_sqlite/mod.rs` 的迁移测试。创建唯一默认展示分组，幂等回填存量密钥，建立非空单归属 FK、转换表、唯一索引及必要索引。验收：在临时旧库和空库启动迁移，运行 `cargo test shared_sqlite`，检查 `PRAGMA foreign_key_check`、默认组唯一性、存量行均有默认组及重复工具转换被数据库拒绝。
2. **密钥领域与跨域转换协调**（依赖步骤 1）：扩展 `src-tauri/src/ai_routing_gateway/gateway_key.rs`，新增展示分组模块和转换协调模块；必要时在 `src-tauri/src/app_store/` 暴露不经 Tauri command 的受锁定服务商状态读写、快照恢复和删除钩子。实现分组 CRUD、删除自定义组原子迁移、密钥 create/update/set-enabled/soft-delete/list、状态计算、统计复用和转换。验收：`cargo test ai_routing_gateway app_store` 覆盖事务、状态、解密边界、服务商删除解除关系及重转。
3. **IPC、注册与前端类型**（依赖步骤 2）：在 `src-tauri/src/ai_routing_gateway/commands/mod.rs` 增加请求/响应 DTO 与命令，在 `src-tauri/src/app_runtime/run_app.rs` 显式注册；在 `src/lib/aiRoutingGateway.ts` 添加对应联合类型、接口和 typed calls，并更新其单元测试。验收：编译期核对命令名称、序列化字段和 invoke 参数，运行 `cargo test` 与前端 wrapper 测试。
4. **密钥页面工作流**（依赖步骤 3）：在 `src/components/AiRoutingGateway/index.tsx` 及按需新增同目录组件中实现展示分组 tabs/管理弹框、创建编辑弹框、一次性密钥结果、筛选表格、图标操作与转换弹框；补齐相关 i18n 资源文件中的可见文案。验收：运行前端测试，人工/浏览器检查窄宽与桌面布局、严格列顺序、tooltip、禁用和加载/错误/空态。
5. **端到端回归**（依赖步骤 1-4）：扩展 Rust、wrapper 和 React 测试，在真实 migration 后的临时配置中验证服务商状态协调。验收：执行仓库既有 `cargo test`、前端 test/lint 命令（以 `package.json` scripts 为准），并完成创建到转换、删除服务商到重转的集成场景。

## 任务边界与依赖

1. order: `1`；task_id: `gateway-key-domain-workflows`；标题：网关密钥数据迁移与领域工作流；概要：新增并注册 V5 SQLite 迁移，建立唯一默认展示分组、存量密钥回填、非空单归属外键、转换关系及唯一索引；实现展示分组 CRUD、默认组保护、删除分组时原子回迁，以及密钥创建、编辑、启停、软删除、筛选、状态和今日及近 30 日用量聚合。迁移与领域测试验证旧库和空库升级、外键及唯一约束、事务回滚、RootKey 和一次性明文边界、状态优先级、组合筛选、费用不可计算语义及编辑不改密钥材料。
2. order: `2`；task_id: `gateway-key-provider-conversion`；标题：服务商转换协调与 typed IPC 契约；概要：实现四种工具的可转换查询和原子批量转换，由后端生成服务商字段并协调 SQLite 关系、服务商记录及 active 状态；提供受共享锁和快照恢复保护的 app-store 内部接口，将服务商删除接入关系解除和失败补偿。同步新增 Tauri DTO、命令及运行时注册，并扩展 TypeScript typed wrapper；测试防重并发、激活替换或追加、中途失败回滚、删除后重转、camelCase 参数、稳定错误透传、脱敏和一次性明文边界，以及客户端不提交可信派生字段。
3. order: `3`；task_id: `gateway-key-management-ui`；标题：网关密钥分组表格与弹框工作流；概要：将 KeysTab 改造为 SSH 式展示分组 tabs 和管理弹框，实现创建编辑、一次性密钥结果、严格列顺序表格、文本与状态组合筛选、带 tooltip 的图标操作及多工具转换弹框，并补齐本地化文案。前端测试和界面检查验证脱敏及费用不可计算显示、窄宽布局、加载错误空态、已转换禁选、统一激活默认关闭、busy 防重、错误重试和操作后分组、列表及可转换工具刷新。
4. order: `4`；task_id: `gateway-key-cross-layer-regression`；标题：网关密钥重构跨层回归与集成验收；概要：补齐迁移、Rust 领域及跨存储协调、IPC wrapper 和 React 的跨层回归，在真实迁移后的临时配置中验证分组建密钥、一次性复制、编辑及状态操作、组合筛选、多工具转换、服务商删除解除关系和同工具重转。运行仓库既有 cargo、前端 test、lint 和 typecheck 命令，确认 SQLite 关系、服务商文件、active 集合、页面状态、失败恢复及无明文泄露保持一致。

## 具体改动

- `src-tauri/src/shared_sqlite/migrations/V5__gateway_key_display_groups_and_conversions.sql`：新增展示分组和转换关系 schema；默认组、回填、FK、唯一性、索引和升级顺序写入单一迁移。
- `src-tauri/src/shared_sqlite/mod.rs` 与其测试：识别新 migration，并在旧 schema 升级、空库初始化和重复启动下验证版本、数据和约束。
- `src-tauri/src/ai_routing_gateway/gateway_key.rs`：将创建输入扩展为单个 `display_group_id`；编辑仅改名称、展示组、过期时间和授权，不触碰密钥材料；启停/软删除对 revoked/expired 状态作后端校验；RootKey 仅传入 `copy_plaintext` 与转换内部解密。
- `src-tauri/src/ai_routing_gateway/key_display_group.rs`（新增）：默认组查询、创建、重命名、删除及密钥回迁的 SQLite transaction 实现，拒绝默认组改删与非法/不存在分组。
- `src-tauri/src/ai_routing_gateway/key_conversion.rs`（新增）：固定 `claude`、`codex`、`gemini`、`opencode` 枚举；读取可转换工具；验证可用密钥；生成系统 base URL、工具专属字段、冲突安全名称/标识；协调服务商状态和关系写入；处理 active 的替换或追加及失败恢复。
- `src-tauri/src/ai_routing_gateway/commands/mod.rs`：新增展示分组、密钥编辑/删除/查询、可转换工具和批量转换命令；将 bootstrap 的 keys 调整为当前展示组可查询数据或单独列表命令；DTO 仅返回脱敏值、分组、计算状态和一次性明文响应。
- `src-tauri/src/app_store/service_provider_commands.rs`、`src-tauri/src/app_store/mod.rs` 及必要的 app-store 内部模块：提供转换协调所需的内部受锁保存/删除/恢复入口；将 public `service_providers_delete` 接入关系解除协调，避免孤立逻辑引用。
- `src-tauri/src/app_runtime/run_app.rs`：将新增网关命令加入 invoke handler，不以动态或隐式注册替代。
- `src/lib/aiRoutingGateway.ts` 与 `src/lib/aiRoutingGateway.test.ts`：定义 `GatewayKeyDisplayGroup`、`GatewayKeyStatus`、分页列表、编辑输入、转换工具/结果等类型和 wrappers；移除不再符合编辑契约的仅更新授权调用方式。
- `src/components/AiRoutingGateway/index.tsx`、`src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` 及相关 locale 文件：以可复用局部组件替换内嵌创建卡片和卡片列表，保留一次性复制、重新生成和授权选择；引入表格和三个弹框，测试其状态与调用参数。

## 接口与数据流

1. 页面加载展示分组，再以当前 `groupId`、文本、状态、分页和排序调用密钥列表；后端按同一后端时钟计算 `expires_at <= now`，默认排除 `revoked_at IS NOT NULL`，并在 SQL/领域层限制当前展示组。
2. 列表项字段包括 `id`、`name`、`maskedKey`、`displayGroup`、`status`、`expiresAt`、`createdAt`、授权摘要、`today` 和 `last30Days`。两个用量窗口均使用现有请求日志聚合；任一窗口所含记录的 `cost_calculable=false` 或费用缺失时，返回 `estimatedCostUsd=null` 与显式 `costCalculable=false`，UI 显示“不可计算”。
3. 创建输入含名称、`displayGroupId`、过期时间、路由账号 `groupIds` 和 `modelIds`；成功只在 `OneTimeGatewayKey` 中返回 `plaintext`。编辑输入不含 key material；复制命令和转换命令在后端按需解密，列表、搜索、日志和前端持久状态均不得保存明文。
4. 转换前页面请求可转换工具集；提交仅含 `keyId`、合法工具数组和 `activate`。服务端重新验证所有工具、密钥状态和关系唯一性，再生成 base URL、服务商 ID/Claude code 等值，不能信任前端派生字段。成功返回新关系/服务商摘要和最新不可选工具集合。
5. 删除服务商经 app-store 删除入口查询其转换关系；在共享锁内保存删除前服务商及 active 快照、删除服务商、解除关系。失败恢复所有已变更状态；成功后工具再次出现在网关密钥的可转换集合。

## 失败处理

- 迁移先创建并验证默认组，随后回填，最后施加非空 FK/索引；任一步 SQLite 失败使整次 migration 回滚，不留下部分回填。
- 分组名称为空、重复、默认组改删、未知组、无效过期时间、空授权或空工具数组返回稳定 `invalid_input`；删除自定义组只在默认组存在且全部密钥更新成功后删除组。
- 启用、复制、重新生成和转换均显式拒绝不存在、软删除、撤销或到期密钥；到期判断以 `expires_at <= now` 为边界，不能由 UI 绕过。
- 转换在开始写服务商前检查所有工具的关系；数据库唯一冲突映射为稳定“该工具已转换”错误，并提示客户端刷新。创建服务商、激活调整、关系写入或持久化失败时，恢复服务商/active 快照并 rollback SQLite transaction，结果不得部分成功。
- 服务商删除后关系解除失败时恢复服务商记录、active 集合和投影可见状态；仅在两端均成功后返回删除成功。
- 明文只能停留在 Rust 局部变量、一次性 command 返回和剪贴板调用期间；错误、debug 输出、DTO、搜索条件持久化和审计数据禁止包含它。

## 测试与验证

- Rust migration 测试：空库、含存量 `ai_gateway_api_keys` 的 V4 库、重复打开后的默认组唯一性、外键约束、回填、`(gateway_key_id, tool)` 唯一约束及无孤立关系。
- Rust 领域测试：展示分组 CRUD 与默认组保护；删除自定义组原子回迁；创建/编辑不改 key material；软删除立即认证失败且默认列表不含该项；`expires_at == now` 归为已过期且优先于 enabled；文本对名称、masked、prefix、suffix 和状态筛选在组内以 AND 工作。
- 用量测试：今日和近 30 个本地自然日边界、token 聚合、完整费用、任一 null/不可计算记录使对应费用为 null，而不是零或部分和。
- 转换测试：四工具多选、默认非激活、Claude/Codex/Gemini 替换 active、OpenCode 追加 active、已转换工具读取和后端拒绝、并发/重复唯一冲突、多工具任意中途失败的完全回滚、RootKey 缺失/解密失败无新增服务商、服务商删除后关系解除和同工具重转。
- IPC/wrapper 测试：每个命令的命名、camelCase 入参、DTO 类型、错误透传和客户端不传 base URL/tool 派生值。
- React 测试：SSH 式 tabs 与管理弹框、创建/编辑弹框、表格列严格为名称/API 密钥/分组/用量/过期时间/状态/创建时间/操作、组合筛选、带 title/aria-label 的图标操作、转换多选与统一激活、已转换禁选、busy 防重和错误后可重试。
- 集成验证：通过 UI 创建密钥并仅复制一次明文，创建多工具服务商，删除其中一个服务商后重新转换；检查服务商文件、SQLite 关系、active 集合和页面状态一致。运行 `cargo test`、`npm test -- --runInBand`（或 `package.json` 中等效 test script）及项目 lint/typecheck scripts。

## 验收标准

- SQLite 升级后恰有一个不可改删的默认密钥展示分组，所有存量密钥归属该组，每个新密钥只能归属一个展示分组；删除自定义组原子迁移其密钥。
- 创建、编辑、重新生成、复制、启停和软删除保留既有认证、加密和授权能力；编辑不改密钥值，完整密钥只经显式一次性路径返回或后端转换解密。
- 当前组列表为严格指定列顺序的 table，仅展示脱敏密钥；文本与状态筛选 AND 组合，状态边界、默认排除软删除及费用不可计算显示正确。
- 转换弹框支持四工具多选和默认关闭的统一激活；已转换工具前端禁选且后端防重；每种工具生成独立服务商，系统而非前端确定 base URL 和工具字段。
- 批量转换及服务商删除/关系解除无半完成状态；前三工具的 active 替换和 OpenCode active 追加符合现有数据模型，删除后可以重转。
- 新命令已注册、前端 wrapper 保持 typed API，后端/前端/集成测试覆盖迁移、回滚、约束、状态、费用 null 和安全边界并通过。

## 兼容、迁移与发布

- 不改网关认证协议、RootKey 加密格式、历史请求日志结构、现有路由账号分组授权、模型授权和既有 AI 终端每工具服务商数据模型；新展示分组与授权分组同屏时使用不同命名和字段。
- migration 必须支持已有 SQLite 数据库，采用默认组的稳定标识和唯一约束避免重复回填。发布前在 V4 快照上演练升级与启动；升级失败按应用既有 migration 回滚语义阻止继续运行，而非以未约束 schema 启动。
- 转换关系不反向改写现有未转换服务商；仅通过受控新增/删除钩子关联网关来源。升级后不批量自动创建服务商或改变任何现有 active。
- 发布验收包含旧库升级、服务商状态文件已有数据、无 RootKey、服务商保存失败和并发重复转换场景；通过后再合并 schema、Rust、IPC 和 UI 改动。
