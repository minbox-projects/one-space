# 新增 AI 路由网关

## Plan Metadata

- plan-id: `add-ai-routing-gateway`
- status: `ready-for-implementation`

## Problem Statement

OneSpace 现有 Protocol Router 服务于既有 Claude/provider 场景；其配置、状态、监听器、命令命名空间及统计口径不适合本机 Codex、OpenCode 等客户端使用的 OpenAI-compatible 网关。需要新增独立 AI 路由网关，同时保持 Protocol Router 名称、入口、配置、统计、运行行为及数据完全不变。

## Solution

新增独立的 Rust 子系统 `src-tauri/src/ai_routing_gateway/`，向 IPv4 loopback `127.0.0.1:17688` 提供 OpenAI-compatible HTTP 服务，并在 Tauri 中提供独立 `ai_routing_gateway_*` IPC。创建未来全应用共享的 SQLite 基座 `~/.config/onespace/data/onespace.sqlite3`，仅增加全局迁移表和 `ai_gateway_` 表。敏感材料逐记录 AES-256-GCM 加密，根密钥存入 macOS Keychain。

前端通过独立 typed facade、主侧边栏入口和五页签模块管理网关：首页、账号池、网关密钥、请求日志、设置。额度窗口和模型映射只在账号详情编辑。

## Goals and Success Criteria

1. AI 路由网关与 Protocol Router 并存；不得重命名、迁移、接入或复用后者的内部状态、监听器、配置类型、命令空间、统计或数据。
2. 在 SQLite 与 Keychain 就绪后自动后台启动，默认监听 `127.0.0.1:17688`；端口冲突、数据库失败、Keychain 锁定时保持停止，展示稳定错误且不循环抢占。
3. `GET /health` 匿名且仅返回状态和版本；`GET /v1/models`、`POST /v1/responses`、`POST /v1/chat/completions` 均验证有效网关 Key，错误使用 OpenAI-compatible envelope 和稳定机器码，且不泄露账号信息。
4. 受支持能力的流式/非流式、工具调用、reasoning/推理强度、用量、结束原因，以及 Responses 和 Chat Completions 双向转换均有测试；不能无损转换时，在访问上游前以 400 拒绝，不得静默丢弃字段。
5. 账号池支持 OAuth 和第三方 API Key、单分组多标签、组/账号排序、备注、启停和健康；默认组不可删除，删除非空组在同一事务迁移账号；删除账号移除凭据、额度和映射，但保留请求历史快照。
6. OAuth 仅允许受控官方 Codex 兼容流程、固定 scope、PKCE、官方刷新语义；loopback、手动完整回调 URL 与 Device Code 均严格校验并只在内存保存临时材料。
7. OAuth 额度支持全局/模型/端点/能力动态窗口、明确基础/附加/未知规则、账号继承或覆盖阈值；全局默认阈值为 10%，过期快照排序降级而不直接停用。
8. 路由按 Key 权限、映射、健康、额度、用户排序、剩余额度和最近使用选择，最多尝试三个账号，且只在尚未向客户端输出流字节时切换。
9. 网关 Key 仅创建或重生成时返回一次明文，数据库仅存前缀与加盐哈希或等效验证材料；OAuth token、第三方 Key 均加密；所有日志、诊断、fixture 禁止正文和任何凭据明文。
10. SQLite 使用 WAL、foreign keys、busy timeout 和 `(subsystem, version)` 事务迁移；首建、升级、失败回滚和并发初始化可测试，且不改 JSON、app_store、Protocol Router 或其他数据源。
11. 逻辑请求和每次上游尝试均结构化记录；明细默认保留 90 天并支持 7/30/90/180/永久；每日聚合按本机时区增量、可补零、可重建和校验；缺价格或用量时费用为不可计算，不得写为 0。
12. 首页展示账号与可用状态、5 小时/7 日/附加窗口、今日 Token 分项、K/M 总量、预估美元费用和 7/15/30 日趋势；支持账号、分组、公开模型筛选，Token 与费用独立视图且禁止双 Y 轴。
13. `npm run build`、`npm run lint`、`npm run test`、`cargo test`、`cargo check` 及项目既有构建类检查通过；测试不依赖公网。本计划不运行未授权的 Playwright、可见浏览器、E2E 或视觉验证。

## User Stories

