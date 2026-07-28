# AI 请求抓包工具实施计划

计划 ID：`ai-request-capture-proxy`

## 目标

- 在 OneSpace 中新增独立的“AI 请求抓包”工具，以仅监听 `127.0.0.1` 的本地反向代理接收常规 HTTP 请求，按配置映射到 HTTP/HTTPS 上游，并完整转发响应。
- 在不阻塞分块请求、SSE 首块或后续流量的前提下，记录请求/响应元数据、敏感头、query 和正文样本，提供查询、详情、筛选、分页、清空、HAR 1.2 导出和 cURL 生成。
- 明确保持所有鉴权值、Cookie 和正文为本地明文；界面持续显示风险警告，并对清空、导出执行二次确认。
- 将该工具接入 More Tools、Launcher、内部导航、中英文资源和三个代码导航索引。
- 保持与 Protocol Router 的职责隔离：本工具不读取其路由、不调用其转换函数、不执行 OpenAI/Anthropic/Gemini 协议转换。

## 需求

### 代理配置与生命周期

- 配置仅包含 `enabled`、`port`、`upstream_base_url`；默认 `enabled = false`、`port = 17688`、上游为空，监听地址不可配置且固定为 `127.0.0.1`。
- `upstream_base_url` 允许 `http`/`https`、显式端口和自定义路径；拒绝 query、fragment、空 host，以及有效目标指向当前 `127.0.0.1:<port>` 的循环配置。
- 上游 URL 由“上游 Base URL 的路径前缀 + 入站原始 path/query”构成；只替换 origin，不重写入站路径、百分号编码或 query。
- `enabled = true` 的有效配置在应用启动时自动恢复监听。绑定、DNS、数据库或其他恢复错误写入运行状态并发出状态事件，但不得阻止 OneSpace 启动。
- 保存配置时先做确定性校验；若配置有效则持久化，再根据 `enabled` 启停或重启运行时。运行时失败保留期望配置并通过 `last_error` 暴露。
- `start`/`stop` 命令只控制当前运行态；持久自动恢复以已保存的 `enabled` 为准。前端切换 Enabled 后通过保存配置改变持久期望状态。

### HTTP 转发边界

- 支持除 `CONNECT` 外的常规 HTTP method，包括带正文或无正文的请求。
- 支持 `Content-Length` 和 chunked 请求体；请求体边接收边转发，不先缓冲完整正文。
- 支持普通、chunked 和 SSE 响应；收到上游首个 body chunk 后立即向客户端发送，不等待完整响应或数据库写入。
- 明确拒绝 `CONNECT` 和 WebSocket Upgrade，不实现证书生成、TLS 中间人、透明代理或 WebSocket 隧道。
- 请求转发前移除标准 hop-by-hop headers、`Connection` 所声明的扩展 hop-by-hop headers、`Host` 和 `Content-Length`，再设置 `Accept-Encoding: identity`；响应返回前同样移除 hop-by-hop headers 和旧 `Content-Length`，由流式响应层决定传输编码。
- 原始入站请求头和原始上游响应头在清理前抓取；所有 Authorization、API Key、Cookie 及其他 header value 原样保留，不做脱敏、遮罩或加密。重复 header value 保留为有序多值；header 名可按 HTTP 库规范化，不承诺保留线上的大小写和字节排版。
- 上游连接失败且尚未发送响应头时返回 `502`；不支持的方法/升级或无效目标返回明确的 4xx/5xx。已经开始流式响应后发生传输错误时关闭流并把记录标记为传输失败，不伪造第二个 HTTP 响应。

### 抓取、解析与保留

