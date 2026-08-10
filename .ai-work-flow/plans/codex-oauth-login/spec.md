# Codex OAuth 登录规格

## 问题陈述

当前 Tauri 2 + React 19 桌面应用的 AI 路由网关账号池尚未提供 Codex OAuth 登录的完整接入链路。项目已有 OAuth 会话、回调状态机、加密存储、稳定外部身份 upsert 与重新授权状态等基础能力，但 OAuth commands 未注册，typed IPC facade 与账号池入口缺失，真实 token 网络请求、claims 映射和刷新调度也未完成。

本规格将现有 Codex OAuth Authorization Code + PKCE S256 登录方案标准化，在账号池内补齐登录和生命周期管理，并保持随机 loopback、严格回调校验、本地加密及既有模块边界。Codex-Manager 当前使用的授权端点、token 端点、公开 client_id、scope 及必要兼容参数并非官方稳定的第三方契约，必须作为可替换配置管理。

任务 04 需要解决 `standards-index-sync-001`：新增 OAuth Tauri commands、运行时注册和 TypeScript typed facade 后，同步当前仍将 OAuth 描述为仅展示且未注册 IPC 的导航索引。任务 04 独占三份导航索引的写权限；任务 06 仅独立复验最终索引与代码的一致性。

## 目标与成功标准

- 用户可从 AI 路由网关账号池发起系统浏览器 OAuth 登录，无需独立登录页。
- 授权流程使用 Authorization Code + PKCE S256、一次性随机 loopback 和严格回调校验。
- 授权完成后，应用能安全交换 token、解析身份、加密落库并刷新账号列表。
- 账号以稳定主体去重并按 workspace 隔离；失效账号可在保留原账号配置的前提下重新授权。
- 凭据使用 RootKey + AES-256-GCM 加密保存，并支持 refresh token rotation 的原子更新。
- 路由使用前可刷新即将到期的 token；永久刷新失败进入重新授权状态并停止路由。
- Rust、IPC facade、React 前端及既有 API Key/Gmail OAuth 回归验证通过。
- `metadata.json` 成为唯一机器元数据来源，规划 Markdown 不包含顶部机器元数据或内嵌元数据 JSON。

## 用户与用户故事

- 桌面应用用户需要在账号池中添加 Codex OAuth 账号，查看等待、取消、超时和错误状态，并在自动回调失败时手动补交完整回调 URL。
- AI 路由网关请求路由方需要只使用有效凭据路由请求，并在可恢复的授权失败时执行受限刷新重试。
- 账号刷新与路由逻辑维护者需要安全读取和原子更新加密凭据，稳定去重账号，并确保敏感信息不进入日志、公开元数据、IPC DTO 或事件。

## 功能需求

1. 在 AI 路由网关账号池新增 OAuth 登录入口，与 API Key 入口并列，不创建独立登录页。
2. 提供集中、可替换的 Codex provider 配置，兼容 Codex-Manager 当前 authorization/token 端点、公开 client_id、scope 与必要兼容参数，并明确其非官方稳定契约风险。
3. 首阶段只实现系统浏览器 Authorization Code + PKCE S256；不实现非标准 Device Code，也不实现 API-key token exchange。
4. OAuth 授权使用一次性随机 loopback 端口和短期内存会话，并自动打开系统浏览器。
5. 前端提供等待、取消、超时和错误状态；自动回调失败时允许粘贴完整回调 URL 作为备用，但必须进入同一严格校验路径。
6. 回调成功后执行 code/token 交换、OIDC 身份解析、凭据加密持久化和账号列表刷新。
7. 注册既有但未注册的 Tauri OAuth commands，并以 typed IPC facade 向前端暴露所需操作和状态，不绕过现有模块边界。
8. OAuth 凭据、provider 端点与连接字段在账号管理界面只读；保留分组、标签、备注和启用状态等既有管理能力。
9. 退出登录时清除本地加密凭据并禁用账号；仅当存在可靠 revoke endpoint 时才执行远端撤销。
10. 保持 split 模式和既有 6 个 task_id、order、blocked_by；任务 04 独占三份导航索引写权限，任务 06 为纯验证任务。

## 非功能需求

