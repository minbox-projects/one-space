# 03 - 接通加密账号持久化、刷新轮换与退出生命周期

- task_id: `codex-oauth-credential-lifecycle`
- order: `03`
- blocked_by: `codex-oauth-token-oidc-backend`
- source_plan: `../plan.md`
- source_plan_digest: `815daa835342a70583c59738ccaef385c69f9e7f6b54c2fec04cf2d093e79be7`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src-tauri/src/ai_routing_gateway/accounts.rs`
  - `src-tauri/src/ai_routing_gateway/oauth.rs`
  - `src-tauri/src/ai_routing_gateway/runtime.rs`
  - `src-tauri/src/ai_routing_gateway/commands/mod.rs`

## AI Work Flow Task Metadata

```json
{
  "plan_id": "codex-oauth-login",
  "plan_digest": "815daa835342a70583c59738ccaef385c69f9e7f6b54c2fec04cf2d093e79be7",
  "preview_revision": 2,
  "task_id": "codex-oauth-credential-lifecycle",
  "order": 3,
  "title": "接通加密账号持久化、刷新轮换与退出生命周期",
  "summary": "复用稳定外部身份 upsert 和 RootKey + AES-256-GCM 边界原子保存 OAuth 凭据，保证同一主体重新登录更新原账号且工作区隔离；在路由使用前完成到期刷新、refresh token rotation 整组替换、授权失败后最多一次刷新与一次原请求重试、临时错误有限退避及永久失效重新授权标记，并实现本地凭据清除、账号禁用和可靠端点下的可选远端撤销。",
  "blocked_by": [
    "codex-oauth-token-oidc-backend"
  ],
  "write_scope_mode": "exhaustive",
  "write_scope": [
    "src-tauri/src/ai_routing_gateway/accounts.rs",
    "src-tauri/src/ai_routing_gateway/oauth.rs",
    "src-tauri/src/ai_routing_gateway/runtime.rs",
    "src-tauri/src/ai_routing_gateway/commands/mod.rs"
  ],
  "acceptance": [
    "登录成功的 OAuth 凭据只以现有 AES-256-GCM schema 加密保存，数据库与公开 metadata 中不可检索到明文 token。",
    "相同稳定主体重复登录更新同一账号，不同 workspace 不互相覆盖，任一加密或事务失败不留下部分成功记录。",
    "路由在到期前刷新，rotation 以整组凭据原子替换；授权失败最多发生一次刷新和一次原请求重试。",
    "临时错误退避次数和时长有明确上限，永久错误停止账号参与路由并设置 `oauth_reauthorization_required`。",
    "退出登录始终清除本地凭据并禁用账号；没有可靠 revoke endpoint 或远端撤销失败时，本地结果仍成功提交。",
    "API Key 账号的凭据读取、路由重试和编辑行为未发生改变。"
  ]
}
```

## 预期结果

复用稳定外部身份 upsert 和 RootKey + AES-256-GCM 边界原子保存 OAuth 凭据，保证同一主体重新登录更新原账号且工作区隔离；在路由使用前完成到期刷新、refresh token rotation 整组替换、授权失败后最多一次刷新与一次原请求重试、临时错误有限退避及永久失效重新授权标记，并实现本地凭据清除、账号禁用和可靠端点下的可选远端撤销。

## 执行状态

已完成并整合至 `integration`，后续不得重新实施；保留以下技术验收作为既有实现的验证依据。

## 实施清单

- [ ] 复用 `upsert_oauth_account` 及 RootKey + AES-256-GCM 边界，在单个事务中按稳定外部身份创建或更新账号、加密 token bundle 和非敏感 provider metadata。
- [ ] 保证同一 `chatgpt_account_id`/workspace 身份重新登录更新原账号，不同 workspace 保持隔离，明文 token 不写入账号表或公开 metadata。
- [ ] 扩展 refresh material 与 token replacement 边界以支持 Codex provider，刷新成功时将 access token、轮换后的 refresh token、scope、token type 和过期时间整组原子替换；未返回新 refresh token 时按协议保留旧值。
- [ ] 在 OAuth 候选账号发送上游请求前检查到期时间并提前刷新；收到可恢复授权失败时最多强制刷新一次并重放原请求一次。
- [ ] 区分临时刷新错误与永久授权错误：临时错误采用有上限的退避且不无限重试，永久错误写入 `oauth_reauthorization_required`、停止账号路由并向管理面暴露重新授权状态。
- [ ] 实现退出登录服务：优先保证本地加密凭据清除和账号禁用；仅在 provider 配置了可靠 revoke endpoint 时尽力撤销远端 refresh/access token，远端失败不回滚本地退出。
- [ ] 保持 API Key 路由、Gmail OAuth 数据和现有 OAuth schema/价格覆盖不变。

## 验收标准

- [ ] 登录成功的 OAuth 凭据只以现有 AES-256-GCM schema 加密保存，数据库与公开 metadata 中不可检索到明文 token。
- [ ] 相同稳定主体重复登录更新同一账号，不同 workspace 不互相覆盖，任一加密或事务失败不留下部分成功记录。
- [ ] 路由在到期前刷新，rotation 以整组凭据原子替换；授权失败最多发生一次刷新和一次原请求重试。
- [ ] 临时错误退避次数和时长有明确上限，永久错误停止账号参与路由并设置 `oauth_reauthorization_required`。
- [ ] 退出登录始终清除本地凭据并禁用账号；没有可靠 revoke endpoint 或远端撤销失败时，本地结果仍成功提交。
- [ ] API Key 账号的凭据读取、路由重试和编辑行为未发生改变。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::accounts`，预期身份去重、workspace 隔离、密文无泄漏、rotation 与退出登录测试通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::runtime`，预期到期前刷新、单次授权重试、有限退避和永久重新授权测试通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期 API Key、存储、路由和网关全量 Rust 回归通过。

## 范围外事项

- 不新增数据库 schema，不迁移 Gmail OAuth 或既有 API Key 数据。
- 不负责 Tauri invoke 注册、typed IPC facade 或 React 授权界面。