- 每条记录保存稳定 UUID、开始/结束时间、运行状态、HTTP 版本、method、原始 path/query、解析后的上游 URL、完整明文 headers、响应状态、耗时、错误和正文统计。
- 状态至少区分 `in_progress`、`completed`、`rejected`、`upstream_error`、`request_transfer_error`、`response_transfer_error`、`client_disconnected`、`interrupted`。
- 请求正文和响应正文分别只保留最前面的 `2 MiB`；同时累计真实传输字节数并保存 `captured_bytes`、`total_bytes`、`truncated`。2 MiB 限制只约束持久化样本，绝不截断、延迟或取消网络转发。
- SQLite 内以 BLOB 保存正文样本。IPC、HAR 和 UI 对 UTF-8 文本按文本返回，对非文本或无效 UTF-8 按 Base64 返回并携带 `encoding = "base64"`；判断结合 Content-Type 和 UTF-8 有效性。
- 最佳努力识别 OpenAI、Anthropic、Gemini 和 unknown；从请求 JSON、Gemini 路径以及普通/流式响应 usage 字段提取模型和 input/output/total token。解析失败、正文非 JSON 或样本截断不影响代理和记录完成。
- 记录固定保留 7 天。启动时把遗留 `in_progress` 标记为 `interrupted` 并清理过期记录；完成写入后以节流方式再次清理。抓包数据库和配置位于应用本机数据目录，不使用 `get_data_dir()` 指向的 Git/iCloud 共享存储，也不进入 `app_store` 同步/outbox。

### 查询、HAR 与 cURL

- 列表查询在 SQLite 端完成，支持文本搜索、method、状态组、provider、model、page、page_size；按 `started_at DESC, id DESC` 稳定分页，响应含总数和页信息，不在 IPC 返回全量正文。
- 详情命令按 ID 返回完整记录和最多 2 MiB 的请求/响应样本；记录在列表刷新期间仍可从 `in_progress` 更新为最终状态。
- HAR 1.2 只导出当前过滤条件命中的已结束记录；包含真实敏感 headers、query、明文文本正文或 Base64 二进制正文。使用标准 `log/entries/request/response/content/postData/timings`，并用 `comment` 与 `_onespace` 扩展标记截断、捕获字节/总字节、运行状态和传输错误。
- HAR 写入用户通过 Tauri 保存对话框选择的路径；先由前端展示包含“明文鉴权、Cookie、正文”的二次确认，取消时不调用导出命令。
- cURL 从选中记录生成，目标使用该次实际解析的上游 URL，保留真实端到端 headers 和 method，排除代理转发时已移除的 hop-by-hop/Host/Content-Length。
- cURL 的文本参数采用 POSIX shell 单引号安全转义。二进制正文使用 POSIX `printf '%b'` 的八进制字节流管道给 `curl --data-binary @-`，避免把任意字节直接放进 argv。无正文的方法不添加 data 参数。
- 正文样本截断或传输失败时仍允许用户生成 cURL，但返回结构必须含 `complete = false` 和明确 warning；UI 在复制前提示该命令不能完整重放，并在复制文本首行加入警告注释。

### 前端体验

- 工具页顶部为全宽配置区：持续可见的明文风险警告、Enabled 开关、端口输入、Upstream Base URL、保存/应用按钮、当前运行状态、监听地址和最后错误。
- 主工作区为稳定尺寸的左右布局。左侧提供搜索与 method/status/provider/model 筛选、记录列表、刷新和分页；右侧提供空状态或选中记录详情。
- 详情概览展示 method、入站 path/query、实际上游 URL、状态、时间、耗时、provider/model/token、请求/响应真实字节数、截断和错误。
- 详情使用 Request/Response 分段控件，各自提供 Headers/Body 视图；headers 显示完整值，正文按文本或 Base64 展示，截断/传输错误始终可见。
- 工具栏提供刷新、导出 HAR、清空记录和复制 cURL。导出与清空均使用现有确认对话框做二次确认；清空成功后清除选中项并刷新第一页。
- 订阅 `ai-request-capture-updated` 增量刷新当前列表/详情，订阅 `ai-request-capture-status-update` 刷新运行状态；组件隐藏时不轮询，重新可见时主动校准一次。

## 实施决策

### 模块与依赖边界

- 新增独立 Rust 门面 `src-tauri/src/ai_request_capture.rs` 和目录 `src-tauri/src/ai_request_capture/`，建议拆分：
  - `types.rs`：配置、状态、查询、列表、详情、body 表示、导出结果。
  - `config.rs`：本机配置路径、默认值、原子写入和 URL/端口/循环校验。
  - `storage.rs`：SQLite schema、迁移、WAL/busy timeout、CRUD、筛选分页、保留清理。
  - `runtime.rs`：全局运行态、监听启停、自动恢复、shutdown 和状态事件。
  - `proxy.rs`：HTTP server、目标 URL 构造、header 清理、请求/响应 body tee 与错误映射。
  - `enrichment.rs`：provider/model/token 最佳努力解析和文本/Base64 表示。
  - `export.rs`：HAR 1.2 序列化、文件写入和 POSIX cURL 生成。
  - `commands.rs`：Tauri 命令薄层；`tests.rs` 及按需测试 helper。