- 原生应用不得复制、持有或依赖 client_secret 的保密性。
- 协议配置须允许未来替换，不把 Codex-Manager 当前常量固化为官方协议保证。
- 每项实现必须沿用现有 typed IPC、OAuth session、账号存储和加密模块边界。
- access token、refresh token、id_token、authorization code、PKCE verifier 和完整 callback URL 不得进入日志、公开 metadata、IPC DTO 或事件。
- 不修改 Gmail OAuth，不扩展 OAuth 账号模型映射或价格覆盖，不进行无关重构。
- OAuth 导航索引必须准确、可审计，且不得包含敏感信息。
- 任务 06 不修改生产实现或导航索引，其写入范围仅限验证所需的测试或报告边界。

## 范围

范围内包括 provider 配置、Authorization Code + PKCE S256、随机 loopback 与手动完整 callback、token 交换与 OIDC 身份映射、加密持久化、刷新轮换、重新授权与退出登录、Tauri commands、typed IPC、React 账号池 UI、本地化与测试，以及 spec、plan、tasks 和 `metadata.json` 的标准化。

导航索引当前内容作为现状基线，任务 04 的 OAuth IPC 接通作为目标状态。任务 04 负责同步 `.ai-work-flow/index/feature-navigation.md`、`.ai-work-flow/index/backend-navigation.md` 和 `.ai-work-flow/index/frontend-navigation.md`，并独占这三份索引的写权限；任务 06 只读复验这三份索引与最终代码的一致性。

任务 01 至 03 已完成并整合至 `integration`。任务 04 的提交 `83e0a36dfe7509cf51379a5d6e1e589ef6509cc9` 尚未整合，后续实施须保留并基于这些既有事实推进。

## 接口与数据

- OAuth 会话沿用现有 `OAuthSessionStore`、loopback PKCE/manual callback/device 状态机中的浏览器授权码与手动回调能力；本次不启用 Device Code 流程。
- 稳定身份优先使用 `chatgpt_account_id`；若有 `workspace_id`，以二者组合作为唯一身份，不同 workspace 视为不同账号。缺失时回退 OIDC `sub`。email 和 name 仅用于展示；无法取得可靠主体时拒绝落库。
- 账号写入沿用 `stable_external_id` upsert。重新登录以同一稳定身份更新原账号，而非创建重复账号。
- token、refresh token 和相关敏感凭据沿用现有 RootKey + AES-256-GCM 存储。敏感字段不得写入公开 metadata、IPC DTO、事件或日志。
- token 刷新成功时原子替换整组凭据，以支持 refresh token rotation，避免部分凭据更新。
- 导航索引应标注四个 OAuth command：`ai_routing_gateway_oauth_begin`、`ai_routing_gateway_oauth_complete`、`ai_routing_gateway_oauth_cancel` 与 `ai_routing_gateway_oauth_logout`；并准确关联 `src-tauri/src/app_runtime/run_app.rs` 的注册入口、`src-tauri/src/ai_routing_gateway/oauth.rs` 的领域入口，以及 `src/lib/aiRoutingGateway.ts` 的 typed facade 与 OAuth 事件订阅。

## 失败模式

- 回调必须严格校验 state、origin、path 和超时；任一不匹配、会话不存在或已过期时拒绝处理，不交换或落库凭据。
- 自动回调无法抵达时，手动完整回调 URL 仍须通过相同 state、origin、path、超时和一次性会话校验。
- 可验证 `id_token` 时校验 `exp`、`iss`、`aud` 与 `nonce`。无法获得可信 JWKS 时必须显式记录验证降级，且不得声称已完成完整 OIDC 验证。
- 无可靠稳定主体、token 交换失败、身份解析失败或加密持久化失败时，账号不得以成功状态落库，并向用户显示相应错误。
- API 授权失败时最多刷新一次并重试原请求一次。临时刷新错误按有上限的退避处理，避免无限重试。
- 永久刷新失效时标记 `oauth_reauthorization_required` 并停止该账号路由；保留账号配置以便原账号重新登录。
- 取消和超时须结束短期会话并清理等待状态，不遗留可复用的回调授权会话。

## 验收标准

