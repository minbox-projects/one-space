# 01 - 建立 Rust 短链接后端

- task_id: `rust-short-link-backend`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `bd1294e58912b10cedf6e2f834807758b2ebaf29b9f2f71d80a8caaa70a145e4`
- write_scope: `src-tauri/src/short_link.rs；src-tauri/src/secrets.rs（仅 TinyURL 专用 secret 能力）；src-tauri/src/lib.rs（仅模块声明）；src-tauri/src/app_runtime/run_app.rs（仅四个命令注册）`

## Outcome

Rust 后端能够安全保存 TinyURL Token、校验 URL、通过可测试的 HTTP 边界创建短链接，并向前端返回最小成功结果或稳定的结构化错误。

## Implementation Checklist

- [ ] 新增 `short_link` 模块，定义四个公开 Tauri 命令及计划规定的请求、响应和结构化错误类型。
- [ ] 使用固定 secret key `tinyurl_api_token`；状态命令仅返回 `{ configured }`，不得返回 Token。
- [ ] 保存 Token 前去除首尾空白并拒绝空值；保存和删除复用现有加密 secret store，存储故障映射为 `storage_error`。
- [ ] 创建命令从 secret store 内部读取 Token，不调用或暴露通用 `get_secret` 明文接口。
- [ ] 使用结构化 URL 解析，仅接受具有有效主机的 `http` 和 `https` URL。
- [ ] 使用现有 `reqwest` 发送带有限超时的 `POST /create`，Bearer Token 放入 Authorization header，请求 JSON 只包含 `url`。
- [ ] 生产环境固定调用 `https://api.tinyurl.com/create`；测试通过模块内部可替换 base URL/HTTP 客户端连接本地 mock server，不增加 provider 抽象。
- [ ] 仅从成功响应提取合法的 `data.tiny_url`，返回 `{ longUrl, shortUrl }`。
- [ ] 实现 `not_configured`、`invalid_url`、`authentication_failed`、`rate_limited`、`request_rejected`、`service_unavailable`、`network_error`、`invalid_response`、`storage_error` 映射。
- [ ] 清理错误详情和日志，禁止输出 Token、Authorization header、TinyURL 原始响应敏感数据或完整长链接。
- [ ] 在 Tauri runtime 注册四个命令。
- [ ] 使用本地 mock HTTP server 补齐 URL、凭据、请求契约、响应解析、状态码、超时、连接失败和安全回归测试。

## Acceptance Criteria

- [ ] `[SC-2/安全]` Token 只进入一次 IPC 和加密 secret store；配置状态、成功结果、结构化错误及日志均不包含 Token 明文。
- [ ] `[SC-3]` 空白、相对地址、无有效主机及 `javascript:`、`data:`、`file:` URL 在发出 HTTP 请求前返回 `invalid_url`。
- [ ] 合法 URL 产生带 Bearer header 且 body 仅为 `{ "url": "..." }` 的 `POST /create` 请求。
- [ ] 401/403、429、其他 4xx、5xx、超时/连接失败和畸形成功响应分别映射到计划指定的稳定错误代码。
- [ ] 缺少 Token 返回 `not_configured`；secret 状态、保存或删除失败返回 `storage_error`。
- [ ] Token 删除不触碰短链接历史或远端 TinyURL。
- [ ] `[SC-7/安全与错误]` 序列化错误和测试日志不包含测试 Token、Authorization header、完整长链接或未经清理的响应正文。
- [ ] 所有 HTTP 测试只访问本地 mock server，不访问真实 TinyURL。

## Verification Steps

- [ ] 在 `src-tauri` 运行 `cargo test short_link`，所有后端及 mock HTTP 测试通过。
- [ ] 在 `src-tauri` 运行 `cargo test`，确认现有 Rust 测试无回归。
- [ ] 在 `src-tauri` 运行 `cargo check`，确认命令注册、Serde 类型和模块引用有效。
- [ ] 检查测试输出及错误序列化断言，确认测试 Token 和完整测试长链接未泄露。

## Out of Scope

不实现自定义 alias、过期时间、统计、远端撤销、多供应商抽象、前端界面或真实 TinyURL 联网测试；不重构通用 secret API。