- 服务端使用 Hyper 1 的 HTTP/1 server body 流，配合 `hyper-util`、`http-body-util`、`bytes`；上游继续使用 reqwest 0.12，并开启 `stream`，用流式 request body 和 `bytes_stream()` 做双向 tee。新增直接依赖后同步 `src-tauri/Cargo.lock`。
- 不从 `protocol_router` 导入任何类型或函数。可以参照其 `OnceLock<Mutex<...>> + oneshot shutdown + TcpListener` 生命周期形状，但新模块拥有独立状态、配置、监听器、HTTP 处理和测试。
- rusqlite 调用放入 `spawn_blocking`，每次操作建立带统一 pragma 的短连接或通过模块内受控连接 helper 执行；异步网络任务不得持有 SQLite connection 或 mutex 跨 `await`。
- 每个进行中请求只在内存持有请求/响应各最多 2 MiB 的 capture buffer。开始时插入轻量记录，结束或失败时一次更新最终字段；崩溃遗留由下次启动标记 `interrupted`。

### 本地路径与 schema

- 配置：`config::get_app_dir()/data/ai-request-capture/config.json`。
- 数据库：`config::get_app_dir()/data/ai-request-capture/captures.sqlite3`。
- 初始 schema 使用 `PRAGMA user_version` 管理，至少包含 `captures` 主表及 `started_at/state/method/status/provider/model` 索引。headers 以 JSON 数组保存，body 以 BLOB 保存，数字字段分别保存总字节与捕获字节。
- 不修改 `src-tauri/src/storage.rs` 或 `src-tauri/src/app_store/` 数据模型；数据库路径不得加入共享快照、Git/iCloud 同步或 dashboard count。

### Tauri 接口

在 `src-tauri/src/app_runtime/run_app.rs` 注册以下命令，并由 `src/lib/aiRequestCapture.ts` 提供同名语义的类型化包装器：

1. `ai_request_capture_get_config() -> AiRequestCaptureConfig`
2. `ai_request_capture_save_config(config) -> AiRequestCaptureConfigApplyResult`
3. `ai_request_capture_start() -> AiRequestCaptureStatus`
4. `ai_request_capture_stop() -> AiRequestCaptureStatus`
5. `ai_request_capture_status() -> AiRequestCaptureStatus`
6. `ai_request_capture_list(query) -> AiRequestCaptureListResult`
7. `ai_request_capture_get(id) -> AiRequestCaptureDetail`
8. `ai_request_capture_clear() -> AiRequestCaptureClearResult`
9. `ai_request_capture_export_har(input) -> AiRequestCaptureExportResult`
10. `ai_request_capture_generate_curl(id) -> AiRequestCaptureCurlResult`

接口约束：

- `save_config` 对格式错误返回结构化校验错误；配置有效但运行时启动失败时返回已保存配置和含 `last_error` 的状态，使 UI 能展示“Enabled 但未运行”。
- `list` 不返回 headers/body；`get` 返回完整敏感值。`export_har` 的 input 包含与列表相同的过滤条件和输出路径，但忽略分页。
- `clear` 清除全部记录，包括已结束和当前 `in_progress` 的数据库行；正在转发的请求不被取消，其完成后可重新写回最终记录。确认文案必须明确这一并发行为。
- 数据创建、完成、失败、清空后发出 `ai-request-capture-updated`，payload 至少含 `kind` 和可选 `id`；启停、自动恢复、配置应用和运行错误后发出 `ai-request-capture-status-update`，payload 为完整状态。

### 文件级实施步骤

#### 阶段 1：配置、状态和 SQLite 契约

涉及文件：

- 新增 `src-tauri/src/ai_request_capture.rs`、`src-tauri/src/ai_request_capture/{types,config,storage,commands,tests}.rs`。
- 修改 `src-tauri/src/lib.rs`、`src-tauri/src/app_runtime/run_app.rs`。

实施内容：

