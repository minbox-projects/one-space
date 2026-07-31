# 06 - 完成跨层整合与完整回归

- task_id: `short-link-full-regression`
- order: `06`
- blocked_by: `rust-short-link-backend, short-link-client-history, short-link-navigation-contracts, short-link-tool-ui, short-link-shell-integration`
- source_plan: `../plan.md`
- source_plan_digest: `bd1294e58912b10cedf6e2f834807758b2ebaf29b9f2f71d80a8caaa70a145e4`
- write_scope: `仅允许修正任务 01-05 所列文件中的跨层契约或回归问题；不得新增功能、依赖、公共接口或扩大文件范围`

## Outcome

前端、Tauri 命令、secret 存储、TinyURL 客户端、历史和导航形成一致的可发布流程，所有聚焦测试、全量检查及安全回归通过。

## Implementation Checklist

- [x] 核对四个 Tauri 命令的名称、参数、camelCase 成功响应和 `{ code, message? }` 错误契约在 Rust、前端包装器及测试 mock 中完全一致。
- [x] 核对稳定 ID `short-link` 在导航、展示、Launcher、可见性、App 和测试中的一致性。
- [x] 核对历史 key、schema、ISO 时间、50 条限制及本地删除语义在存储模块和组件中一致。
- [x] 核对九个稳定错误代码均有 Rust 映射、前端分支和中英文文案。
- [x] 审查 Token 数据流，确认不存在读取回前端、持久化到 localStorage、日志输出或错误泄漏路径。
- [x] 审查测试网络目标，确认自动化测试只使用本地 mock server，不访问 TinyURL 生产 API。
- [x] 先运行新增聚焦测试，再运行全量前端测试、Lint、构建、Rust 测试和 Rust 检查。
- [x] 仅修复验证发现的跨层契约或回归问题，不重复实现前置任务的核心功能。

## Acceptance Criteria

- [x] `[SC-1]` More Tools 与 Launcher 均可打开工具，返回、标题、面包屑及可见性设置符合现有行为。
- [x] `[SC-2]` Token 可安全保存、替换、删除并默认遮罩；后端不向前端返回已保存明文。
- [x] `[SC-3]` 仅合法 HTTP(S) URL 能触发 TinyURL 请求，成功结果正确展示。
- [x] `[SC-4]` 重复提交被阻止，输入和成功结果按计划保留，复制反馈明确。
- [x] `[SC-5]` 最近 50 条成功记录按时间倒序持久化，重载、复制、删除和确认清空均有效。
- [x] `[SC-6]` 本地历史操作不调用远端删除接口，且文案不暗示远端链接失效。
- [x] `[SC-7]` 凭据、限流、拒绝、服务、网络、响应、剪贴板和历史故障可区分且不泄露 Token。
- [x] `[SC-8]` 前端核心交互与历史、Rust URL/HTTP/错误映射、导航回归、TypeScript 构建、Lint、Rust 测试及检查全部通过。
- [x] 自动化验证未使用真实 TinyURL Token，也未访问 `https://api.tinyurl.com/create`。
- [x] 未增加前端依赖、数据库迁移、CSP 变更、供应商抽象或计划外功能。

## Verification Steps

- [x] 运行 `npm run test -- src/lib/shortLink.test.ts src/lib/shortLinkHistory.test.ts src/components/ShortLinkTool.test.tsx`。
- [x] 运行 `npm run test -- src/components/MoreToolsHub.test.tsx src/components/Launcher.test.tsx src/App.moreToolsNavigation.test.tsx src/lib/launcherToolVisibility.test.ts`。
- [x] 在 `src-tauri` 运行 `cargo test short_link`。
- [x] 在仓库根目录运行 `npm run test`。
- [x] 在仓库根目录运行 `npm run lint`。
- [x] 在仓库根目录运行 `npm run build`。
- [x] 在 `src-tauri` 运行 `cargo test`。
- [x] 在 `src-tauri` 运行 `cargo check`。
- [x] 检查完整测试输出，确认无真实 TinyURL 请求、Token、Authorization header 或完整敏感长链接。

## Verification Evidence

### 验证上下文

