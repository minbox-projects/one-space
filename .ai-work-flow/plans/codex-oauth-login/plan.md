# Codex OAuth 登录实施计划

## 计划元数据

- plan-id: `codex-oauth-login`
- status: `ready-for-implementation`
- source_spec: `.ai-work-flow/plans/codex-oauth-login/spec.md`
- source_spec_digest: `af0f19deb39612432f67b658617b488674de445675d0b7cbfde9dd1ffb963deb`
- task_mode: `split`

## 技术与代码上下文

现状：AI 路由网关已有 OAuth 账号模型、`OAuthSessionStore` 的随机 loopback、PKCE S256、短期会话、手动完整回调校验和取消清理能力；`accounts.rs` 已有基于稳定外部身份的 OAuth upsert、RootKey + AES-256-GCM 凭据加密与原子写入；`runtime.rs` 已能读取 OAuth 凭据并参与路由。前端已有账号池、账号详情、typed IPC facade 和网关事件订阅。

缺口：生产 OAuth store 默认处于发布阻断状态，配置仅含授权端点且保留 Device Code 分支；已有 OAuth commands 未纳入 Tauri invoke handler，完成命令只校验 callback 而未交换 token、解析身份或保存账号；facade 与账号池没有 Codex 登录工作流；刷新、轮换、重新授权与退出登录的真实网络生命周期尚未完成。

目标：在不改变 Gmail OAuth、API Key 行为、OAuth 账号模型映射或价格覆盖的前提下，为账号池接入 Codex-Manager 兼容的可替换 Authorization Code + PKCE S256 登录链路，安全保存可路由的账号并维护其刷新和重新授权状态。

## 实施方案

1. 将 Codex-Manager 兼容参数封装为内部 provider 配置，包含授权端点、token 端点、公开 client_id、scope、必要兼容参数、issuer/audience/JWKS 验证资料和可选 revoke endpoint；明确其不是官方稳定第三方契约，禁止引入 client_secret。
2. 收敛 `oauth.rs` 到浏览器授权码流程：保留随机端口、内存 session、state、PKCE verifier、nonce、TTL 与同一路径的手动完整 callback 校验；删除或隔离本功能路径对 Device Code 的调用，不启用 API-key token exchange。
3. 在后端 OAuth 服务层完成 token 交换、可信声明验证和身份映射；仅在取得 `chatgpt_account_id`（可附加 `workspace_id`）或回退 `sub` 后调用现有 OAuth upsert。token bundle、refresh token 和 provider 连接资料只进入加密载荷或非敏感 metadata。
4. 将 OAuth commands 注册到 Tauri，并通过 facade 提供开始、提交手动回调、取消、退出登录和状态事件；开始时绑定临时 loopback listener 并打开系统浏览器，listener 失败时保留会话供手动回调。
5. 在账号池入口添加 OAuth 与 API Key 并列的添加方式，以及等待、取消、超时、错误和手动回调 UI；OAuth 账号连接字段只读，继续允许现有分组、标签、备注和启用状态管理。
6. 在路由凭据读取路径加入到期前刷新、refresh token rotation 的加密凭据原子替换、单次刷新后单次原请求重试、有限退避和永久失效标记；退出登录清除本地凭据并禁用账号，仅在配置了可靠 revoke endpoint 时远端撤销。

## 顺序执行步骤

1. 盘点 `oauth.rs`、`accounts.rs`、`runtime.rs`、`commands/mod.rs`、`run_app.rs` 与 typed facade 的现有 DTO、错误码和事件契约，确定不改变 Gmail/API Key 路径的接入点。
2. 添加可替换 Codex provider 配置及 token/claims 服务，扩展 OAuth session 的 nonce 和完成结果，使 callback 校验后才可执行 code 交换。
3. 实现身份映射、加密 upsert、退出登录和刷新凭据事务；将永久失效接入已有 `oauth_reauthorization_required` 健康原因。
4. 实现 loopback listener 与浏览器打开编排，接通并注册命令、事件和 facade，确保自动与手动回调汇合到同一后端完成路径；同步三份导航索引，准确记录 commands、注册、领域入口、facade 和事件订阅。
5. 修改账号池 UI 与本地化文案，接入授权状态机、账户列表刷新和 OAuth 账户只读连接信息。
6. 让路由在使用 OAuth 凭据前检查到期时间，按单次刷新重试和有限退避规则处理错误，随后完成单元、集成、facade 和前端交互验证，并只读复验三份导航索引与最终代码一致。

