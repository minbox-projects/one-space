# CodexManager OAuth 登录实现研究

## 范围与方法

- 问题：分析 `qxcnm/Codex-Manager` 的 OAuth 登录实际实现，包括授权、PKCE/state、回调、令牌、账号与会话、错误与安全边界，以及可移植性。
- 结论依据：仓库 `main` 在提交 `63d44b24b22a4f422054eb03fbc76275d3aa0042` 的公开源码、仓库维护者文档，以及 IETF OAuth 标准。访问日期：2026-08-09；资料时点：2026-08-09。
- 结论边界：`auth.openai.com` 的端点、client ID、私有 scope、`originator`、`codex_cli_simplified_flow`、设备码 API 与 ID token 私有 claims 来自该项目源码，不应误当作 OpenAI 面向第三方发布的稳定 OAuth 合约。本次未找到 OpenAI 官方公开文档为这些私有字段背书。

## 结论摘要

项目是 Rust service 处理 OAuth、Tauri/Next 前端仅经本地 RPC 调用的桌面/自托管架构。浏览器路径采用 Authorization Code + PKCE S256；另有一条 Device Code 路径。登录会话、code verifier、账号与 token 均落入 SQLite。登录成功后，将当前鉴权账号设为新账号并触发用量刷新。刷新 token 有显式 RPC、定时调度字段与文档化的后台刷新行为，但本报告不把未在可核验片段中出现的 refresh 请求体字段或轮换语义当作事实。

## 1. 授权端点与流程

### 浏览器授权

1. `account/login/start` 接受 `chatgpt`、`chatgptDeviceCode`/`device`；桌面端 Tauri 命令只是把参数转发给 service RPC。浏览器路径先启动回调服务器，生成 state 和 PKCE，再把会话写入存储；可选地用系统默认浏览器打开 URL。
2. 默认 issuer 为 `https://auth.openai.com`，默认 client ID 为 `app_EMoamEEZ73f0CkXaXp7hrann`。授权 URL 是 `{issuer}/oauth/authorize`，包含：`response_type=code`、`client_id`、`redirect_uri`、`scope=openid profile email offline_access api.connectors.read api.connectors.invoke`、`code_challenge`、`code_challenge_method=S256`、`id_token_add_organizations=true`、`codex_cli_simplified_flow=true`、`state`、`originator`；调用方给出 workspace 时还带 `allowed_workspace_id`。
3. 回调收到 code 后，service 向 `{issuer}/oauth/token` 发 `application/x-www-form-urlencoded` 请求：`grant_type=authorization_code`、`code`、`redirect_uri`、`client_id`、`code_verifier`。响应模型要求 `id_token`、`access_token`、`refresh_token`。
4. 该实现还尝试用 id token 对同一 `/oauth/token` 进行 RFC 8693 风格的扩展 token exchange，`grant_type=urn:ietf:params:oauth:grant-type:token-exchange`，`requested_token=openai-api-key`，`subject_token_type=urn:ietf:params:oauth:token-type:id_token`；失败被 `.ok()` 忽略，不阻断登录。它保存返回的 `api_key_access_token`，但此字段是项目的上游兼容细节。

源码依据：

- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/core/src/auth/mod.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_login.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_tokens.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/apps/src-tauri/src/commands/login.rs

这与 OAuth Authorization Code 的端点角色相符；授权码不会经浏览器携带 access token，PKCE 将 verifier 在 token 请求中呈现的模式也符合 RFC 7636。

- https://www.rfc-editor.org/rfc/rfc6749.html#section-4.1
- https://www.rfc-editor.org/rfc/rfc7636.html#section-4

### Device Code

Device 路径先 `POST {issuer}/api/accounts/deviceauth/usercode`，JSON 仅含 `client_id`；服务返回 `device_auth_id`、`user_code`、可选 `interval`。UI 显示的验证地址固定构造为 `{issuer}/codex/device`。后台线程随后轮询 `POST {issuer}/api/accounts/deviceauth/token`，携带 `device_auth_id`、`user_code`；成功响应提供 `authorization_code`、`code_verifier`。它将 verifier 原子写入仍为 pending 的登录会话，再以 `redirect_uri={issuer}/deviceauth/callback` 走上述授权码兑换。403/404 在 15 分钟窗口内继续轮询，其他非成功状态失败。