- 固定验证基点：`0e2ed2542386e7647e38fcfd0e8d8fba01793198`。
- 本次修复基点/旧 review commit：`56ec18495587f7bb9a1b0ee4edad3698a5311ece`；开始时 `HEAD` 精确等于该 SHA，`git status --porcelain=v2 -z --untracked-files=all` 输出为 0 字节。
- 执行时间与环境：`2026-07-31 21:11-21:14 CST`，macOS `26.5.2`、Apple Silicon `arm64`、Node `v24.18.0`、npm `11.16.0`、rustc/cargo `1.93.1`。
- 前端命令工作目录均为仓库根目录；Rust 命令工作目录均为 `src-tauri`。未启动浏览器或 Playwright，未手工或自动访问 TinyURL；Rust 网络测试使用 `127.0.0.1:0` 本地 mock server。

### Implementation Checklist 逐项证据

| # | 对应实现项 | 可复核证据 |
|---|---|---|
| 1 | 四个 Tauri 命令与跨层契约 | `src-tauri/src/short_link.rs` 的 `short_link_config_status`、`short_link_save_token(token)`、`short_link_delete_token`、`short_link_create(url)` 及 `ShortLinkConfigStatus`/`ShortLinkResult`/`ShortLinkError`；`src/lib/shortLink.ts` 的四个同名 IPC 包装器；`src/test/mocks/tauri.ts` 的四命令 mock。`src/lib/shortLink.test.ts` 断言命令名、`{ token }`/`{ url }` 参数、`configured`/`longUrl`/`shortUrl` 最小 camelCase 响应及 `{ code, message? }` 归一化。命令 1 的 11 个包装器测试包含这些断言并通过。 |
| 2 | 稳定 ID `short-link` | `src/lib/navigation.ts` 的详情 ID 解析、`src/App.tsx` 的标题/面包屑、`src/components/MoreToolsHub.tsx` 与 `src/components/Launcher.tsx` 的入口、`src/lib/launcherToolVisibility.ts` 的默认值/持久化；对应 `MoreToolsHub.test.tsx`、`Launcher.test.tsx`、`App.moreToolsNavigation.test.tsx`、`launcherToolVisibility.test.ts` 覆盖展示、搜索、打开、返回和可见性。命令 2 共 63 个测试通过。 |
| 3 | 历史 key/schema/时间/50 条/本地语义 | `src/lib/shortLinkHistory.ts` 的 `SHORT_LINK_HISTORY_KEY`、`SHORT_LINK_HISTORY_LIMIT`、`ShortLinkHistoryRecord`、`isValidIso8601`、`newestFirst`、`loadShortLinkHistory`、`addShortLinkHistory`、`deleteShortLinkHistory`、`clearShortLinkHistory`；`src/lib/shortLinkHistory.test.ts` 断言 UUID、ISO 时间、严格 schema、倒序、52→50、损坏恢复、删除/清空及失败语义；`ShortLinkTool.test.tsx` 的 `reloads at most 50 newest records...` 覆盖 UI 重载/复制/删除/确认清空。命令 1 的历史 18 项和 UI 26 项通过。 |
| 4 | 九个稳定错误码 | `src-tauri/src/short_link.rs` 的 `ShortLinkErrorCode`、`validate_http_url`、`map_status`、HTTP/存储/网络/响应映射；`src/lib/shortLink.ts` 的 `SHORT_LINK_ERROR_CODES` 与 `normalizeError`；`src/components/ShortLinkTool.tsx` 的错误分支；`src/i18n.ts` 的 `shortLinkError_*` 英文与中文键。`ShortLinkTool.test.tsx` 的 `maps backend error %s by code without exposing message` 逐项断言 `not_configured`、`invalid_url`、`authentication_failed`、`rate_limited`、`request_rejected`、`service_unavailable`、`network_error`、`invalid_response`、`storage_error`；Rust 的状态/网络/响应测试覆盖后端映射。命令 1、3、7 均通过。 |
| 5 | Token 数据流与保留键隔离 | `src-tauri/src/secrets.rs` 的 `TINYURL_API_TOKEN_KEY` 及专用 get/save/delete；`get_secret_command_cannot_read_tinyurl_token`、`save_secret_command_cannot_overwrite_tinyurl_token`、`delete_secret_command_cannot_delete_tinyurl_token`、`tinyurl_dedicated_commands_still_manage_reserved_token` 证明 generic secret 读/写/删均被拒且专用命令仍可管理。`short_link_config_status` 只返回 `configured`；`ShortLinkTool.test.tsx` 的 `loads credential status...without exposing a returned token` 与 `saves, replaces, masks...` 证明前端不读回/渲染明文。短链历史模块只持久化 URL/时间/id，无 Token；Rust 序列化断言排除 Token、长 URL 和 Authorization。命令 1、7 通过。 |
| 6 | 测试网络边界 | `src-tauri/src/short_link.rs` 的 `spawn_mock` 绑定 `TcpListener::bind("127.0.0.1:0")`，测试经 `create_with_dependencies(..., &endpoint)` 注入本地 endpoint；生产 `https://api.tinyurl.com/create` 只由 `TINYURL_CREATE_URL` 常量传入正式 `short_link_create`。命令 3/7 的短链网络测试均使用本地 mock；安全扫描仅发现该 1 处生产常量。 |
| 7 | 聚焦到全量验证顺序 | 本次按 Verification Steps 顺序实际执行 8 条命令：聚焦前端 55、导航 63、聚焦 Rust 10、全量前端 260、Lint、构建、全量 Rust 356、`cargo check`；退出状态均为 0，详细计数见下表。 |
| 8 | 仅处理跨层回归 | `git diff --name-status 0e2ed2542386e7647e38fcfd0e8d8fba01793198 56ec18495587f7bb9a1b0ee4edad3698a5311ece --` 仅返回本 task Markdown；固定验证基点到旧 review 没有源码、测试、索引、配置或依赖变化。本次修复也只追加此证据区段。 |