- 作为本机 Codex/OpenCode 用户，我可以以 OpenAI-compatible API 和网关 Key 调用被授权的公开模型，而无需暴露上游凭据。
- 作为管理员，我可以按组管理 OAuth 与第三方账号、模型映射、额度和健康状态，并可预期地控制路由优先级。
- 作为管理员，我可以创建受组和模型限制的多个网关 Key，且仅在创建或重生成当时取得一次明文。
- 作为管理员，我可以查看逻辑请求、尝试、用量和可计算费用，并从首页按时间、账号、组和模型理解容量与趋势。
- 作为 OAuth 用户，我可以使用官方浏览器授权、手动回调或 Device Code 完成登录；重新授权同一稳定账号只原子更新凭据和元数据。

## Scope

后端新增 `src-tauri/src/shared_sqlite` 与 `src-tauri/src/ai_routing_gateway/`。后者按 `commands`、`storage`、`runtime`、`oauth`、`router`、`protocol`、`usage`（额度刷新子边界）、`pricing`、`types`、`tests` 组织。

`src-tauri/src/lib.rs` 仅增加 `shared_sqlite` 和 `ai_routing_gateway` 两个模块声明；不得在此文件添加初始化、命令注册或其他功能。运行时初始化、`generate_handler!` 命令注册、Protocol Router 既有 autostart/status event 协调及退出清理均在 `src-tauri/src/app_runtime/run_app.rs` 和既有生命周期边界完成。

前端新增 `src/lib/aiRoutingGateway.ts` 作为唯一 typed invoke/event facade，并新增 `src/components/AiRoutingGateway/` 五页签模块。导航接入 `src/App.tsx`、`src/lib/navigation.ts`、`src/components/MoreToolsHub.tsx`、`src/components/Launcher.tsx` 的既有模型；翻译仅扩充 `src/i18n.ts` 的内联中英文资源，不创建 `public/locales/`。

## Implementation Decisions

### 隔离与生命周期

- 新网关拥有独立数据库访问、安全状态、HTTP server、OAuth 会话、额度调度器、维护调度器、路由健康和 Tauri 状态事件；启动、停止、重启必须幂等。
- 初始化顺序固定为：SQLite bootstrap 与迁移成功 -> Keychain 检查、读取或按规则创建根密钥 -> 读取设置并预检端口 -> 启动 HTTP、额度、维护调度器。任一步失败均停止服务并发出稳定状态，不重试抢占端口。
- 默认运行开关启用时，在上述依赖就绪后自动启动。完全退出时先停止接入新请求，在有上限的排空期完成在途请求和日志事务，再释放 listener 并停止调度器；未完成流记录为取消或中断，不得伪报成功。
- 保存端口前必须预检占用；运行中改端口固定执行“停止接入 -> 排空 -> 释放旧 listener -> 绑定新端口”。绑定失败保持停止并报告端口冲突，不影响应用其他功能。
- 仅绑定 `127.0.0.1`，拒绝 LAN/public 地址；默认端口为 `17688`，不得使用 Protocol Router 的 `127.0.0.1:17687` listener。

### 存储与安全

- SQLite 固定路径为 `~/.config/onespace/data/onespace.sqlite3`。bootstrap 创建目录和数据库，设置 `journal_mode=WAL`、`foreign_keys=ON`、明确 `busy_timeout`；不使用单一 `user_version`。
- 建立全局 `app_schema_migrations`，以 `(subsystem, version)` 唯一；本子系统 `subsystem` 固定 `ai_routing_gateway`。每版迁移在单事务中执行，失败不写入版本；并发 bootstrap 使用数据库锁与幂等存在性检查；不得修改未知表。
- 每条敏感记录采用 AES-256-GCM，使用密码学安全随机 nonce，记录 cipher version，并以“记录类型 + 稳定记录 ID”为 AAD。未知版本、认证失败或 AAD 不匹配使该凭据不可用，仅记录安全类别和实体 ID。
- 增加与 Rust 2021/MSRV 1.77.2 和当前 Tauri 工具链兼容的 `keyring` 依赖，将根数据密钥存入 macOS Keychain；不复用 `src-tauri/src/secrets.rs` 现有 `.local_key` 路径。可参考现有 `src-tauri/src/crypto.rs` 的工程惯例，但不得共享其根密钥状态。
- 仅在数据库没有既有网关密文时创建新的 Keychain 根密钥。若数据库已有网关密文而 Keychain 项缺失或不可访问，进入锁定：停止网关和敏感操作，引导用户逐账号重新授权或录入；不得静默覆盖旧密文。API Key 在离开 invoke 边界后立即按该方案保护，读取 API 永不返回明文。
- tracing、IPC、HTTP 错误、SQLite 日志、测试 fixture 禁止完整请求/响应正文、提示词、工具参数、Authorization、Cookie、Token 与 API Key 明文。