## 任务边界与依赖

1. `codex-oauth-provider-protocol`：建立 Codex OAuth provider 配置与授权码协议模型。集中定义可替换的 Codex-Manager 兼容授权端点、token 端点、公开 client_id、scope、兼容参数、issuer、audience、JWKS 与可选 revoke endpoint，并扩展 OAuth 会话的随机 loopback、PKCE S256、state、nonce、TTL、一次性消费及自动与手动回调共用的严格校验模型；明确兼容风险且不引入 client_secret、Device Code 或 API-key token exchange。
2. `codex-oauth-token-oidc-backend`：实现真实授权码交换与 OIDC 身份验证后端。在严格回调校验成功后执行真实 authorization code 与 PKCE verifier 的 token 交换，验证可信 id_token 的 exp、iss、aud 和 nonce，并在缺少可信 JWKS 时记录可见验证降级；按 chatgpt_account_id、可选 workspace_id 和 sub 回退规则生成稳定身份，无可靠主体或任一交换、解析、验证失败时拒绝产生成功账号。
3. `codex-oauth-credential-lifecycle`：接通加密账号持久化、刷新轮换与退出生命周期。复用稳定外部身份 upsert 和 RootKey + AES-256-GCM 边界原子保存 OAuth 凭据，保证同一主体重新登录更新原账号且工作区隔离；在路由使用前完成到期刷新、refresh token rotation 整组替换、授权失败后最多一次刷新与一次原请求重试、临时错误有限退避及永久失效重新授权标记，并实现本地凭据清除、账号禁用和可靠端点下的可选远端撤销。
4. `codex-oauth-tauri-typed-ipc`：注册 Tauri OAuth commands 并完善 typed IPC 契约。将开始登录、自动或手动完成回调、取消和退出登录 commands 注册到 Tauri invoke handler，编排临时 loopback listener 与系统浏览器并在 listener 失败时保留手动回调会话；同步 Rust DTO、状态事件和 TypeScript facade，确保参数与序列化一致且敏感材料不通过 IPC、事件或日志泄露；同步 `.ai-work-flow/index/feature-navigation.md`、`backend-navigation.md`、`frontend-navigation.md`，准确导航四个 OAuth commands、`run_app.rs` 注册入口、`oauth.rs` 领域入口、TypeScript OAuth facade 与事件订阅，且索引不含敏感材料；从既有提交事实继续修复 `SPEC-OAUTH-001`、`SPEC-OAUTH-002`、`SPEC-OAUTH-003` 与 `standards-index-sync-001` 四个 blocking findings。
5. `codex-oauth-account-pool-ui`：实现 React 账号池 Codex OAuth 登录交互。在账号池中增加与 API Key 并列的 OAuth 添加入口，接入 typed facade 和状态事件，完整处理浏览器授权等待、取消、超时、错误、手动粘贴完整回调、成功后刷新账号列表及重新授权；保持 OAuth provider 与连接字段只读，同时保留分组、标签、备注和启用状态管理并补齐本地化文案。
6. `codex-oauth-cross-layer-verification`：完成 Codex OAuth 跨层测试与回归验证。补齐 Rust 协议、回调、token、OIDC、身份去重、加密持久化、rotation、路由刷新重试、退出登录测试，验证 Tauri command 注册、typed IPC DTO 与无敏感字段事件，并覆盖 React 登录状态机、手动回调、成功刷新和只读连接信息；执行 Rust、前端定向及全量测试、lint 和构建，确认 API Key、Gmail OAuth 与网关 Bootstrap 行为无回归；只读复验三份导航索引与最终四个 OAuth commands、`run_app.rs` 注册入口、`oauth.rs` 领域入口、TypeScript OAuth facade 及事件订阅一致，不授予任何索引写权限，发现不一致时阻断。

## 具体改动

