# AI 路由网关账号池独立管理实施计划

## 计划元数据

- plan-id: `20260806-account-pool-dedicated-management`
- status: `ready-for-implementation`
- source_spec: `.ai-work-flow/plans/20260806-account-pool-dedicated-management/spec.md`
- source_spec_digest: `cf8b377d0c732ded1c2fdbb7d0fe641289a9f265d98bf702115d1d0a7eed82cd`
- task_mode: `split`

## 技术与代码上下文

- 前端入口集中在 `src/components/AiRoutingGateway/index.tsx`：现有 `AccountsTab` 在列表内展开 `AccountDetail`，新增 API Key 调用旧 `ai_routing_gateway_account_create_api_key`，详情编辑分别调用 `account_update`、`mapping_save`、`price_save`。
- 前端 typed IPC facade 位于 `src/lib/aiRoutingGateway.ts`，测试位于同目录 `aiRoutingGateway.test.ts`；Tauri mock 与组件测试已使用 `src/test/mocks/tauri.ts` 和 `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`。
- 后端命令与 serde DTO 位于 `src-tauri/src/ai_routing_gateway/commands/mod.rs`，Tauri 注册位于 `src-tauri/src/app_runtime/run_app.rs`。
- `accounts::create_api_key_account` 已在单一事务内写入账号、加密凭据、默认组和同名映射；`pricing::save_price` 已持有 API Key 类型检查、十进制校验和账号覆盖快照语义。新路径应复用这些规则，不改变现有三个独立写命令。
- `schema_v1.sql` 已包含 `ai_gateway_accounts`、`ai_gateway_credentials`、`ai_gateway_account_model_mappings`、`ai_gateway_model_prices` 及所需外键和级联删除；现有 schema 版本已至 v4。组合创建只使用既有列和表，故不新增迁移或数据回填。

## 实施方案

采用一个仅面向 API Key 新增的原子命令 `ai_routing_gateway_account_create_api_key_with_configuration`（名称在前后端同步固定）。它接收连接字段、可选显式映射和按公开模型组织的可选四类价格，返回现有 `AccountDto`/`GatewayAccount` 形状。旧 `ai_routing_gateway_account_create_api_key`、`ai_routing_gateway_account_update`、`ai_routing_gateway_mapping_save` 和 `ai_routing_gateway_price_save` 保持名称、DTO 和行为不变。

领域层在单个 `rusqlite::Transaction` 中先校验全部输入并生成最终映射，再写账号、加密凭据、默认组关联、映射及非空价格。价格写入复用 pricing 的解析和持久化核心，并提供 transaction 可用的内部入口；映射校验和写入抽取为 accounts 内部 transaction helper。任何校验或 SQL 失败均在提交前返回错误，Rust transaction drop 自动回滚，不发布账号事件。

前端账号池保留在同一模块内，以 `viewMode: "list" | "detail"`、`detailMode: "create" | "edit"` 和选中账号 ID 明确拆分状态；不引入 URL 路由。列表只负责筛选、分组与既有卡片管理动作，详情负责新建或编辑表单、映射、价格及保存。新增 API Key 使用新原子 facade 一次提交；已有 API Key 编辑继续通过旧的独立更新、映射和价格 facade。OAuth 详情显示已加载的映射和价格，但不渲染可写控件，也不发起写调用。

## 顺序执行步骤