### Acceptance Criteria 逐项证据

| # | 验收项 | 测试场景与命令结果 |
|---|---|---|
| 1 | `[SC-1]` 入口与导航 | `MoreToolsHub.test.tsx` 的短链卡片展示/选择，`Launcher.test.tsx` 的中英文搜索/打开，`App.moreToolsNavigation.test.tsx` 的标题/面包屑/返回，`launcherToolVisibility.test.ts` 的默认及开关持久化；命令 2：4 文件、63/63 通过。 |
| 2 | `[SC-2]` Token 生命周期与遮罩 | `ShortLinkTool.test.tsx` 的 `saves, replaces, masks, clears, and deletes...`、`loads credential status...without exposing...`；`src-tauri/src/secrets.rs` 的 4 个保留键/专用命令测试；命令 1 的 UI 场景与命令 7 的 Rust 场景通过。响应仅有 `{ configured }`。 |
| 3 | `[SC-3]` URL 校验与成功展示 | `ShortLinkTool.test.tsx` 的 `rejects invalid URL %j without create IPC`、`accepts an HTTP URL with a host...`；Rust 的 `short_link_url_validation_accepts_only_absolute_http_urls_with_hosts`、`short_link_invalid_url_fails_before_credentials_or_http_are_used`、`short_link_create_sends_minimal_authenticated_post_and_returns_minimal_result`；命令 1、3、7 通过。 |
| 4 | `[SC-4]` 防重复、状态保留、复制反馈 | `ShortLinkTool.test.tsx` 的 `prevents duplicate creates, preserves both URLs...`、`keeps a successful result copyable...`、`reports clipboard failure...`；命令 1：对应 UI 文件 26/26 通过。 |
| 5 | `[SC-5]` 最近 50 条历史 | `shortLinkHistory.test.ts` 的 UUID/ISO/重载倒序/52→50/删除清空，`ShortLinkTool.test.tsx` 的 `reloads at most 50 newest records...`；命令 1：历史 18/18、UI 26/26 通过。 |
| 6 | `[SC-6]` 本地删除无远端语义 | `shortLinkHistory.test.ts` 的 `删除单条和清空只修改历史 key，不调用 Tauri` 明确断言 `invokeMock` 未调用；UI 场景断言删除/清空提示包含远端 TinyURL 仍有效，且除配置状态外 IPC 调用为空；命令 1 通过。 |
| 7 | `[SC-7]` 故障分类且不泄密 | Rust 的 HTTP 状态、缺凭据/存储、超时/连接、畸形响应及敏感错误序列化测试；前端九码参数化文案测试、剪贴板失败场景；历史 `read_failed`/`cleanup_failed`/`write_failed` 场景。命令 1、3、7 通过，敏感字段断言通过。 |
| 8 | `[SC-8]` 全链路回归 | 下表 8 条命令全部退出 0：前端 260/260、Lint 0 errors、构建成功、Rust 356/356（另 2 个无关本地环境 smoke test 按定义忽略）、`cargo check` 成功。 |
| 9 | 无真实 Token/生产 TinyURL 访问 | 测试值仅为 `local-mock-sensitive-token`、`test-token-placeholder` 等显式占位符；Rust 测试只绑定 `127.0.0.1:0` 并注入 endpoint。生产 endpoint 精确扫描仅命中正式常量 1 次；测试调用点均调用可注入的 `create_with_dependencies`，没有调用正式 `short_link_create` 触发生产常量。命令 3/7 与安全扫描退出 0。 |
| 10 | 无计划外依赖/CSP/迁移/provider | 固定验证基点到旧 review 的 `git diff --name-status` 仅含本 task Markdown；因此未变更 `package.json`/锁文件、Tauri/CSP 配置、迁移或 provider 路径。`npm run build` 与 `cargo check` 通过，未出现新增依赖或配置要求。 |