### 账号、模型、额度与 OAuth

- 每账号恰好属于一个组，可有多个标签；组和账号均可排序。标签仅用于筛选识别，不参与路由。新 OAuth/第三方账号进入唯一默认组；默认组不可删除。删除非空非默认组时，在同一事务将其账号迁至默认组。
- 账号支持 OAuth 和第三方 API Key。第三方账号保存 Base URL、加密 API Key、鉴权方式和上游协议；上游协议仅允许 OpenAI-compatible Responses 或 Chat Completions。禁用账号是常规退出路由。永久删除须由前端取得二次确认令牌并由后端验证，删除凭据、额度、映射而不级联删除历史和聚合。
- 维护统一公开模型目录和每账号独立映射。OAuth 从官方目录产生默认映射并可逐项禁用；第三方维护“公开模型 -> 上游模型”。未映射模型不得透传。`/v1/models` 只返回该 Key 授权且至少一个可路由账号支持的公开模型。
- OAuth 只使用官方 Codex 兼容端点、固定 client ID、固定 scope 和官方刷新语义；禁止自定义 issuer/client ID/scope、Cookie 导入、网页抓取、浏览器会话模拟。若官方政策、法律或技术流程不允许第三方桌面接入，则作为发布阻塞停止该能力，不绕过。
- 浏览器授权每次使用随机 state、PKCE verifier 和随机 loopback 回调端口，并调用系统默认浏览器；保留手动粘贴完整 callback URL。两种方式共享内存会话，校验 state、code、error；自动回调失败不作废会话。Device Code 严格按服务端 `interval` 轮询，`pending` 继续、`slow_down` 增加间隔、`expired`/`cancel` 终止；UI 显示 code、verification URL、倒计时、复制、打开和取消。code、device code、PKCE verifier、state 只存在内存并在完成、失败、取消、超时后清理。
- 按稳定账号 ID 去重；再授权以事务原子替换凭据和元数据。额度仅适用于 OAuth，来源仅为 OAuth 可访问接口与上游响应限额元数据，禁止解析网页。登录后、手动、请求完成后刷新；后台默认每 5 分钟刷新。同账号并发刷新合并，失败采用有上限指数退避。
- 动态额度持久化名称、上游 ID、剩余/已用百分比、重置时间、时长、范围（全局/模型/端点/能力）、最近成功和过期状态，支持 5h/7d、仅 7d、Code Review、Spark 与未知窗口。任一适用标准基础窗口耗尽即暂停普通请求；附加窗口仅限制对应能力；未知窗口有范围时参与门禁，无范围仅展示。
- 全局阈值范围为 0-100，默认 10%；账号可继承或覆盖。任一适用窗口低于阈值时，该账号对当前请求不可用；达到阈值自动恢复；0 表示仅完全耗尽时停用。刷新失败保留最后成功快照并标过期，过期不触发阈值停用，仅路由排序降级。设置变更立即作用于后续请求，不中止在途请求。

### Key、路由与协议