1. 建立配置默认值、原子持久化、URL/path/query/fragment/host/port 校验和 loopback 同端口检测。
2. 建立运行状态结构和十个命令的可编译骨架；接入应用 setup 的非阻塞 autostart，失败只更新状态。
3. 建立 SQLite schema、初始/最终记录写入、详情、稳定筛选分页、清空、启动中断恢复和 7 天清理。
4. 先以 in-memory SQLite 和临时目录测试数据契约，不启动真实监听。

阶段关口：配置边界、数据库迁移、明文 headers/BLOB 往返、2 MiB 字段、筛选分页、保留清理和启动不中断测试全部通过；命令已注册且不依赖 Protocol Router。

#### 阶段 2：流式反向代理与抓取

涉及文件：

- 新增 `src-tauri/src/ai_request_capture/{runtime,proxy}.rs`。
- 修改 `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 及阶段 1 类型/命令/测试文件。

实施内容：

1. 实现固定 loopback 监听、幂等 start/stop/restart、oneshot shutdown 和完整状态事件。
2. 实现 origin 替换和 Base URL path 拼接，保留原始 path/query；在保存、启动和每次转发前防止当前监听循环。
3. 实现 request/response 流式 tee、header 清理、identity 编码、常规 method、chunked 和 SSE；capture buffer 达 2 MiB 后仅停止追加样本，继续计数和转发。
4. 实现 CONNECT/Upgrade 拒绝、上游失败、请求/响应传输失败、客户端断开和已经发送响应头后的错误收尾。
5. 使用本地受控 mock upstream 做端到端测试，不访问公网。

阶段关口：同步屏障测试证明 SSE 首块在上游结束前已到客户端；大于 2 MiB 的请求和响应在上游/客户端完整到达而数据库仅存上限；敏感 headers 原值到达上游并明文入库；循环、CONNECT、Upgrade 和 hop-by-hop 行为符合契约。

#### 阶段 3：AI 元数据、HAR 与 cURL

涉及文件：

- 新增 `src-tauri/src/ai_request_capture/{enrichment,export}.rs`。
- 修改 `types.rs`、`storage.rs`、`proxy.rs`、`commands.rs`、`tests.rs`。

实施内容：

1. 在流结束后的后台收尾中解析 OpenAI/Anthropic/Gemini 普通 JSON 与 SSE/分块 JSON 样本；所有解析均为 best effort，不影响客户端链路。
2. 实现文本/Base64 body 表示和 HAR 1.2；导出仅查询过滤后的已结束记录，并写入真实敏感 headers/body。
3. 实现 POSIX shell 单引号转义、文本/二进制 body cURL、截断/传输失败 warning 和 `complete` 标记。

阶段关口：三类供应商普通与流式 fixture、unknown/无效 JSON/截断样本、HAR schema 关键字段、二进制 Base64、敏感值无脱敏、shell 引号及任意字节 body 测试全部通过。

#### 阶段 4：前端工作区与 IPC

涉及文件：

- 新增 `src/lib/aiRequestCapture.ts`、`src/lib/aiRequestCapture.test.ts`。
- 新增 `src/components/AiRequestCaptureTool.tsx`、`src/components/AiRequestCaptureTool.test.tsx`，复杂度需要时只在 `src/components/aiRequestCapture/` 内拆分类型和纯展示组件。

实施内容：

1. 提供十个命令的类型化包装器、错误格式化和两个事件订阅 helper。
2. 实现持续风险警告、配置编辑/校验/应用、运行状态和错误展示。
3. 实现服务端筛选分页列表、选中记录竞态保护、事件驱动刷新、概览和 Request/Response + Headers/Body 视图。
4. 实现刷新、清空二次确认、HAR 保存路径与二次确认、cURL 完整/不完整提示和复制反馈。
5. 保持固定布局尺寸和 overflow 约束；长 header、URL、Base64 和正文只能滚动/换行，不得撑破工具区域。

阶段关口：组件测试覆盖配置成功/校验/运行失败、风险警告常驻、筛选分页、过期详情响应不覆盖、进行中状态更新、敏感值展示、Base64、截断、事件刷新、导出/清空二次确认和 cURL warning。

#### 阶段 5：导航、Launcher、i18n 与索引

涉及文件：

- `src/App.tsx`
- `src/lib/navigation.ts`
- `src/components/MoreToolsHub.tsx`
- `src/components/Launcher.tsx`
- `src/lib/launcherToolVisibility.ts`
- `src/lib/moreToolPresentation.ts`
- `src/i18n.ts`
- `src/components/MoreToolsHub.test.tsx`
- `src/App.moreToolsNavigation.test.tsx`
- 按现有测试布局新增或扩展 navigation、Launcher visibility/presentation 测试。
- `.ai-work-flow/index/feature-navigation.md`
- `.ai-work-flow/index/frontend-navigation.md`
- `.ai-work-flow/index/backend-navigation.md`

实施内容：

1. 新增 nav/tool ID `ai-request-capture`，接入 `MoreToolsSection`、alias map、More Tools 卡片和详情渲染。
2. 使用 Lucide `ScanSearch`（网络检查语义）或仓库当前版本中最接近的现成网络检查图标；不得手绘 SVG。
3. 新增 Launcher 快捷工具、内部 target、搜索文本和可见性开关；默认显示。读取旧 localStorage 对象缺少新 key 时回落为 `true`，不破坏既有用户选择。
4. 在 `src/i18n.ts` 同步完整中英文工具名、说明、配置、状态、筛选、详情、风险、确认、HAR、cURL、截断和错误文案。
5. 更新三个代码导航索引，记录独立前后端入口，并明确“不属于 Protocol Router”。

阶段关口：从 Launcher 和 More Tools 均可进入工具，返回目的地保持现有语义，默认可见与开关持久化兼容旧值，中英文无裸 key，三个索引路径真实存在。

#### 阶段 6：集成验证与最终审查

1. 先运行聚焦 Rust/React 测试，再运行全部规定命令。
2. 用本地 mock upstream 执行一次真实代理验收：普通 JSON、chunked request、大正文、SSE、上游错误、敏感 header、HAR、cURL；不得使用真实生产凭证作为 fixture。
3. 检查抓包数据库位于本机目录且共享 Git/iCloud 根目录无新增抓包文件。
4. 固定 review commit 后只执行一次 Standards + Spec 双轴最终审查；审查范围包含源码、测试、Cargo 依赖和三个索引。发现先报告用户，由用户决定是否修复，不自动进入修复循环。

## 接口与数据约束

- 代理监听端没有本地客户端鉴权；不得新增 token、密码、来源应用校验或请求签名。
- 不得对保存、详情、HAR 或 cURL 中的 Authorization、`x-api-key`、Cookie、Set-Cookie 或正文做脱敏、加密、删除或替换。唯一允许的内容缩减是每方向 2 MiB 样本上限，并必须附带截断元数据。
- 配置和数据库是本机运行数据，不走用户选择的存储后端。不得为了复用现有结构把抓包记录塞入 `storage.rs` 加密 JSON、`app_store` snapshot/outbox 或 Protocol Router stats。
- 网络字节流与捕获样本分离：存储、解析、事件或 UI 失败不得改变已建立连接的转发内容；捕获失败应记录/日志化并尽最大可能继续代理。
- 上游 path 前缀拼接规则固定为：去掉 Base URL path 的末尾 `/`，追加原始入站 path（保证恰好一个分隔 `/`），最后原样追加入站 query；Base URL 自身不得提供 query/fragment。
- 清理 headers 时只影响实际转发副本，存储的请求/响应 headers 使用清理前快照。cURL 使用可直接请求实际上游的端到端 header 集合，不重放 hop-by-hop 字段。
- 数据库和 IPC 不保证重建原始 HTTP wire framing、header 大小写、chunk 边界或 TLS 信息；验收关注语义等价转发、原始敏感值和正文样本字节，而不是抓取 TCP/TLS 包。
- 不支持 HTTP `CONNECT`、TLS MITM、WebSocket、HTTP/2 入站、多监听地址、远程访问、协议转换、请求编辑/重放队列或按记录删除。

## 验证

### Rust 测试

- 配置：默认值、端口边界、HTTP/HTTPS、自定义 path、query/fragment/空 host 拒绝、localhost/IPv4/IPv6/DNS loopback 同端口循环拒绝。
- URL：Base path、根 path、重复 slash 边界、百分号编码和 query 原样保留。
- HTTP：常规 methods、无正文、Content-Length、chunked request、普通/chunked/SSE response、HEAD、上游 4xx/5xx。
- 流式：首 chunk 非阻塞同步测试；请求/响应超过 2 MiB 后网络完整、样本截断、总字节正确。
- headers：Connection 指定字段和标准 hop-by-hop/Host/Content-Length 清理，Accept-Encoding identity，Authorization/API Key/Cookie/Set-Cookie 原值转发并入库。
- 错误：CONNECT、Upgrade、循环、bind 失败、上游连接失败、request/response transfer error、client disconnect、启动中断恢复。
- SQLite：schema/user_version、并发开始/完成、筛选、稳定分页、详情、清空竞态、7 天清理、不合法/非 UTF-8正文 BLOB。
- 解析：OpenAI/Anthropic/Gemini 普通与流式 token/model fixture，unknown 和 malformed 数据不影响记录。
- 导出：HAR 1.2、只含过滤后的已结束记录、二进制 Base64、敏感值、截断/错误注释；cURL 单引号、换行、任意字节、无正文和不完整 warning。

### React 测试

- API wrapper 的命令名、参数和事件名。
- 配置区默认值、保存、校验错误、Enabled 运行态、启动失败展示和持续明文警告。
- 列表筛选、分页、刷新、事件更新、选中详情竞态和空状态。
- Request/Response、Headers/Body、真实敏感值、文本/Base64、token、截断和传输错误展示。
- 清空与 HAR 二次确认、取消不 invoke、成功后的状态刷新；截断 cURL 的额外提示和复制内容 warning。
- More Tools 卡片/图标/详情、Launcher 默认可见与旧 localStorage 回退、入口搜索、导航及返回目的地、中英文 key。

### 必跑命令

按顺序执行并保存结果：

1. `git diff --check`
2. `npm test`
3. `npm run build`
4. `npm run lint`
5. `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
6. `cargo test --manifest-path src-tauri/Cargo.toml`
7. `cargo check --manifest-path src-tauri/Cargo.toml`

