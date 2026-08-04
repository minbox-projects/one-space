# AI 路由网关账号页面重设计实施计划

## 计划元数据

- plan-id: `redesign-ai-routing-gateway-account-pages`
- status: `ready-for-implementation`
- source_spec: `.ai-work-flow/plans/redesign-ai-routing-gateway-account-pages/spec.md`
- source_spec_digest: `65df4d023f7d8d21684162dfeb24bc7ca71ca5fe5eff5c2c7c7b362a7182817b`
- task_mode: `split`

## 技术与代码上下文

- `src/components/AiRoutingGateway/index.tsx` 当前在同一模块中实现 `AccountsTab`、内联 `AccountDetail`、账号操作和表单局部状态。重设计应沿既有函数边界拆出账号列表卡片、账号详情表单和账号页状态协调组件，保留现有 React、Tailwind、i18n、Tauri facade 与测试选择器风格。
- `src/components/AiEnvironments/ServiceProviderList.tsx`、`ServiceProviderDetail.tsx` 及其 `index.tsx` 的 list/detail 状态模式仅作为交互组织参考；账号池保留独立 DTO、状态与数据请求，不复用或耦合终端服务商数据。
- `src/lib/aiRoutingGateway.ts` 是前端调用 Tauri 命令的唯一 typed facade。所有新命令、输入草稿和详情返回类型必须先在此定义并通过 `call` 统一转换错误，组件不得直接 `invoke`。
- Rust 账号域位于 `src-tauri/src/ai_routing_gateway/accounts.rs`：账号、加密凭据、标签、模型映射及删除确认在此实现；命令编排在 `commands/mod.rs`；官方价格与账号字段级覆盖逻辑在 `pricing.rs`；跨进程 DTO 位于 `types.rs`。
- 现有 `create_api_key_account` 已将账号、加密凭据和官方模型默认映射置于一个事务，`ai_routing_gateway_account_update` 已在命令层对连接、账号字段与标签使用事务；现有映射和价格保存仍是独立命令。新保存入口应取代页面对这些分散命令的编排。
- `AccountDto`、`accounts_list` 和 bootstrap 保持无 API Key 明文。`gateway_key::copy_plaintext` 展示了取得 `RootKey` 后解密 credential 的模式，但账号详情必须使用独立 DTO 和命令，不能改变网关密钥对象或返回契约。
- 现有 `ai_gateway_accounts`、`ai_gateway_credentials`、`ai_gateway_account_model_mappings` 和 `ai_gateway_model_prices` 可承载所需数据；不新增数据库 schema migration。

## 实施方案

- 在 `types.rs` 与 `commands/mod.rs` 定义账号详情、共享保存草稿和模型行 DTO。保存草稿以可选 `account_id` 区分新增与编辑：缺失时只允许 API Key 新增并在事务中分配 ID；存在时要求目标为 API Key 并更新该 ID。草稿包含当前账号字段、API Key 连接字段、完整固定官方模型集合的映射，以及每项四个可空的账号价格覆盖字段。
- 后端将新增与编辑收敛为一个原子 upsert/update 命令。先完成输入与官方模型集合校验，再开启单个 SQLite 事务；在事务内创建或更新账号、加密并写入凭据、替换标签、对每个官方模型 upsert 映射、写入或删除该模型各字段的账号覆盖，最后提交。任一步返回错误时不提交，并以稳定的领域错误代码返回；成功后只返回无密钥的 `AccountDto` 并发送既有账号更新事件。
- 价格持久化应保留 `pricing.rs` 的官方优先与字段级回退语义：草稿中的 `null` 表示删除对应账号字段覆盖并继承官方值，非空值经现有十进制定价校验后保存为 `account_override`。将价格辅助函数改造为可接收事务，避免每个模型或字段独立提交；按现有有效时间规则写入或更新可识别的当前覆盖，恢复官方价格不创建空价格快照。
- 新增 API Key 账号时，为事务中读取到的每个官方模型创建同名、启用的映射；价格不复制官方记录，靠无覆盖字段继承官方价格。编辑时官方模型集合是权威固定行集合，新增官方模型自动获得同名映射；草稿不得提交非官方模型或删除映射行。禁用只更新映射 `enabled`，不删除其上游名称或价格覆盖。
- 新增详情读取命令仅供账号详情页使用：先读取无密钥账号数据、完整映射、价格视图和 OAuth 公开元数据；仅当 `account_type` 为 `api_key` 时，使用 `RootKey` 和 `decrypt_credential` 读取对应 credential，并在专用详情 DTO 的 `api_key` 字段返回 UTF-8 明文。OAuth 返回 `api_key: None`，不解密 token bundle，也不暴露 OAuth client secret、access token 或 refresh token。命令、事件、列表、bootstrap、日志和错误文本均不得拼接、序列化或记录该明文。
- 前端以账号页面自身维护 `viewMode`、当前详情 ID、详情加载状态、筛选值和成功提示。进入详情时调用专用详情接口，离开或保存成功返回列表时保留原筛选；详情草稿通过初始快照比较产生脏状态，切换卡片、返回、删除前先确认。保存期间冻结重复提交与冲突操作，失败保留草稿并将领域错误映射到顶部汇总、对应字段和首个错误控件。
- API Key 详情复用一套新增/编辑表单，OAuth 详情使用同一展示骨架但只渲染公开字段、完整映射和官方有效价格，且不提供连接、映射、价格或保存控件。密码输入的明文只存在详情 DTO 与受控草稿中；不写入全局账号列表、事件处理器、错误状态或调试输出。
- 列表卡片展示认证类型、地址、最多前三项映射和禁用状态；OAuth 地址固定显示 `-`。卡片内启停、排序、删除、编辑等控件调用 `stopPropagation`，防止触发卡片详情导航；启停和排序继续使用既有窄命令，删除继续先获取并消费确认令牌。