- 网关 Key 支持名称、可见前缀、创建时间、最近使用、启用、失效、一个或多个组权限和公开模型权限。使用密码学安全随机源；数据库只保存高熵 Key 的前缀与加盐哈希或等效验证材料。明文仅在创建或重生成响应中返回一次；重生成原子撤销旧材料；禁用、撤销、过期即时影响新请求。日志只存内部 Key ID 与名称快照。
- 受保护 HTTP 接口至少接受 `Authorization: Bearer <key>`，并设置合理请求头和 JSON 大小限制。请求依次执行：限制输入/生成 request ID -> 验 Key -> Key 权限 -> 协议能力校验 -> 候选过滤与排序 -> 同协议受控透传或跨协议转换 -> 最多三次上游尝试并受首字节门禁 -> 日志与聚合 -> 兼容响应。
- 候选过滤顺序固定为：Key 组和模型权限、账号启用、有效凭据、模型映射、健康状态、适用额度。排序固定为：账号用户排序升序；排序相同则新鲜额度优先于过期额度；再按适用窗口最低剩余百分比降序；再按最近使用时间升序（从未使用视为最早）；最终按稳定账号 ID 字典序升序，保证确定性。
- 每逻辑请求最多尝试三个不同账号。流式请求首字节输出前允许切换，首字节输出后固定账号。客户端断开即取消上游并记录取消，不计入上游健康失败。
- OAuth 的 401/403 自动刷新一次，仍失败则标授权失效；第三方 401/403 直接标失效。其他请求或模型语义 4xx 不影响健康且不切换。明确额度耗尽时暂停对应 scope 至 reset 并触发刷新；429 优先使用 `Retry-After`，缺失时自 60 秒指数冷却、最长 15 分钟；网络或 5xx 连续三次熔断 60 秒，后续指数增长至最多 15 分钟，到期只允许单次探测，成功清零。没有候选时返回 `no_available_upstream`，不得暴露过滤原因。
- 对外端点固定为 `GET /health`、`GET /v1/models`、`POST /v1/responses`、`POST /v1/chat/completions`。建立内部规范化请求、事件、响应模型，先校验能力再选择上游。同协议仅受控透传；异协议双向转换请求、非流式响应、SSE、工具调用、reasoning/推理强度、用量、结束原因和错误。任何不可无损表达的输入在请求上游前返回 400 和稳定码 `lossless_conversion_unsupported`。
- 所有外部错误均使用 OpenAI-compatible `error` envelope；稳定机器码至少包括 `authentication_failed`、`permission_denied`、`model_unavailable`、`no_available_upstream`、`lossless_conversion_unsupported`、`invalid_request`、`upstream_rate_limited`、`upstream_authorization_invalid`、`upstream_unavailable`、`gateway_not_ready`。

### 日志、价格与前端

- 请求入口固化 request ID、Key、模型、端点、价格快照和本机时区上下文。每次账号调用写 attempt，最终完成、上游失败、客户端取消和流中断均写 logical request。请求完成事务同时写最终日志和本机日期每日聚合。
- Token 字段为输入、输出、缓存读、缓存写、总量；未知值保持缺失。价格按公开模型维护每百万 Token 输入/输出/缓存读/缓存写。官方内置快照随应用更新，第三方覆盖由用户维护；请求开始取不可变价格快照。OAuth 费用标记为公开 API 单价等效预估，不代表订阅扣款；缺价格或用量时费用不可计算，禁止以 0 替代。
- 明细默认保留 90 天，可选 7/30/90/180/永久。批量清理、手动清空、适度 SQLite 维护及聚合重建/校验均由后台执行，不进入请求热路径；每日聚合长期保留。查询支持时间、账号快照、组、公开/上游模型、状态、错误、Key 和稳定游标分页。趋势 7/15/30 天补连续日期，未知费用不得补成已知 0。
- `src/lib/aiRoutingGateway.ts` 是唯一 invoke/event facade；组件不得散布命令字符串或依赖 Rust 存储结构。IPC 前缀统一 `ai_routing_gateway_*`，覆盖 runtime/settings、账号/组/标签、OAuth/Device Code、额度/阈值、模型/映射、Key/权限、日志/尝试、价格、统计/维护。写入命令使用明确 DTO、后端校验和事务，输出不含敏感值；事件分 runtime、OAuth、额度/账号和维护进度。
- UI 使用现有 React 19、TypeScript、Vite 7、Tailwind、Radix、Lucide 惯例，参考 `AiUsageStats.tsx` 的数据和趋势组织，但必须采用独立数据源。首页支持账号/分组/公开模型筛选，展示账号数量、可用/不可用进度、5 小时/7 日和附加窗口、今日 Token 分项、K/M 总量、美元预估费用及 7/15/30 日趋势，Token/费用为独立视图。账号池支持排序、标签筛选、启停、健康、备注、OAuth/第三方创建和永久删除；详情编辑额度、过期、阈值、映射。Key 页支持一次性明文、复制、权限、重生成、禁用、撤销、过期。日志页支持筛选、分页、attempt、不可计算费用、清空。设置页支持端口、服务、全局阈值、保留、价格、聚合维护。

## Implementation Changes

### 阶段 1：共享 SQLite 与安全基座