1. 定义 Rust 新建 DTO 和领域输入结构：在 `commands/mod.rs` 增加 camelCase 反序列化 DTO，包含 `name`、`baseUrl`、`apiKey`、`authMethod`、`upstreamProtocol`、`note`、可选 `mappings`，以及每个 `publicModelId` 的 `inputPerMillionUsd`、`outputPerMillionUsd`、`cacheReadPerMillionUsd`、`cacheWritePerMillionUsd`。在 `accounts.rs` 定义仅供领域层使用的组合创建输入，避免把 command DTO 泄漏到领域层。依赖：无。验证：DTO 的 serde 名称和 TypeScript 契约逐字段对应。
2. 实现领域层组合原子创建：在 `accounts.rs` 将当前 API Key 创建的共有校验、加密、默认组解析、账号/凭据插入和默认映射构造收敛到 transaction helper；新函数在开启事务前完成连接字段和价格格式的可失败校验，在事务内读取官方模型、建立同名且启用的完整默认集、以显式映射按 `public_model_id` 覆盖并拒绝未知 ID，再写最终映射。依赖：步骤 1。验证：成功结果对每个官方模型都有一条映射，密钥仅以现有加密机制写入。
3. 抽取价格 transaction 写入最小接口：在 `pricing.rs` 保留 `save_price(&Connection, ...)` 的公开历史入口及其行为；提取共享的 API Key 账号类型、十进制校验和插入逻辑，使组合创建能对同一 `Transaction` 写 `account_override` 快照。组合路径仅为四个字段至少一个非空的模型写一条快照，沿用传入的创建时刻和现有历史快照列。依赖：步骤 2。验证：空价格不插入，任一提供价格非法时整个创建未提交。
4. 接入 command 与运行时注册：在 `commands/mod.rs` 新增 Tauri command，打开连接、取得 root key、调用组合领域函数，成功后按现有账号创建模式发出账号更新事件并返回 `AccountDto`；在 `run_app.rs` 的 AI routing gateway invoke handler 中注册一次，并将注册完整性测试命令清单加入新后缀。保持旧创建 command 和 `account_update` 原样。依赖：步骤 2、3。验证：命令可从 invoke handler 解析，失败不发成功账号事件。
5. 收紧 OAuth 映射写防线：修改 `accounts::set_model_mapping` 或其最小前置检查，在 upsert 前确认目标账号存在且类型为 `api_key`，OAuth 和不存在账号返回现有可识别 invalid/not-found domain 错误；`price_save` 保持现有 `pricing::save_price` 的 OAuth 拒绝。依赖：无，可与步骤 1-4 并行。验证：直接调用 `mapping_save` 不能给 OAuth 创建或修改映射。
6. 扩展 TypeScript typed IPC facade：在 `src/lib/aiRoutingGateway.ts` 导出原子新增 input 的显式接口（映射项、价格项、四类 nullable/optional价格）及 `aiRoutingGatewayAccountCreateApiKeyWithConfiguration` facade，固定 invoke 命令和 `{ input }` 参数形状，并返回 `GatewayAccount`。保留全部现有 facade。依赖：步骤 1、4。验证：facade 单测断言命令名、camelCase payload 和返回泛型。
7. 重构账号池 list/detail 视图：在 `src/components/AiRoutingGateway/index.tsx` 将列表内 `expanded`、`showCreate` 和内嵌详情替换为模块内部的 `viewMode`、`detailMode`、`selectedAccountId`。账号卡片显示名称、API Key/OAuth 标签、API 地址、映射 `public -> upstream` 列表，并保留按标签筛选、分组创建、上/下排序、启停和删除；主体和编辑动作进入编辑详情，新增进入空白创建详情。详情提供返回和保存，返回或保存成功后回到列表并 `reload`。依赖：步骤 6。验证：不新增路由，列表不再展开详情。
8. 实现创建与编辑详情差异：创建 API Key 表单以 `data.models` 初始化每个官方模型的同名、启用映射，允许改上游名称与启用状态，并为每个模型维护 input/output/cache_read/cache_write 四类每百万 token USD 字段；提交只调用新原子 facade，失败保留全部表单状态，成功清除敏感本地密钥并返回列表刷新。编辑详情继续使用 `account_update` 保存连接/基础字段，并对 API Key 保留 `mapping_save`、`price_save` 交互。OAuth 详情加载且显示连接、映射和价格，但将映射开关、添加/保存映射和价格输入/保存动作替换为只读展示。依赖：步骤 7。验证：新增不拼接旧写命令；API Key 编辑兼容旧命令；OAuth 无映射和价格写入口。
9. 补齐定向测试并运行验证矩阵：分别覆盖 facade、React 交互、accounts/pricing 领域原子性与命令注册。依赖：步骤 1-8。验证：所有列出的命令通过，且未修改 schema 或迁移文件。

## 任务边界与依赖

- 后端领域边界：`accounts.rs` 负责账号类型、默认映射、加密凭据、默认分组和组合事务编排；`pricing.rs` 只提供价格校验与 transaction-safe 快照插入。不得把前端 DTO、Tauri event 或 UI 判断放入领域函数。
- 命令边界：`commands/mod.rs` 仅转换 DTO、取得 root key、调用领域函数和成功后发事件；`run_app.rs` 仅注册命令。命令名称、旧命令与现有错误编码不得被替换。
- 前端契约边界：`aiRoutingGateway.ts` 是 Rust camelCase DTO 的唯一 typed IPC 入口；组件不得直接 `invoke`，也不得在新增流程调用旧创建后再逐项保存映射/价格。
- 视图边界：`index.tsx` 内账号池专属 list/detail 组件或局部组件可拆分，但不得改动 `AiEnvironments` 服务商组件，不新增 URL、深链接、OAuth 登录或官方模型/价格管理能力。
- 依赖顺序：Rust DTO/领域事务和 OAuth 拒绝先完成，命令注册与 TypeScript facade 随后完成，前端视图依赖 facade，测试最后覆盖端到端契约；独立的 OAuth mapping 防线可并行实施但必须在验证前合入。