## 顺序执行步骤

1. 审核并扩展 Rust DTO、命令输入与前端 facade 类型，区分无密钥列表 DTO、可含 API Key 的专用详情 DTO和原子保存草稿；同步注册新 Tauri 命令。
2. 在 `accounts.rs` 与 `pricing.rs` 提取可在同一 SQLite transaction 中调用的账号字段、凭据、标签、固定官方映射和字段级价格覆盖写入辅助函数，实施 API Key 原子新增/编辑命令与详情读取命令。
3. 为后端补充事务、默认映射、价格继承/恢复、OAuth 只读和明文边界的单元与命令测试，确保旧的单项映射/价格命令不再被账号详情保存路径调用。
4. 新建账号列表卡片、详情页面及共享表单组件，将 `AiRoutingGateway/index.tsx` 的账号页改为独立 list/detail 页面状态，同时维持其余页签行为和现有账号操作。
5. 将列表与详情只经 `src/lib/aiRoutingGateway.ts` 调用新接口，实现筛选恢复、脏状态确认、字段错误聚焦、保存/加载/删除状态和成功后刷新。
6. 扩展前端与 facade 测试，再运行前端完整质量检查和 Rust 定向测试；根据失败输出收敛命令序列、可访问名称及错误映射。

## 任务边界与依赖

- 数据契约与 Rust 原子保存是前置边界：前端详情和共享草稿必须建立在稳定的详情 DTO、保存 DTO、错误代码和命令注册之上。
- 价格事务辅助依赖现有 `pricing` 验证和官方快照查询，账号保存编排依赖该辅助与 `accounts` 的 credential/映射/标签事务函数；不得由命令层分别调用会自行提交的保存函数。
- 前端 facade 类型与调用测试依赖最终 Tauri 命令名称及 serde 字段命名；组件拆分依赖 facade 契约，但不依赖或修改 AI Environments 的实现。
- 列表卡片与详情 UI 可并行开发，但 list/detail 状态协调、筛选恢复和脏状态保护需要两者接口完成后整合。
- 测试工作按后端持久化与安全边界、facade 命令编组、组件交互三层划分；不在本计划中生成任务文件或任务草案。

## 具体改动