1. 在 `src-tauri/src/lib.rs` 仅声明 `shared_sqlite`、`ai_routing_gateway`。
2. 实现共享 SQLite bootstrap、连接 PRAGMA、锁、迁移执行器和 `app_schema_migrations`，确保仅 AI 子系统写入自己的版本记录。
3. 新增 Keychain 根密钥适配层、AES-256-GCM 凭据封装、锁定状态和脱敏诊断；为 keyring 提供隔离测试替身。
4. 初始迁移创建本计划列出的所有 `ai_gateway_` 表、外键、删除语义和查询索引。

### 阶段 2：账号、OAuth、额度与价格

1. 实现组、标签、账号、凭据、模型目录、映射和默认组事务规则。
2. 实现仅官方 Codex OAuth 的 PKCE loopback、手动回调、Device Code、稳定 ID upsert 和临时材料清理。
3. 实现 OAuth 额度提取、动态窗口、刷新去重、退避、阈值与首页聚合口径。
4. 实现公开模型价格快照、第三方覆盖和不可计算费用规则。

### 阶段 3：Key、HTTP runtime、路由与协议

1. 实现网关 Key 创建、一次性返回、哈希验证、权限、重生成、撤销、禁用和过期。
2. 实现独立 HTTP runtime、loopback 绑定、输入限制、四端点、运行状态和端口受控重启。
3. 实现候选过滤、确定性排序、三次尝试、健康/冷却/熔断和首字节切换门禁。
4. 实现规范化协议模型、Responses/Chat 双向转换、SSE、取消和 OpenAI-compatible 错误。

### 阶段 4：日志、费用与聚合维护

1. 实现逻辑请求与尝试快照、完成事务、价格与 Token 字段。
2. 实现本机时区每日聚合、补零查询、稳定游标筛选、保留策略、清空、维护、重建与校验。

### 阶段 5：IPC、导航、UI 与 i18n

1. 实现 typed facade、DTO、命令注册和事件订阅释放。
2. 按既有导航模型接入独立侧边栏入口、More Tools Hub、Launcher 和五页签模块。
3. 在 `src/i18n.ts` 同步添加完整中英文键；不创建 `public/locales/`。

### 阶段 6：生命周期、诊断与回归收口

1. 在 `run_app.rs` 接线初始化、命令注册、自动启动、状态事件、端口改动和退出排空；保留 Protocol Router 原路径不变。
2. 完成脱敏审计、锁定/错误状态、全套无公网测试和构建门禁；仅在 lifecycle 与 Protocol Router 回归完成后启用默认自动启动。

## Public Interfaces

HTTP 接口仅为：

- `GET /health`：匿名，返回网关状态和版本。
- `GET /v1/models`：Bearer 验证后，返回当前 Key 授权且存在可路由账号支持的公开模型。
- `POST /v1/responses`：Bearer 验证后处理 OpenAI-compatible Responses 请求。
- `POST /v1/chat/completions`：Bearer 验证后处理 OpenAI-compatible Chat Completions 请求。

Tauri 命令均以 `ai_routing_gateway_*` 命名，按 runtime/settings、账号/组/标签、OAuth/Device Code、额度/阈值、模型/映射、Key/权限、日志/尝试、价格、统计/维护分组；所有写命令使用明确 DTO、后端校验和事务，所有读取输出均不含 token、第三方 API Key 或已创建 Key 的明文。

运行事件分为 runtime、OAuth、额度/账号、维护进度。前端只经 `src/lib/aiRoutingGateway.ts` 使用这些命令和事件。

## Data Flow and Failure Modes

### SQLite schema、关系与索引

初始 schema 固定包括：