这不是 RFC 8628 标准端点/字段名的直接实现，而是当前 issuer 的私有协议适配；不能复制端点和字段去对接其他 OAuth 提供者。

源码依据：

- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/core/src/auth/mod.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_login.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_tokens.rs

## 2. PKCE、state 与回调

- PKCE：`generate_pkce` 用 `rand::thread_rng` 生成 64 字节随机数，以无 padding Base64url 作为 verifier，对 verifier 的 ASCII 字节取 SHA-256 并 Base64url 编码为 challenge，明确传 `S256`。64 字节随机输入产生 86 字符 verifier，满足 RFC 7636 的 43-128 字符约束。
- State：`generate_state` 用 32 随机字节 Base64url 编码。浏览器登录的 `login_id=state`，并将同值写入 `login_sessions.login_id` 与 `login_sessions.state`。回调要求非空 state，先确认同名会话存在，再完成兑换；未知或过期会话归为 “State mismatch or expired login session”。这提供请求与回调的绑定。
- 回调监听：默认 `localhost:1455`，路径仅接受 `/auth/callback`。`localhost` 会同时尝试 `127.0.0.1` 与 `[::1]`，以缓解浏览器 IPv4/IPv6 选择差异；非 loopback 地址默认被拒绝，只有 `CODEXMANAGER_ALLOW_NON_LOOPBACK_LOGIN_ADDR` 的明确真值才允许。监听端口占用会报错。`CODEXMANAGER_REDIRECT_URI` 可以覆盖回调 URI，并尝试按其 host/port 起服务。
- 回调错误：优先处理 `error` 和可选 `error_description`；对 `access_denied` 且描述含 `missing_codex_entitlement` 给出 workspace 未启用 Codex 的特定提示。缺 state、缺 code、状态不为 pending、存储不可用、兑换失败都会结束或拒绝会话。成功页/失败页均是本地 HTML，失败文本做 `&`、`<`、`>` 转义。
- 生命周期与并发：pending 15 分钟过期，`completing` 5 分钟无进展视为陈旧；完成前通过 `claim_login_session_for_completion` 领取会话，防止重复完成。取消会话可取消设备轮询任务。

源码依据：

- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/core/src/auth/mod.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_callback.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_login.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/core/migrations/002_login_sessions.sql

外部浏览器、loopback 回调、双栈尝试、PKCE 与高熵 state 都与 native-app OAuth BCP 一致。差异是该项目的默认 URI 使用 `localhost`、固定 1455 端口和常驻 server；RFC 8252 更建议回环 IP literal、临时端口，并在收到响应后关闭端口。因此这是安全改进空间，而非该实现已满足的结论。

- https://www.rfc-editor.org/rfc/rfc8252.html#section-6
- https://www.rfc-editor.org/rfc/rfc8252.html#section-7.3
- https://www.rfc-editor.org/rfc/rfc8252.html#section-8.1
- https://www.rfc-editor.org/rfc/rfc8252.html#section-8.3
- https://www.rfc-editor.org/rfc/rfc8252.html#section-8.9

## 3. Token、刷新与存储

### 持久化与刷新