1. 用户可从账号池发起 Codex OAuth，系统浏览器使用 Authorization Code + PKCE S256 完成授权。
2. 随机 loopback、短期内存会话以及 state、origin、path、超时校验均生效；不使用固定端口常驻回调。
3. 等待、取消、超时、错误和手动完整回调 URL 的主要前端交互均可用，手动回调不降低校验强度。
4. provider 端点、公开 client_id、scope 和兼容参数集中配置，可替换且明确标注 Codex-Manager 兼容风险。
5. 完成 token 交换、可信声明解析、稳定身份映射、加密落库与账号列表刷新；无可靠主体时拒绝保存。
6. `chatgpt_account_id` 与 `workspace_id` 去重和工作区隔离正确，`sub` 回退正确，email/name 不参与身份判定；同一稳定身份重新登录更新原账号。
7. 可取得可信 JWKS 时验证 `id_token` 的 `exp`、`iss`、`aud`、`nonce`；无法验证时记录可见降级状态。
8. token 使用 RootKey + AES-256-GCM 加密；日志、公开 metadata、IPC DTO 和事件中不包含敏感凭据、authorization code、PKCE verifier 或完整 callback URL。
9. 到期前刷新、refresh token rotation 原子替换、单次刷新重试、临时错误上限退避与永久失效的 `oauth_reauthorization_required` 状态均符合规则。
10. 失效账号停止路由但保留管理配置；退出登录清除加密凭据并禁用账号，仅在可靠 revoke endpoint 存在时远端撤销。
11. 已有未注册 OAuth commands 被接通，typed IPC facade、前端账号池入口和真实 token 网络层按模块边界工作。
12. 测试覆盖 PKCE/state、回调校验、token 交换与轮换、身份去重、加密存储、重新授权状态、IPC facade 和主要前端交互；API Key 与 Gmail OAuth 无回归。
13. `metadata.json` 是唯一机器元数据来源；`spec.md`、`plan.md` 和任务 Markdown 不保留顶部机器元数据，任务 Markdown 不保留内嵌元数据 JSON。
14. 任务 04 独占 `.ai-work-flow/index/feature-navigation.md`、`.ai-work-flow/index/backend-navigation.md` 和 `.ai-work-flow/index/frontend-navigation.md` 的写权限，并将当前导航基线同步至 OAuth IPC 已接通的目标状态。
15. 同步后的导航索引准确记录四个 OAuth commands、`src-tauri/src/app_runtime/run_app.rs` 注册入口、`src-tauri/src/ai_routing_gateway/oauth.rs` 领域入口、`src/lib/aiRoutingGateway.ts` typed facade 与 OAuth 事件订阅，且不包含敏感信息。
16. 任务 06 独立检查三份导航索引与最终代码一致；其写入范围仅为验证所需的测试或报告边界，不包含生产实现或任何导航索引路径，且不与任务 04 的索引写范围重叠。
17. split 模式及 6 个 task_id、order、blocked_by 保持不变；任务 01 至 03 的已完成事实和任务 04 的现有提交不得被重做、丢弃或重写。
18. spec、plan、任务 manifest、依赖和所有摘要经独立复验，OAuth 产品目标、规则和验收保持不变。

## 兼容性与迁移

- 不要求迁移 Gmail OAuth 或既有 API Key 账号。
- 新增 Codex OAuth 凭据使用现有 RootKey + AES-256-GCM 加密方案，不引入 client_secret。
- 账号稳定身份规则复用现有 upsert 机制；对缺少可靠主体的旧或新数据均不创建不安全映射。
- 旧完整 planning context JSON 已不可恢复。历史摘要 `257fdcca114afe27a94c3c594bcf3b16862d82b10b08f76667dd6a06b4f4112b` 仅作为迁移来源证据，不表示其字节内容等于当前经用户确认的标准化上下文。
- 当前权威标准化上下文由固定 `metadata.json` 的 `source_context` 绑定；其摘要按当前 `source_content` 原始字节计算。
- 既有 Git 实施事实必须保留：任务 01 至 03 已整合至 `integration`；任务 04 的提交 `83e0a36dfe7509cf51379a5d6e1e589ef6509cc9` 尚未整合。

## 范围外事项

- 非标准 Device Code。
- API-key token exchange。
- Gmail OAuth 的任何行为或实现修改。
- OAuth 账号模型映射、价格覆盖及无关重构。
- 任务 06 对生产实现或导航索引的修改。

## 假设

- Codex-Manager 当前公开 client_id、端点和参数可在目标环境用于发起兼容授权，但其可用性不构成官方稳定承诺。
- token 响应可提供 `id_token` 或等价声明，以支撑所需身份映射；没有可靠主体时拒绝落库。
- 现有 RootKey、AES-256-GCM 存储和 SQLite migration 能承载凭据组的原子更新。
- `.ai-work-flow/index/feature-navigation.md`、`.ai-work-flow/index/backend-navigation.md` 和 `.ai-work-flow/index/frontend-navigation.md` 构成所需的最小导航索引集合。
- 任务 04 的后续实施将在现有提交基础上完成 OAuth IPC 接通和导航同步。

## 开放问题

N/A