- `ai_gateway_settings`：端口、全局阈值、日志保留、运行开关。
- `ai_gateway_groups`：名称、排序、默认标记；默认组唯一且不可删除。
- `ai_gateway_accounts`：稳定 ID、类型、名称、组、排序、备注、启用、健康、最近使用、阈值覆盖；删除时不级联历史。
- `ai_gateway_tags`、`ai_gateway_account_tags`：账号多标签关联。
- `ai_gateway_credentials`：账号一对一密文、nonce、版本、元数据；随账号永久删除而删除。
- `ai_gateway_models`：统一公开模型目录。
- `ai_gateway_account_model_mappings`：账号公开模型到上游模型映射；随账号删除而删除。
- `ai_gateway_quota_windows`：OAuth 动态额度窗口；随账号删除而删除。
- `ai_gateway_api_keys`：Key 安全材料、前缀、状态、时间。
- `ai_gateway_api_key_groups`、`ai_gateway_api_key_models`：Key 的组和公开模型权限。
- `ai_gateway_request_logs`：逻辑请求及不可变 Key/账号/模型/价格快照。
- `ai_gateway_request_attempts`：每次上游尝试、顺序、账号快照、时间、状态、错误、是否输出流字节、健康影响。
- `ai_gateway_model_prices`：官方和第三方覆盖价格。
- `ai_gateway_daily_aggregates`：长期保留的本机日期聚合。

为逻辑日志、尝试和聚合的时间范围与筛选维度创建组合索引；为账号组/排序、模型映射、额度 scope/reset、Key 哈希、授权关联创建索引。账号、组、Key 删除或变更均保留日志/聚合快照可读。

### 启动、停止与存储失败

启动严格按“SQLite+迁移 -> Keychain 检查/创建或锁定 -> 设置与端口预检 -> HTTP、额度、维护调度器”。迁移 busy、失败回滚、数据库不可用、Keychain 锁定、根密钥缺失且存在密文、端口被占用和 listener 绑定失败均使 runtime 保持停止，产生稳定的非敏感错误状态，不自动循环尝试。

停止和改端口先停止新接入，在上限排空期内等待在途处理及日志事务，随后释放 listener。无法完成的流写取消/中断结果；日志写入失败不得伪报请求或聚合成功，记录脱敏存储错误并返回适当网关失败响应。迁移失败不得写迁移版本；并发初始化不得重复建表或破坏已有数据。

### 请求、流式与健康失败

请求在限制输入并生成 ID 后，执行认证、权限、协议能力校验、候选过滤和确定性排序。无权限模型返回 `model_unavailable` 或 `permission_denied`；认证失败返回 `authentication_failed`；网关停止/锁定返回 `gateway_not_ready`；所有候选不可用返回 `no_available_upstream`。上游前发现无损转换不可能时返回 HTTP 400 `lossless_conversion_unsupported`，且禁止发起上游请求。

每一尝试及最终逻辑结果入库。首字节前可对未输出客户端的失败使用下一个账号，最多三个不同账号；首字节后不切换。SSE 保持事件顺序和工具调用增量；客户端断开取消上游、记取消、不记健康失败。OAuth 401/403 刷新一次后仍失败为授权失效；第三方 401/403 直接失效；语义 4xx 不降健康也不切换。额度耗尽、429、网络/5xx 按既定暂停、冷却、熔断、探测规则更新健康。

### OAuth、额度和聚合失败

回调 state/code/error 不匹配、PKCE 或固定 scope 不合规、Device Code 过期/取消均终止会话并清理内存材料。自动 loopback 回调失败保留手动回调路径。刷新失败保留最近成功额度并标过期，按上限指数退避；过期仅降低排序，不直接按阈值禁用。

每日聚合使用请求开始时固化的本机日期和时区上下文；系统时区之后变更不重写历史日期。缺失 Token 或价格使费用字段及聚合费用保持不可计算状态。趋势补零只补真正无请求日期，未知费用不折算为零。

## Testing Decisions

所有测试使用本机 mock server、fixture 和替身，不依赖公网，不启动浏览器。测试按成功标准覆盖如下：