- `src-tauri/src/ai_routing_gateway/oauth.rs`：替换发布阻断的生产配置接入，定义 Codex provider 配置和仅授权码 PKCE 的 session/回调编排；保留一次性消费、TTL、origin/path/state 校验与 listener 失败后的手动回调能力。
- `src-tauri/src/ai_routing_gateway/accounts.rs`：复用 `upsert_oauth_account`、`load_oauth_refresh_material`、`replace_oauth_tokens` 等边界，确保稳定身份 upsert、凭据整组原子替换、退出时清除凭据并禁用账号。
- `src-tauri/src/ai_routing_gateway/runtime.rs`：在 OAuth 上游请求前进行到期前刷新；授权失败最多刷新和重试各一次；临时错误有限退避，永久错误写入 `oauth_reauthorization_required` 且停止路由该账号。
- `src-tauri/src/ai_routing_gateway/commands/mod.rs`：将 begin/complete/cancel 演进为完成登录生命周期的 command，并新增必要的退出登录 command、DTO 和事件；command 不返回敏感 token。
- `src-tauri/src/app_runtime/run_app.rs`：以配置化 OAuth store 初始化，并在 `generate_handler!` 注册已实现的 OAuth commands。
- `src/lib/aiRoutingGateway.ts`：增加与 Rust DTO 一致的 OAuth 输入、结果、状态和 facade 函数，不向 React 暴露凭据。
- `src/components/AiRoutingGateway/index.tsx` 及相邻账号池组件：增加 OAuth 添加入口与授权对话状态，调用 facade、订阅事件、完成后刷新 bootstrap；OAuth 编辑页继续只读连接字段。
- `src/i18n.ts` 及现有语言资源：增加所需的等待、取消、超时、手动回调、重新授权和兼容风险文案，避免把 token 或 callback 中的敏感查询参数回显到日志。
- `.ai-work-flow/index/feature-navigation.md`、`.ai-work-flow/index/backend-navigation.md`、`.ai-work-flow/index/frontend-navigation.md`：仅任务 04 更新，用于同步 OAuth commands、Tauri 注册与领域入口、TypeScript facade 和 OAuth 事件订阅的代码导航；任务 06 仅验证其与最终实现一致。

## 接口与数据流

前端账号池发起 `begin` -> Rust 分配一次性随机 loopback 端口、session_id、state、nonce 和 PKCE verifier -> 打开系统浏览器 -> loopback listener 或用户粘贴的完整 callback URL 调用同一 `complete` command -> `OAuthSessionStore` 消费 session 并严格验证 scheme/loopback origin/path/state/TTL -> token 服务提交 authorization code + verifier -> claims 验证和主体映射 -> `upsert_oauth_account` 在单个事务中加密保存 -> account event/command 结果驱动前端刷新列表。

主体键为 `chatgpt_account_id`，存在 `workspace_id` 时将二者组成稳定外部身份；前者缺失才用 OIDC `sub`，email/name 仅展示。无法取得可靠主体、token 交换失败、claims 解析失败或加密写入失败时不产生成功账号。

路由数据流为候选 OAuth 账号 -> 解密 token bundle -> 临近到期时刷新 -> 成功则原子替换 access/refresh token 及过期信息 -> 发送上游请求；若收到可恢复授权失败，最多执行一次刷新和一次同请求重试。刷新永久失败时停止候选账号路由并标记重新授权；临时失败遵循有上限退避，不无限重试。

## 失败处理

- callback 的 state、origin、path、TTL、session 存在性和一次性消费任一失败，立即拒绝且不得交换或落库；取消、超时和终态错误清除会话与前端等待状态。
- 有可信 JWKS 时校验 `id_token` 的 `exp`、`iss`、`aud` 和 nonce；不能获取可信 JWKS 时记录可见验证降级状态，不将其表述为完整 OIDC 验证。
- token、refresh token、authorization code、PKCE verifier 和完整 callback URL 不写日志、不进入公开 metadata、不从 typed IPC 返回。错误仅使用既有分类/安全错误码供 UI 映射。
- 远端 revoke 仅在配置中明确可靠 endpoint 时调用；无 endpoint 或调用失败不阻断本地凭据清除和账号禁用。

## 测试与验证