- 调整 `src-tauri/src/ai_routing_gateway/types.rs`，增加账号详情返回、原子 API Key 保存输入、固定模型映射草稿及四字段价格覆盖 DTO；明确 `AccountDto` 不增加 API Key 字段。
- 调整 `src-tauri/src/ai_routing_gateway/accounts.rs`，保留既有创建、更新、删除兼容入口所需行为，同时增加统一事务中的 API Key upsert/update、详情组装和 API Key 专用解密辅助；OAuth 分支仅读取公开字段。
- 调整 `src-tauri/src/ai_routing_gateway/pricing.rs`，提供可复用的 transaction 内账户覆盖校验、写入和恢复辅助，复用既有 decimal 校验与官方价格回退语义。
- 调整 `src-tauri/src/ai_routing_gateway/commands/mod.rs` 及命令注册位置，暴露详情读取与原子保存命令，取得 root key 仅传递给需要解密或加密的账号路径，维持事件负载为无明文账号 DTO。
- 调整 `src/lib/aiRoutingGateway.ts`，添加详情、草稿和保存 typed facade，移除账号详情 UI 对独立 mapping/price 保存调用的依赖；更新 `aiRoutingGateway.test.ts` 断言命令名、参数封装和错误包装。
- 将 `src/components/AiRoutingGateway/index.tsx` 中账号列表与内联详情拆分为同目录账号列表卡片、详情表单和页面状态组件，保持主组件负责 bootstrap、事件订阅与页签装配；按仓库习惯补充必要翻译键和无障碍标签。
- 更新 `src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，覆盖卡片显示、导航、局部操作事件隔离、详情编辑与只读分支、筛选恢复、脏状态确认、保存反馈和删除确认令牌复用。

## 接口与数据流

- bootstrap 与账号列表继续传递 `GatewayAccount`/`AccountDto`，其中只含展示所需的账号、标签和映射数据。账号列表不可请求或缓存 API Key 明文。
- 点击 API Key 卡片或新增入口后，页面请求详情 DTO；编辑场景 DTO 中唯一的 `apiKey` 明文填充受控草稿。OAuth 卡片请求同一专用详情读取，但返回公开元数据、映射和官方/有效价格视图，API Key 字段为空。
- 保存时前端将完整草稿作为一次请求发送。后端以 `accountId` 的存在区分创建和更新，在一个 transaction 内处理账号、credential、tags、映射和价格覆盖，成功后返回无密钥账号并发出更新事件；前端清除明文草稿、刷新 bootstrap、恢复原筛选并回到列表。
- 每个固定官方模型行包含映射开关与上游模型名，以及输入、输出、缓存读、缓存写四个覆盖字段。读取时 UI 以官方值和覆盖值计算显示有效值及继承状态；保存时只发送覆盖值，恢复动作将对应字段设为 `null`。
- 详情读取、保存成功结果、账号事件、`GatewayBootstrap`、请求日志 DTO 和 `AiRoutingGatewayError` 的 message 均以类型和测试保证不携带 API Key 明文。后端错误仅返回分类/实体标识，前端错误汇总不拼接草稿机密字段。

## 失败处理

- 保存请求前验证名称、组、Base URL、认证方法、协议、必填 API Key（仅新增）、固定官方模型集合、非空启用映射名称、阈值范围和四类价格格式；前端保留草稿，后端重复验证以防绕过。
- 事务内任一账号、credential、标签、映射或价格操作失败时回滚。新建不留下账号、credential、映射或覆盖价格；编辑保持完整旧状态。提交失败按 storage 错误处理且不发送账号更新事件。
- API Key 解密失败、不是 API Key 的详情读、账号不存在或非法输入返回既有领域错误类别，不在错误内容中带密文、明文、token 或草稿值。OAuth 详情绝不触发 credential 解密。
- 列表/详情加载失败展示非机密通用错误与重试入口；保存失败展示顶部汇总、字段级错误并滚动聚焦首个错误。保存期间禁用重复保存、返回、编辑和删除触发点。
- 放弃新增或编辑、切换详情、切换账号页模式以及返回列表时，若草稿有修改则显示确认；确认离开后清除含 API Key 的本地草稿。禁用映射或恢复价格不影响未保存的其他字段。
- 删除仍先申请一次性确认令牌再执行永久删除；令牌失效、账号已删除或删除失败时保留列表/详情状态并显示通用错误，成功后清除选中详情和可能失效的筛选值。

## 测试与验证

- 在 `accounts.rs` 增加数据库测试：原子新增覆盖账号、加密 credential、默认官方同名映射和无价格覆盖；在映射或价格写入制造失败时验证所有写入回滚；编辑失败后验证基础字段、密钥、映射和价格均保持原值。
- 为专用详情读取增加测试：API Key 详情只在该 DTO 返回正确明文；`get_account`、账号列表、bootstrap 序列化、账号事件和错误字符串不含 fixture secret；OAuth 详情不返回 API Key 且不解密 OAuth token。覆盖错误 root key 与 AAD/credential 异常。
- 在 `pricing.rs` 测试字段级覆盖、官方回退、恢复单字段或全字段为继承、仅 API Key 可保存覆盖、禁用映射后价格仍保留，以及固定官方模型新增时同名映射默认值。
- 在 `src/lib/aiRoutingGateway.test.ts` 测试详情读取与原子保存的 typed 参数、camelCase/snake_case 命令边界、响应类型及错误包装；确保账号页不再以多次 facade 调用保存映射和价格。
- 在 `AiRoutingGateway.test.tsx` 使用 Tauri mock 覆盖 API Key/OAuth 卡片、前三项映射截断与其余计数、禁用标记、OAuth `-`、卡片主体导航、图标/行内操作阻止传播、筛选恢复、脏状态确认、保存禁用与字段错误聚焦、成功刷新返回、OAuth 只读、编辑页删除确认流程。
- 执行 `npm run test`、`npm run lint`、`npm run build`；执行与账号、价格、命令相关的 Rust 定向测试，并在修改共享 Rust 契约后运行相应 crate 测试。验证过程中使用不含真实密钥的 fixture，检查测试失败输出与日志不回显 fixture secret。

## 兼容、迁移与发布

- 不改变现有表结构或创建 schema migration。现有 API Key、OAuth、映射、标签和价格记录在新详情读取与保存逻辑下保持可读；缺失的官方模型映射在读取或下一次原子保存时按明确的兼容规则补齐，避免破坏已有路由数据。
- 保持已有账号列表、bootstrap、事件和单项操作的无密钥返回契约；新增命令采用新名称和 DTO，不向现有列表 DTO 添加敏感字段。若保留旧创建、更新、映射或价格命令，标记为既有兼容路径并避免新页面调用。
- 不增加 URL 路由、深链接或 OAuth 授权流程。账号池组件只参考而不连接 AI Environments 的列表/详情代码和数据源。
- 发布前以已有账号数据库的升级样本验证：API Key 可读取、OAuth 可只读展示、历史映射和价格的有效值不变、删除确认仍可用；在错误监控与调试日志中禁止记录保存草稿和详情响应中的 API Key。