- 根 README 明确桌面默认数据库为应用数据目录的 `codexmanager.db`；service 可由 `CODEXMANAGER_DB_PATH` 指定。初始迁移显示 SQLite `tokens` 表将 `id_token`、`access_token`、`refresh_token` 作为 `TEXT NOT NULL` 保存，并以 `account_id` 为主键；当前登录还保存 `api_key_access_token`。资料未显示这些 OAuth token 在 SQLite 落盘前进行了字段级加密或使用系统密钥链，因此不能声称“加密存储”。
- 调度迁移添加 `access_token_exp`、`next_refresh_at`、`last_refresh_attempt_at` 及 `next_refresh_at` 索引。桌面命令暴露 `account/chatgptAuthTokens/refresh` 和 `refreshAll`；配置文档列出 `CODEXMANAGER_TOKEN_REFRESH_POLLING_ENABLED` 和 `CODEXMANAGER_TOKEN_REFRESH_POLL_INTERVAL_SECS`，并明确 refresh token 请求会走配置的 OpenAI 上游代理。
- 文档还规定：刷新时若收到 `unsupported_country_region_territory`，service 会将账号标为 `refresh_token_region_blocked` 并暂停自动刷新，代理修正后需要人工恢复。该行为是仓库维护者文档描述；本报告未验证完整状态迁移代码，因此不推断触发阈值、token rotation 或 refresh 请求的全部表单参数。

依据：

- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/README.md
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/core/migrations/001_init.sql
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/core/migrations/025_tokens_refresh_schedule.sql
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/apps/src-tauri/src/commands/login.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/docs/zh-CN/report/%E7%8E%AF%E5%A2%83%E5%8F%98%E9%87%8F%E4%B8%8E%E8%BF%90%E8%A1%8C%E9%85%8D%E7%BD%AE%E8%AF%B4%E6%98%8E.md

RFC 6749 将 refresh token 定义为仅交给授权服务器以换取新 access token 的凭证，不应发送给资源服务器；因此本地数据库、日志和备份均应按高敏感凭证治理。

- https://www.rfc-editor.org/rfc/rfc6749.html#section-1.5
- https://www.rfc-editor.org/rfc/rfc6749.html#section-6

### 账号标识与会话衔接

- `id_token` 被解码为 claims，读取 `sub`、email/profile email、`workspace_id` 与 `https://api.openai.com/auth` 命名空间的 `chatgpt_account_id`、plan、user id。代码仅 Base64url 解码 payload 并反序列化，所示函数没有验 JWT 签名、issuer、audience 或 nonce；安全性依赖 token 由 HTTPS token endpoint 获取，而不是该解析器独立完成验签。
- `sub` 是 subject account ID，email（若无则 sub）成为 label。ChatGPT account/workspace ID 优先取 claims、再从 id/access token 提取并规范化。若登录开始时指定 workspace，`ensure_workspace_allowed` 会把预期 workspace 或 ChatGPT account ID 与 token 所得标识比对；缺 claim 或不匹配会失败。
- 存储 ID 由 subject、ChatGPT account、workspace 和 tags 构建；先查询既有相同身份组合，避免重复账号。随后 upsert account、写 subject identity/metadata 和 token；会话置 success；`set_current_auth_account_id(Some(account_key))`、`set_current_auth_mode(Some("chatgpt"))` 将服务当前鉴权指向新账号，并投递用量刷新。
- 网关会话层另有“同一会话只绑定一个活动账号”的 Codex-First 目标说明；它是登录后的网关路由语义，不能与 OAuth 的 browser session 混同。

依据：

- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/core/src/auth/mod.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_tokens.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/docs/zh-CN/report/FAQ%E4%B8%8E%E8%B4%A6%E5%8F%B7%E5%91%BD%E4%B8%AD%E8%A7%84%E5%88%99.md

## 4. 错误处理与安全边界