路由规则未授权浏览器自动化或可见浏览器，因此本计划不把 Playwright/截图作为实施命令；界面结构与交互通过 React 组件测试和 Tauri 手工验收记录验证，不启动浏览器自动化。

### 最终验收

- `127.0.0.1:17688` 可按已保存 Enabled 配置恢复；失败仅体现在状态，不阻止应用启动。
- 代理仅替换 origin，常规 HTTP、chunked 和 SSE 可用，首块不被完整捕获或数据库写入阻塞。
- 每方向超过 2 MiB 时客户端/上游收到完整数据，数据库和 UI 明确显示保存样本已截断。
- 明文 Authorization/API Key/Cookie/body 在详情、HAR 和 cURL 中保持真实值，页面持续警告，清空/导出均经过二次确认。
- HAR、cURL、筛选分页、7 天清理、provider/model/token 最佳努力解析符合契约。
- 抓包数据不进入 Git/iCloud，Protocol Router 代码和协议行为未被耦合或修改。
- Launcher、More Tools、导航、i18n 和三个索引完整；全部 npm/Cargo 命令通过。

## 审查关口

| 关口 | 进入条件 | 通过条件 |
|---|---|---|
| A：数据与配置 | 阶段 1 完成 | schema、明文往返、校验、分页、清理和 autostart 失败隔离测试通过 |
| B：代理正确性 | 阶段 2 完成 | SSE 首块、chunked、2 MiB tee、headers、拒绝面和错误状态端到端测试通过 |
| C：导出与敏感 UX | 阶段 3-4 完成 | HAR/cURL 保真、不完整警告、持续风险警告与两类二次确认测试通过 |
| D：产品集成 | 阶段 5 完成 | 两个入口、返回语义、默认可见、i18n、事件和三个索引通过 |
| E：最终审查 | 全部验证命令通过并形成稳定 review commit | Standards 与 Spec 双轴审查各完成一次，发现已报告用户 |