1. Rust 存储：首建、升级、迁移失败原子回滚、重复和并发初始化、PRAGMA、未知未来表；默认组、删除组迁移、账号删除后历史保留和索引查询。
2. 安全：隔离 keyring 替身验证根密钥创建/读取/不可访问/已有密文时丢失/不覆盖/重新录入；AES nonce、AAD、篡改、版本、脱敏；网关 Key 熵、一次明文、哈希、重生成、禁用、撤销、过期和权限。
3. OAuth/额度：本机 mock 和 fixture 覆盖受控 URL、PKCE/scope/state、loopback、手动回调、清理；Device Code 所有状态；刷新并发合并、稳定 ID upsert、窗口类别和刷新触发/退避；0/10/100 阈值边界和首页分母规则。
4. Router/HTTP/protocol：loopback mock upstream 覆盖候选过滤、稳定排序、401/403、耗尽、429、网络、5xx、熔断、探测、首字节门禁、最多三次、四端点、Bearer、立即失效、models 交集；Responses/Chat 双向 fixture 覆盖非流、SSE、工具、reasoning、usage、finish reason、error；无损转换失败为 400 且 mock 未收到上游请求。
5. 日志/统计/费用：逻辑/attempt 字段、账号删除后快照、敏感 fixture 扫描、保留策略/清空/回滚、本机时区、跨日/DST、补零、筛选、重建/校验、价格优先级/快照/不追溯和不可计算费用。
6. 前端/Tauri/lifecycle：Tauri mock facade 与 event cleanup；一次性 Key 不进入持久状态或日志；Vitest/Testing Library 覆盖导航、五页签、账号、OAuth 三路径、额度、Key、日志、设置、首页、i18n 完整性及错误/锁定/端口/重启状态；Protocol Router 回归；Rust 生命周期覆盖初始化顺序、冲突不重试、自动启动、改端口和退出排空。
7. 执行 `npm run build`、`npm run lint`、`npm run test`、`cargo test`、`cargo check` 及项目已有构建类检查。本计划不含 Playwright、浏览器自动化、E2E 或视觉验证；发布前需此类人工或视觉验证时必须另行授权。

## Rollout and Compatibility

按以下 gate 分阶段交付：

1. schema/security：完成 SQLite、迁移、Keychain、加密与锁定测试，入口存在但服务关闭。
2. account/OAuth/quota：完成账号池、官方 OAuth、额度与价格；若官方政策或技术不允许第三方 Codex OAuth，停止发布该能力。
3. router/protocol：完成 Key、HTTP、路由和 mock 协议转换矩阵。
4. logs/stats/UI：完成日志、费用、聚合、IPC、导航、五页签和 i18n。
5. lifecycle/regression：完成自动启动、端口重启、退出排空和 Protocol Router 回归后，启用默认自动启动。

不迁移不存在的旧 AI 网关，也不迁移 JSON、app_store 或 Protocol Router 数据。数据库仅新增迁移表和 `ai_gateway_` 表，不修改未知表。默认仅 loopback，不扩大网络面；端口冲突不影响其他应用功能。数据库跨设备迁移而 Keychain 未迁移时，凭据不可解密，用户必须重新授权或录入；schema 兼容时非敏感历史仍可读。

发布阻塞条件为：官方政策/技术不允许第三方 Codex OAuth；关键转换矩阵失败；敏感数据进入日志；Keychain 丢失路径覆盖密文；无法保证 loopback-only 或优雅生命周期。回滚通过运行开关停止服务并隐藏入口，不影响 Protocol Router；不得删除 SQLite、迁移记录、`ai_gateway_` 表或未知表，不进行破坏性逆迁移，后续仅前向修复；退出仍须优雅停止并提交已完成日志。

## Out of Scope

- 改造、重命名、合并 Protocol Router，或迁移其配置、统计、状态。
- 迁移其他 JSON/app_store 数据到共享 SQLite。
- LAN/public、TLS、CORS、远程、多用户。
- Anthropic/Gemini 原生协议；仅支持 OpenAI Responses/Chat。
- Cookie 导入、网页抓取、chatgpt.com 页面解析、浏览器会话模拟 OAuth。
- 自定义 OAuth issuer/client ID/scope 或非官方流程。
- 保存完整请求/响应、提示词、工具参数、认证凭据。
- WebSocket。
- 核对真实 ChatGPT 订阅账单。
- 未明确的数据导入/导出。
- 未授权浏览器自动化、E2E、视觉验证。

## Assumptions

- 首发仅支持 macOS Keychain，不承诺其他平台。
- 使用场景为本机单用户、loopback 客户端，不包含远程多租户。
- 官方 Codex OAuth 的合法、政策和技术许可是发布前提。
- OneSpace 后台进程持续提供 Tauri invoke/event、HTTP runtime 和优雅退出能力。
- 额度窗口可能新增；动态 scope 必须兼容，未知且无 scope 的窗口仅展示。
- 本机时区是每日分桶权威；历史保存当时的时区/日期，不因系统时区修改而重写。
- 官方价格为公开 API 定价，第三方覆盖由用户维护；二者均非实际订阅账单。

## Further Notes

N/A：已确认的产品、协议、安全、存储、测试、发布和回滚决策均已纳入，无额外说明。
