# Codex OAuth 登录规格

## 规格元数据

- plan-id: `codex-oauth-login`
- status: `approved`
- source_context_id: `conversation-codex-oauth-login-20260809`
- source_context_digest: `257fdcca114afe27a94c3c594bcf3b16862d82b10b08f76667dd6a06b4f4112b`

## 问题陈述

当前 Tauri 2 + React 19 桌面应用的 AI 路由网关账号池尚未提供 Codex OAuth 登录的完整接入链路。项目已有 OAuth 会话、回调状态机、加密存储、稳定外部身份 upsert 与重新授权状态等基础能力，但 OAuth commands 未注册，typed IPC facade 与账号池入口缺失，真实 token 网络请求、claims 映射和刷新调度也未完成。

本规格在账号池内补齐 Codex OAuth 登录和生命周期管理，参考 Codex-Manager 当前使用的授权端点、token 端点、公开 client_id、scope 及必要兼容参数，同时保持本项目随机 loopback、严格回调校验和本地加密等安全边界。Codex-Manager 的参数并非官方稳定的第三方契约，必须作为可替换配置管理。

## 目标与成功标准

- 用户可从 AI 路由网关账号池发起系统浏览器 OAuth 登录，无需独立登录页。
- 授权完成后，应用能安全处理回调、交换 token、解析身份、加密落库并刷新账号列表。
- 账号以稳定主体去重并按工作区隔离；失效账号可在保留原账号配置的前提下重新登录。
- 路由使用前可刷新即将到期的 token，处理 rotation、临时错误与永久失效。
- 关键 Rust、IPC facade 和前端交互路径具备测试覆盖，既有 API Key 与 Gmail OAuth 行为不回归。

## 用户与用户故事

- 桌面应用用户需要在账号池中添加、等待完成、取消或手动补交 Codex OAuth 回调，并查看账号的可用或需重新授权状态。
- AI 路由网关需要只使用有效凭据路由请求，并在可恢复的授权失败时执行受限刷新重试。
- 账号刷新与路由逻辑需要安全读取和原子更新加密凭据，不暴露 token 到日志或公开元数据。

## 功能需求

1. 在 AI 路由网关账号池新增 OAuth 登录入口，与 API Key 入口并列，不创建独立登录页。
2. 提供集中、可替换的 Codex provider 配置，兼容 Codex-Manager 当前 authorization/token 端点、公开 client_id、scope 与必要兼容参数，并标注其非官方稳定契约风险。
3. 首阶段只实现系统浏览器 Authorization Code + PKCE S256；不实现非标准 Device Code，也不实现可选 API-key token exchange。
4. OAuth 授权使用一次性随机 loopback 端口和短期内存会话；自动打开系统浏览器。
5. 前端提供等待、取消、超时和错误状态；自动回调失败时允许粘贴完整回调 URL 作为备用，但必须进入同一严格校验路径。
6. 回调成功后执行 code/token 交换、身份解析、凭据加密持久化和账号列表刷新。
7. 注册既有但未注册的 Tauri OAuth commands，并以 typed IPC facade 向前端暴露所需操作和状态，不绕过现有模块边界。
8. OAuth 凭据、provider 端点与连接字段在账号管理界面只读；保留分组、标签、备注和启用状态等既有管理能力。
9. 退出登录时清除本地加密凭据并禁用账号；仅当存在可靠 revoke endpoint 时才执行远端撤销。

## 非功能需求

- 原生应用不得复制、持有或依赖 client_secret 的保密性。
- 协议配置须允许未来替换，不把 Codex-Manager 当前常量固化为官方协议保证。
- 每项实现必须沿用现有 typed IPC、OAuth 会话、账号存储和加密模块边界。
- 不修改 Gmail OAuth，不扩展 OAuth 账号模型映射或价格覆盖，不进行无关重构。

## 范围

包含 provider 配置、浏览器授权码 PKCE、随机 loopback 与手动完整回调、token 交换、OIDC 身份映射、AES-256-GCM 加密持久化、刷新轮换和重新登录、Tauri commands、typed IPC、账号池前端状态与相关测试。

## 接口与数据

- OAuth 会话沿用现有 `OAuthSessionStore`、loopback PKCE/manual callback/device 状态机中的浏览器授权码与手动回调能力；本次不启用 Device Code 流程。
- 稳定身份优先使用 `chatgpt_account_id`；若有 `workspace_id`，以二者组合作为唯一身份，不同工作区视为不同账号。缺失时回退 OIDC `sub`。email 和 name 仅用于展示；无法取得可靠主体时拒绝落库。
- 账号写入沿用 `stable_external_id` upsert。重新登录以同一稳定身份更新原账号，而非创建重复账号。
- token、refresh token 和相关敏感凭据沿用现有 RootKey + AES-256-GCM 存储。敏感字段不得写入公开 metadata 或日志。
- token 刷新成功时，原子替换整组凭据，以支持 refresh token rotation，避免部分凭据更新。

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
8. token 使用 RootKey + AES-256-GCM 加密；公开 metadata 和日志中不包含 token、refresh token 或其他敏感字段。
9. 到期前刷新、refresh token rotation 原子替换、单次刷新重试、临时错误上限退避与永久失效的 `oauth_reauthorization_required` 状态均符合规则。
10. 失效账号停止路由但保留管理配置；退出登录清除加密凭据并禁用账号，仅在可靠 revoke endpoint 存在时远端撤销。
11. 已有未注册 OAuth commands 被接通，typed IPC facade、前端账号池入口和真实 token 网络层按模块边界工作。
12. 测试覆盖 PKCE/state、回调校验、token 交换与轮换、身份去重、加密存储、重新授权状态、IPC facade 和主要前端交互；API Key 与 Gmail OAuth 无回归。

## 兼容性与迁移

- 不要求迁移 Gmail OAuth 或既有 API Key 账号。
- 新增 Codex OAuth 凭据必须使用现有加密方案，且不要求引入客户端 secret。
- 账号稳定身份规则应复用现有 upsert 机制；对缺少可靠主体的旧或新数据均不创建不安全映射。

## 范围外事项

- 非标准 Device Code。
- API-key token exchange。
- Gmail OAuth 的任何行为或实现修改。
- OAuth 账号模型映射、价格覆盖及无关重构。

## 假设

- Codex-Manager 当前公开 client_id、端点和参数可在目标环境用于发起兼容授权，但其可用性不构成官方稳定承诺。
- token 响应可提供 `id_token` 或等价声明，以支撑所需身份映射；没有可靠主体时将拒绝落库。
- 现有 RootKey、AES-GCM 存储和 SQLite migration 能承载凭据组的原子更新。

## 开放问题

N/A