## 建议任务切片与依赖

| 任务 | 可独立验收的纵向结果 | 依赖 |
|---|---|---|
| T1 `capture-config-storage` | 配置、状态命令、SQLite schema/CRUD/分页/清理可测试，autostart 失败不阻塞应用 | 无 |
| T2 `capture-basic-proxy` | Enabled 配置可启动 loopback 代理，一次普通敏感 JSON 请求可转发并在 list/detail 查询 | T1 |
| T3 `capture-streaming-fidelity` | chunked/SSE、任意 method、header 清理、2 MiB tee 和错误状态完整通过 | T2 |
| T4 `capture-export-enrichment` | 三供应商元数据、HAR 1.2、二进制 Base64 和安全 cURL 可通过命令使用 | T3 |
| T5 `capture-workspace-ui` | 工具页可配置、看状态、筛选分页、查看 Request/Response，并响应更新事件 | T2；详情 token 展示依赖 T4 |
| T6 `capture-sensitive-actions` | HAR 保存/二次确认、清空二次确认、cURL 完整性提示和复制闭环 | T4、T5 |
| T7 `capture-navigation-i18n` | More Tools、Launcher、默认可见、导航返回、中英文和三个索引完整 | T5 |
| T8 `capture-integration-verification` | 全部 npm/Cargo 验证、本地 mock 代理验收、数据目录检查和稳定 review commit | T3、T4、T6、T7 |
| T9 `capture-final-review` | 固定提交上的 Standards + Spec 双轴审查结果报告 | T8 |