## 具体改动

- `src-tauri/src/ai_routing_gateway/accounts.rs`：增加组合 API Key 创建领域输入和函数；抽取可复用的 transaction helpers；从官方模型生成默认映射并合并显式映射；在同一 transaction 内写账号、加密凭据、默认分组、映射和调用价格 helper；为 `set_model_mapping` 增加 API Key 类型防线；添加领域单元测试。
- `src-tauri/src/ai_routing_gateway/pricing.rs`：将现有十进制校验、API Key 账号判断和插入拆为共享私有函数及 transaction 变体，保留 `save_price` 对独立 `price_save` 的既有签名和语义；添加组合创建所需价格与回滚测试。
- `src-tauri/src/ai_routing_gateway/commands/mod.rs`：新增组合创建请求 DTO、Tauri command、成功事件发射路径；保留旧 `CreateAccountInput` 和所有既有命令；补充 command 层相关测试（如本文件已有测试模式适用）。
- `src-tauri/src/app_runtime/run_app.rs`：在隔离的 AI routing gateway command block 注册新 command，并更新“注册且仅一次”测试清单。
- `src/lib/aiRoutingGateway.ts`：新增原子创建 input/output 类型和 facade；既有 `aiRoutingGatewayAccountCreateApiKey`、`aiRoutingGatewayAccountUpdate`、`aiRoutingGatewayMappingSave`、`aiRoutingGatewayPriceSave` 不改名且继续导出。
- `src/lib/aiRoutingGateway.test.ts`：断言新 facade 的命令名、`{ input }` 包装、连接/映射/四类价格 camelCase payload，以及旧 facade 仍可调用。
- `src/components/AiRoutingGateway/index.tsx`：将账号池从折叠行改为卡片列表和模块内独立详情；实现 create/edit 状态与 API Key/OAuth 可写性差异；保持既有列表管理动作。
- `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`：更新现有折叠详情断言，增加卡片字段、进入/退出详情、创建默认映射和原子 payload、提交成功刷新返回、失败保留表单、OAuth 只读与 API Key 编辑旧命令兼容断言。
- 不修改 `src-tauri/src/ai_routing_gateway/schema_v1.sql`、`schema_v2.sql`、`schema_v3.sql`、`schema_v4.sql` 或 shared SQLite migration 代码：已有映射和价格表及外键已经承载所有新增记录，规格也要求无事实性缺口时不迁移。

## 接口与数据流

1. 新增详情从 bootstrap 的官方 `models` 初始化前端映射数组：每项 `{ publicModelId, upstreamModelId: publicModelId, enabled: true }`，价格四字段为空。
2. 用户编辑后，前端将连接信息、最终显式映射和每模型可选四项价格一次传给 `aiRoutingGatewayAccountCreateApiKeyWithConfiguration`；空价格表达“无账号覆盖”，而非写入空快照。
3. command 反序列化 DTO，取得 root key，并调用 accounts 组合创建。领域层先验证公开模型 ID 与价格格式，再在一个 SQLite transaction 中插入账户、密文凭据、默认组关联、完整最终映射和非空账号覆盖价格；commit 后读取并返回不含明文密钥的账户 DTO。
4. command 仅在 commit 成功后发送账号更新事件；前端成功时 reload 并切回列表。失败时 facade 抛出 `AiRoutingGatewayError`，详情保留本地内容且列表不产生新卡片。
5. 编辑 API Key 继续分别使用旧 `account_update`、`mapping_save` 和 `price_save`。OAuth 详情仅消费 `mapping_list`、`prices_list` 和 quota/账号读取数据；UI 不调用写 facade，后端仍拒绝其 `mapping_save` 和既有 `price_save` 写入。

## 失败处理