| 范畴 | 已实现的边界 | 未证明或风险 |
| --- | --- | --- |
| 授权码截获 | S256 PKCE、state 与 pending 会话匹配、单次领取完成 | 固定默认端口/`localhost` 和常驻 listener 比临时 loopback IP listener 更宽；其他本机进程仍可访问 loopback 回调并尝试竞态。 |
| 回调暴露 | 仅 `/auth/callback`；默认拒绝非回环监听；错误页转义 | `CODEXMANAGER_ALLOW_NON_LOOPBACK_LOGIN_ADDR` 会显式放宽边界，部署者须承担暴露风险。 |
| 敏感信息 | token endpoint 错误 URL 会脱敏 `code`、verifier、state、各 token 等 query 项；安全文档要求不得把 token/cookie/key 提交或贴入日志 | 成功登录 info 日志含数据库路径、login/account/workspace/ChatGPT account ID 和 redirect URI；虽非 token，仍属于可关联身份元数据。数据库备份也含明文 token 列的风险。 |
| 网络错误 | connect 15 秒、读取 30 秒、总请求 60 秒；非成功 token 响应解析 JSON/HTML，并带 request ID、CF Ray、授权错误头等诊断；Cloudflare 挑战/地区阻断分类 | 错误文本可能来自上游；虽 URL 已脱敏，调用方仍应避免原样传播到不可信日志或 UI。 |
| issuer/client 配置 | issuer 与 client ID 可被环境变量覆盖，便于测试或自托管 | 环境覆写意味着部署控制面是信任边界；若攻击者能写 env，可改为攻击者 issuer 并收集 token。 |

依据：

- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_callback.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_tokens.rs
- https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/SECURITY.md

## 5. 可移植与栈绑定

| 可移植的设计 | 与本项目绑定的实现 |
| --- | --- |
| Authorization Code + PKCE S256；每次登录生成高熵 verifier/state；state 关联短命服务端会话；回调先验 state 再兑换；失败、取消、超时成为显式终态。 | `auth.openai.com` URL、client ID、scope、originator、Codex flags、workspace 私有 claims、API-key token exchange、Device Code 端点/字段。 |
| 默认外部浏览器；回调仅 loopback；优先双栈；端口占用诊断；给用户手动粘贴回调的替代路径。 | Rust `tiny_http`、`reqwest`、Tokio 单独 auth runtime、`webbrowser`、Tauri command/RPC、SQLite migration/storage API、环境变量名。 |
| token 与账号用稳定 subject + provider account/workspace 的复合身份去重；将 refresh 排程字段与业务账户状态分开；对诊断 URL 的机密 query 脱敏。 | 该项目的账户 ID 拼接规则、tags 对身份的影响、当前 auth account 全局指针、Codex-First 会话绑定和上游网关策略。 |

迁移建议：以 provider 的 OIDC discovery/注册资料替换任何硬编码端点和 client 参数；把 callback URI、PKCE verifier、state、issuer、nonce（若使用 OIDC）一并保存并严格比对；为 token 采用操作系统安全存储或经过密钥管理的加密数据库；将监听器限定为一次性、临时端口、IP literal 的 loopback。不要复制 OpenAI/Codex 的私有 scope、client ID 或设备码字段。

## 引用清单

1. https://api.github.com/repos/qxcnm/Codex-Manager/commits/63d44b24b22a4f422054eb03fbc76275d3aa0042
2. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/core/src/auth/mod.rs
3. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_login.rs
4. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_callback.rs
5. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/service/src/auth/auth_tokens.rs
6. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/apps/src-tauri/src/commands/login.rs
7. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/core/migrations/001_init.sql
8. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/core/migrations/002_login_sessions.sql
9. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/crates/core/migrations/025_tokens_refresh_schedule.sql
10. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/README.md
11. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/SECURITY.md
12. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/docs/zh-CN/report/%E7%8E%AF%E5%A2%83%E5%8F%98%E9%87%8F%E4%B8%8E%E8%BF%90%E8%A1%8C%E9%85%8D%E7%BD%AE%E8%AF%B4%E6%98%8E.md
13. https://github.com/qxcnm/Codex-Manager/blob/63d44b24b22a4f422054eb03fbc76275d3aa0042/docs/zh-CN/report/FAQ%E4%B8%8E%E8%B4%A6%E5%8F%B7%E5%91%BD%E4%B8%AD%E8%A7%84%E5%88%99.md
14. https://www.rfc-editor.org/rfc/rfc6749.html
15. https://www.rfc-editor.org/rfc/rfc7636.html
16. https://www.rfc-editor.org/rfc/rfc8252.html