### Verification Steps 本次重跑结果

| # | 命令（工作目录） | 退出状态 | 通过/忽略/警告 |
|---|---|---:|---|
| 1 | `npm run test -- src/lib/shortLink.test.ts src/lib/shortLinkHistory.test.ts src/components/ShortLinkTool.test.tsx`（`.`） | 0 | 3 文件；55 passed，0 failed，0 ignored，0 warnings。 |
| 2 | `npm run test -- src/components/MoreToolsHub.test.tsx src/components/Launcher.test.tsx src/App.moreToolsNavigation.test.tsx src/lib/launcherToolVisibility.test.ts`（`.`） | 0 | 4 文件；63 passed，0 failed，0 ignored，0 warnings。 |
| 3 | `cargo test short_link`（`src-tauri`） | 0 | `src/lib.rs`：10 passed，0 failed，0 ignored，348 filtered out；`src/main.rs`：0 tests。filtered out 是名称过滤，不是忽略或失败。 |
| 4 | `npm run test`（`.`） | 0 | 30 文件；260 passed，0 failed，0 ignored，0 warnings。 |
| 5 | `npm run lint`（`.`） | 0 | 0 errors，386 warnings。相对固定验证基点属于既有基线：`0e2ed254...56ec184` 只有 Markdown 差异，没有 ESLint 输入变化。专用短链文件 `ShortLinkTool*`、`shortLink*`、`shortLinkHistory*`、`src/test/mocks/tauri.ts` 未命中 warning；共享集成文件 `src/App.tsx` 与 `src/components/Launcher.tsx` 有既有 warning，但行号均不在短链入口/导航改动处。warnings 不触发非零退出，且聚焦/全量测试与构建通过，因此不阻塞本次验收。 |
| 6 | `npm run build`（`.`） | 0 | TypeScript 与 Vite 构建通过，2590 modules transformed；0 errors，1 个 chunk-size warning（主 chunk 1563.09 kB，高于 1450 kB 阈值），与短链契约无关且不阻塞产物生成。 |
| 7 | `cargo test`（`src-tauri`） | 0 | `src/lib.rs`：356 passed，0 failed，2 ignored；`src/main.rs`/doc-tests：0 tests。忽略项精确为 `ai_sessions::tests::test_local_claude_binding`、`ai_sessions::tests::test_local_gemini_binding`，Cargo 原因均为 `local environment smoke test`；二者依赖本机 CLI/会话环境且不属于短链、secret 或导航链路，短链 10 项和 generic secret 保留键 4 项均实际通过，故不影响本次验收。 |
| 8 | `cargo check`（`src-tauri`） | 0 | dev profile 完成；0 errors，0 warnings，0 ignored。 |
| 9 | 下列安全扫描（`.`） | 0 | 扫描结果见下一节；未发起网络请求。 |

### 安全与差异审计