- 未知 `publicModelId`：在映射合并前拒绝，事务未启动或未提交，返回现有 `invalid_input` 类错误。
- 空名称、密钥、无效 API 地址或认证方式：沿用当前 accounts 校验并不留下记录。
- 任一价格为负数、科学计数法、空显式值或超过既有小数精度：复用 `parse_decimal` 规则拒绝整个组合请求；空字段本身不校验也不插入。
- 账号、凭据、默认组、映射或价格任一 SQL 操作失败：不 commit transaction，自动回滚所有本请求插入，不发送事件。
- OAuth 调用 `mapping_save`：accounts 类型检查拒绝；`price_save` 保持 pricing 的 API Key 限制；UI 仅提供只读信息，避免正常路径触发。
- 新命令失败：详情错误区域展示 facade 归一化错误，所有输入、映射和价格状态保留；不清空 API Key 或切回列表。

## 测试与验证

| 层级 | 覆盖矩阵 | 可执行命令 |
| --- | --- | --- |
| TypeScript facade | 新命令、camelCase 输入、四类价格、返回类型；旧 account/mapping/price facade 未变 | `npm test -- src/lib/aiRoutingGateway.test.ts` |
| React | 卡片信息和管理动作；list/detail/create/edit 切换；默认同名映射；原子新增 payload；成功回列表刷新；失败保留值；OAuth 映射/价格只读；API Key 编辑旧命令 | `npm test -- src/components/AiRoutingGateway/AiRoutingGateway.test.tsx` |
| Rust accounts | 成功时加密凭据、默认组、全部官方模型最终映射、显式覆盖、仅非空四类价格；未知模型、非法价格和强制后续写入失败均零残留；OAuth mapping 拒绝 | `cargo test ai_routing_gateway::accounts --manifest-path src-tauri/Cargo.toml` |
| Rust pricing | 共享 transaction 写入不改变既有价格校验、历史快照和 OAuth 拒绝 | `cargo test ai_routing_gateway::pricing --manifest-path src-tauri/Cargo.toml` |
| Rust registration | 新 command 在 AI routing gateway block 中恰好注册一次，旧 command 仍在 | `cargo test ai_routing_gateway_commands_are_registered_once_in_the_isolated_block --manifest-path src-tauri/Cargo.toml` |
| 回归与构建 | 前端完整测试、lint、类型构建与 Rust 全量测试 | `npm test && npm run lint && npm run build && cargo test --manifest-path src-tauri/Cargo.toml` |

人工验收使用本地应用的账号池：确认新增页面所有官方模型预填同名且启用、四项价格可按模型填写；创建后卡片显示名称、认证标签、地址和映射；进入 OAuth 卡片可读但没有映射/价格写控件；已有 API Key 的编辑、映射和价格保存仍可用。

## 验收标准

- 账号池采用卡片列表，卡片显示账号名称、API Key/OAuth 标签、API 地址、映射关系及原有排序、启停、删除动作。
- 卡片主体/编辑和新增均进入模块内部详情，返回和保存成功均回列表并刷新，无 URL 或深链接改动。
- API Key 新增表单对全部官方模型展示同名、启用默认映射，允许覆盖上游名、启用状态和四类每百万 token USD 价格。
- 新增提交只触发新 typed IPC 原子 facade；账户、密文凭据、默认组、最终映射和每个非空价格快照在同一 transaction 成功提交。
- 没有显式映射时保存完整默认集合；显式项覆盖默认；未知公开模型、非法价格与中途存储失败不留下任何本请求数据。
- OAuth 详情能显示现有连接、映射和价格，映射与价格不可编辑；后端拒绝 OAuth `mapping_save`，并维持 `price_save` 拒绝。
- `account_update`、`mapping_save`、`price_save` 和旧 API Key 创建接口继续保留，现有 API Key 编辑行为不回归。
- 测试矩阵命令通过，且 schema/migration 文件未发生变更。

## 兼容、迁移与发布

- 兼容性：新 command 为附加接口；所有旧命令、参数和返回约定继续存在。既有账户数据无需转换，详情仅改变 `AiRoutingGateway` 模块内部呈现方式。
- 迁移：不新增 schema migration。`ai_gateway_accounts` 已保存 API Key 连接字段并关联默认组，`ai_gateway_credentials` 保存密文，映射表以 `(account_id, public_model_id)` 唯一，价格表保存四个可空覆盖字段及账号外键，完整满足组合写入。
- 发布：先完成 Rust 事务与 command 注册及其测试，再提交 typed facade 和 UI；通过全量验证后作为单一应用版本发布。失败回退仅移除新 command/facade/UI 调用并恢复旧列表呈现，不触碰既有表或数据；已创建的有效账号记录保持兼容，因事务原子性不需要清理半成品数据。