- Rust 单元与集成测试：覆盖 PKCE S256、随机 loopback、手动/自动回调共用严格校验、超时/取消/重放、token 请求参数、claims 验证与降级、身份去重与 workspace 隔离、AES-GCM 载荷无泄漏、rotation 原子更新、刷新重试/退避/永久重新授权、退出登录。
- Tauri/IPC 测试：验证 commands 已注册，facade 的参数名和序列化 DTO 一致，事件不包含敏感字段。
- 导航索引验证：任务 04 核验三份索引已准确覆盖四个 OAuth commands、`run_app.rs` 注册、`oauth.rs` 领域入口、`aiRoutingGateway.ts` typed facade 与 OAuth 事件订阅，且没有 access token、refresh token、id_token、authorization code、PKCE verifier 或完整 callback URL；任务 06 以只读方式复验相同索引与最终代码一致。
- React 测试：验证 OAuth 入口、等待、取消、超时、错误、手动完整 callback、成功后列表刷新，以及 OAuth 账号连接字段只读且现有标签/备注/启用控制可用。
- 回归范围：现有 API Key 创建/编辑/路由测试、Gmail OAuth 相关测试和网关 Bootstrap 行为。
- 执行命令：`cargo test --manifest-path src-tauri/Cargo.toml`、`npm test -- --run src/lib/aiRoutingGateway.test.ts src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`、`npm test`、`npm run lint`、`npm run build`。

## 验收标准

- 账号池中 OAuth 登录与 API Key 入口并列，浏览器授权仅使用 Authorization Code + PKCE S256，自动和手动 callback 都经过同一严格校验。
- Codex-Manager 兼容参数集中可替换，并显式标记非官方稳定契约；不存在客户端 secret、Device Code 或 API-key token exchange 实现。
- 成功流程完成 token 交换、必要 claims 验证、稳定主体映射、AES-256-GCM 加密落库和账号列表刷新；重新登录更新同一账号，不产生重复账号。
- OAuth token 生命周期符合到期前刷新、rotation 原子替换、单次刷新重试、有限退避与永久重新授权规则；退出登录清除本地凭据并禁用账号。
- API Key 与 Gmail OAuth 行为保持不变，相关 Rust、IPC facade、前端交互和全量回归命令通过。
- 任务 04 的 exhaustive write scope 包含且仅由其写入三份导航索引；任务 06 不含索引 write scope，只读复验索引一致性，发现偏差即阻断。
- 保持六任务的 task_id、顺序和 `blocked_by`：01 无依赖，02 依赖 01，03 依赖 02，04 依赖 01-03，05 依赖 04，06 依赖 01-05；任务 01-03 不重新实施，任务 04 基于提交 `83e0a36dfe7509cf51379a5d6e1e589ef6509cc9` 继续修复 `SPEC-OAUTH-001`、`SPEC-OAUTH-002`、`SPEC-OAUTH-003` 与 `standards-index-sync-001`。

## 兼容、迁移与发布

不迁移 Gmail OAuth 或既有 API Key 数据，不修改 OAuth 账号模型映射或价格覆盖。新 Codex OAuth 记录继续使用现有加密 schema 和稳定身份 upsert；无法可靠映射主体的记录不创建不安全关联。

发布前应在隔离测试账号上验证当前兼容配置的端点、公开 client_id、scope 和兼容参数仍可用，并将变动限制在 provider 配置。若兼容契约失效、JWKS 信任条件不满足或上线出现授权失败，回滚为关闭 Codex OAuth 入口/阻断新登录；已有 API Key 与 Gmail OAuth 不受影响。对已保存 Codex 账号，保留非敏感管理配置并标记重新授权，禁止继续路由；不得通过降级校验或持久化明文凭据恢复服务。

本计划落实规格修订 `REV-001`：保持 split 模式、六任务结构、既有产品行为和安全边界不变，由任务 04 同步导航索引，由任务 06 独立只读复验。任务 01-03 已整合至 `integration`（发现时 HEAD 为 `47d2d32e29e942f87ca35e2c38716dc1c82b33a3`），后续实施不得重做；任务 04 的现有提交尚未整合，须从其评审事实继续。