- 扫描对象：`src/lib/shortLink.ts`、`src/lib/shortLink.test.ts`、`src/lib/shortLinkHistory.ts`、`src/lib/shortLinkHistory.test.ts`、`src/components/ShortLinkTool.tsx`、`src/components/ShortLinkTool.test.tsx`、`src/test/mocks/tauri.ts`、`src-tauri/src/short_link.rs`、`src-tauri/src/secrets.rs`，以及生产 endpoint 的 `src`/`src-tauri/src` 全域固定字符串扫描。
- 生产 endpoint：`rg -n --fixed-strings 'https://api.tinyurl.com/create' src src-tauri/src` 退出 0，仅命中 `src-tauri/src/short_link.rs` 的 `TINYURL_CREATE_URL` 生产常量 1 次。测试侧 `spawn_mock` 绑定 `127.0.0.1:0`，并把本地 URL 注入 `create_with_dependencies`；因此“存在生产常量”不等于“测试访问生产网络”。
- Token/认证模式：对上述对象执行 `rg -n -i '(authorization|bearer|local-mock-sensitive-token|test-token-placeholder|tinyurl_api_token)' ...`，仅命中专用保留键、正式 `.bearer_auth`、本地 mock 捕获/排泄漏断言及显式测试占位 Token；未发现真实 Token。Rust 测试断言请求头精确为本地占位值生成的 `Bearer`，并断言序列化结果不含 Token、长 URL 或 `authorization`。
- 完整敏感长 URL：对上述对象执行 `rg -n -P 'https?://\S{80,}' ...`，结果 `NO_MATCH`。已知测试敏感 URL 为假域名 `https://example.test/private/path?secret=query-value`，仅用于本地 mock 请求及错误序列化排除断言，不是生产目标；测试输出未打印该值。
- generic secret 保留键：`tinyurl_api_token` 的 generic 读、写、删隔离测试和 dedicated 管理测试均包含在全量 Rust 的 356 passed 中；错误还断言不包含保留键及测试 Token。
- 九错误码端到端：Rust 枚举/映射测试、前端 `SHORT_LINK_ERROR_CODES`/`normalizeError`、UI 九码参数化场景及 `src/i18n.ts` 中英文 `shortLinkError_*` 键形成逐码闭环；命令 1、3、7 全部通过。
- 本地历史删除无远端 IPC：`deleteShortLinkHistory`/`clearShortLinkHistory` 只读写 `SHORT_LINK_HISTORY_KEY`；两组测试均断言 `invokeMock` 未调用，UI 还断言远端链接仍有效的中英文语义；命令 1 通过。
- 依赖/CSP/迁移/provider：`git diff --name-status 0e2ed2542386e7647e38fcfd0e8d8fba01793198 56ec18495587f7bb9a1b0ee4edad3698a5311ece --` 仅输出 `M .ai-work-flow/plans/add-tinyurl-short-link-tool/tasks/06-short-link-full-regression.md`，没有源码、测试、依赖清单/锁文件、CSP、迁移或 provider 路径差异；本次证据修复同样不触及这些路径。

### Finding 关闭映射

| Finding | 本记录中的关闭证据 |
|---|---|
| Standards `RS-VALREC-001` | “Implementation Checklist 逐项证据”提供 8/8 项源码、符号、断言与命令映射。 |
| Standards `RS-VALREC-002` | “Acceptance Criteria 逐项证据”提供 10/10 项测试场景与对应命令结果。 |
| Standards `RS-VALREC-003` | “Verification Steps 本次重跑结果”提供 9/9 项命令、工作目录、退出状态和通过/忽略/警告计数。 |
| Spec `RS-001` | 验证上下文固定记录基点、旧 review、日期/环境，并由 8 项实现证据证明跨层契约闭环。 |
| Spec `RS-002` | 安全审计记录 endpoint、Token、Authorization/Bearer、敏感长 URL、generic secret 隔离、九错误码和本地删除边界。 |
| Spec `RS-003` | Lint/Rust ignored 例外均有精确归因；固定基点差异证明无依赖、CSP、迁移、provider 或范围外变更。 |

## Out of Scope

不增加新功能、不调整已确认交互、不进行真实 TinyURL 手工调用、不修改计划外文件，也不处理 alias、统计、远端撤销、多供应商、历史加密或同步。