建议按 T1 → T2 → T3 → T4 推进后端主链；T5 可在 T2 契约稳定后并行准备但写入角色仍须串行，最终由 T6/T7 汇合到 T8。每个任务只在其关口通过后进入依赖任务，不以跳过测试或降低断言推进。

## 风险与控制

| 风险 | 控制措施 |
|---|---|
| 本地任意进程可调用代理并读取后续 UI/导出中的明文凭证 | 固定 loopback、不增加远程监听；页面持续警告且导出二次确认。该风险是用户明确接受的产品行为，不通过鉴权或脱敏改变 |
| 捕获层缓冲导致 SSE 或大请求阻塞 | 双向流式 tee，buffer 满后只停止捕获；同步屏障和大正文端到端测试作为硬门禁 |
| 上游配置形成自循环 | 保存、启动和请求前校验有效端口与 loopback 解析结果；运行状态记录拒绝原因 |
| SQLite 同步 IO 阻塞 runtime | 所有 DB 工作进入 `spawn_blocking`，网络任务不跨 await 持有连接/锁，流完成后批量收尾 |
| 清空时仍有 in-flight 请求重新出现 | 二次确认明确说明；不取消网络，完成后允许记录重新写入，保证代理优先 |
| HAR/cURL 因 2 MiB 或传输错误不能完整重放 | `complete=false`、UI 提示和导出/cURL warning；不得暗示样本完整 |
| HTTP 库不保留 wire framing/header casing | 数据契约只承诺语义 headers、真实值和 body 样本字节，不宣称 TCP/TLS 抓包 |
| 新依赖扩大构建面 | 只增加流式 HTTP 所需 Hyper 生态依赖，锁定 Cargo.lock，并以完整 cargo test/check 验证 |

## 范围外

- 接入 Protocol Router 路由、模型映射、鉴权或协议转换。
- 本地客户端鉴权、敏感信息脱敏、主密码加密、字段遮罩或导出时删除凭证。
- HTTPS CONNECT、TLS MITM、证书管理、WebSocket、透明系统代理、HTTP/2 入站或局域网监听。
- 修改、重放、批量重放请求，按单条删除，抓取 TCP 包或保留完整 wire framing。
- 自定义保留期、自定义每条 body 上限、按供应商配置解析器或同步抓包数据。
- 修改现有 Protocol Router、`proxy_http_request`、Git/iCloud 同步协议或普通内容存储。

## 假设

- OneSpace 当前目标桌面环境可写 `config::get_app_dir()`，并允许绑定 loopback 端口。
- “非文本 Base64”适用于 IPC/UI/HAR 表示；SQLite 始终保存捕获到的原始 BLOB。
- 用户已批准明文保存和导出的安全风险，以及 loopback 无客户端鉴权的访问模型。
- 当前规格已批准，且上述命令参数、事件 payload、SQLite 列拆分、HTTP 库选择和前端组件拆分属于不改变需求的实现决定。
- 本计划无待确认产品决策；在工作区依赖可正常安装和测试工具可运行的前提下，可直接进入实施。
