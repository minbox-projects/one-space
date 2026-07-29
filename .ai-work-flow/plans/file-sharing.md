# 文件共享工具实施计划

计划 ID：`file-sharing`

## 目标与成功标准

- 新增 `file-sharing` 工具：用户选择多个文件和一个私有 IPv4 地址，OneSpace 在该地址的随机端口启动临时 HTTP 服务。
- 每次会话生成 32 字节随机令牌；接收方通过二维码或链接打开响应式文件列表并逐个下载。
- 共享仅限当前应用进程，不上传云端、不进入同步或备份；切换页面或隐藏窗口不停止，手动停止或退出应用时立即失效。
- 首版仅支持可信局域网内的 HTTP 下载，不支持上传、目录、ZIP、公网、TLS、IPv6 或自动恢复。
- 验收时必须证明：多个文件可完整下载且哈希一致；大文件流式传输不会按文件大小占用内存；停止或退出后旧链接立即不可访问。

## 产品范围

### 发送方流程

1. 在 More Tools 或 Launcher 中进入“文件共享”。
2. 通过系统文件对话框选择一个或多个普通文件，可继续添加、移除或清空选择。
3. 从检测到的私有 IPv4 地址中选择用于共享的网卡地址。
4. 点击启动后，由操作系统分配空闲端口，界面展示二维码、可复制链接、共享文件列表和实时传输记录。
5. 用户可以切换 OneSpace 页面或隐藏主窗口；共享继续运行。
6. 用户手动停止或退出 OneSpace 时，监听器、令牌和正在进行的下载立即失效。

### 接收方流程

- 接收方扫描二维码或打开链接，进入不依赖外部资源的文件列表页。
- 页面根据浏览器 `Accept-Language` 显示中文或英文，并展示 OneSpace、文件数量、总大小、文件名、大小和逐文件下载操作。
- 同一令牌在会话停止前可由多个设备重复使用；首版不提供一次性下载和主机逐次审批。
- 接收端只能执行 `GET` 或 `HEAD`，不能浏览发送方目录、提交路径、上传文件或修改任何本地内容。

### 安全提示

- 使用普通 HTTP，随机令牌只能防止地址被猜中，不能防止同一网络中的被动流量监听；发送方页面持续展示“仅在可信局域网使用”的风险提示。
- 任何取得完整链接的人在会话有效期内都能下载全部共享文件；链接不得进入日志、同步数据、备份或持久化配置。
- 接收页不加载外部脚本、字体、图片或统计资源，避免通过 Referer 或第三方请求泄露令牌。
- 不自动修改系统防火墙；首次监听可能触发 macOS 网络访问提示，绑定失败需在界面显示可操作的错误。

## 后端设计

### 模块与依赖

- 新增门面 `src-tauri/src/file_sharing.rs` 和内部目录 `src-tauri/src/file_sharing/`，建议包含 `types.rs`、`runtime.rs`、`http.rs` 与 `tests.rs`。
- Tauri 命令集中在门面或 `commands.rs`；网络发现、路径校验、令牌、HTTP 路由、流式读取、取消和内存记录均隐藏在该模块的接口之后。
- 在 `src-tauri/src/lib.rs` 声明模块，并在 `src-tauri/src/app_runtime/run_app.rs` 注册命令及退出清理。
- 新增 `if-addrs = 0.15.0` 枚举网卡；新增 `tokio-util = 0.7.19`，启用 `io-util` 和 `rt`；现有 Tokio 增加 `fs` 特性。同步更新 `Cargo.lock`。
- 不修改 `app_store`、Cloud Drive、`storage.rs`、同步、备份或数据目录结构；本功能没有数据库和配置文件。

### 网络发现与绑定

- `file_sharing_networks` 每次调用重新枚举网卡，仅返回 RFC1918 IPv4：`10.0.0.0/8`、`172.16.0.0/12`、`192.168.0.0/16`。
- 排除 loopback、unspecified、multicast、公网、IPv6 和重复地址；结果按网卡名及地址稳定排序。
- 每个候选项包含稳定的会话内 `id`、`interfaceName` 和 `address`。启动输入使用 `networkId`，不直接信任前端传入监听地址。
- 启动时重新枚举并匹配 `networkId`，防止网卡在选择后失效或地址变化；匹配失败则不创建会话。
- 监听器只绑定所选私有地址，端口传 `0` 交给操作系统分配；绑定成功后从 `local_addr` 读取实际端口并构建 URL。

### 文件选择与会话创建

- 启动输入必须至少包含一个路径；去除完全重复的路径，随后对每个路径执行 canonicalize、metadata 和可读性检查。
- 只接受普通文件；目录、设备文件、无法读取或启动时已消失的文件均使整个启动原子失败，不允许部分共享。
- 符号链接在启动时解析为 canonicalized 目标，并保存解析后的路径，后续符号链接变化不能切换共享目标。
- 每个文件生成独立 opaque ID；接收方路由和 HTML 中从不出现真实路径。发送方 snapshot 可显示 source path 以便确认选择。
- 会话令牌由 CSPRNG 生成 32 字节随机数并编码为无 padding 的 Base64URL；每次启动生成新令牌和 session ID。
- 正在运行时再次调用 start 返回明确的 `already running` 错误，不隐式替换现有会话。启动失败保留现有前端选择。

### HTTP 接口与文件流

- 列表入口固定为 `GET /s/{token}/`，文件入口固定为 `GET /s/{token}/files/{fileId}`；相应 `HEAD` 请求返回同样 headers 但无 body。
- 无效、过期令牌和未知文件 ID 均返回内容一致的 `404`，根路径和其他未知路径也不暴露运行状态。
- 除 `GET`、`HEAD` 外的方法返回 `405 Method Not Allowed` 并设置 `Allow: GET, HEAD`。
- 文件响应设置 `Content-Length`、`Accept-Ranges: bytes`、`Content-Disposition: attachment` 和 `application/octet-stream`；文件名同时提供安全 ASCII fallback 与 UTF-8 `filename*`。
- 支持单段 byte range，包括 `start-end`、`start-` 和 suffix range，返回正确的 `206` 与 `Content-Range`；不可满足或语法错误的单段 range 返回 `416`。多段 range 不生成 multipart，忽略 Range 并返回完整 `200`。
- 使用 `tokio::fs::File`、seek/take 和 `tokio_util::io::ReaderStream` 流式发送，不把完整文件读入内存。
- 下载开始时重新打开文件并读取当前 metadata；文件在会话中被删除、替换为非普通文件或变得不可读时，该次请求返回失败并记录错误。
- 文件内容不做副本或快照；启动后内容发生修改时，后续请求读取 canonicalized 目标的当前内容。响应长度以该次打开时的 metadata 为准。

### 浏览器页面与响应安全

- 服务端生成静态响应式 HTML，仅将经过 HTML escaping 的文件名、格式化大小和 opaque 下载地址插入页面。
- 所有页面和文件响应设置 `Cache-Control: no-store`、`Referrer-Policy: no-referrer`、`X-Content-Type-Options: nosniff`。
- HTML 设置 CSP：`default-src 'none'`，仅允许内联样式；不使用脚本、表单、iframe 或外部资源。
- 响应 header 值剔除 CR/LF 和控制字符，避免文件名造成 header injection。

### 生命周期与传输记录

- 运行时持有监听任务、会话 cancellation token、文件映射、AppHandle、累计摘要和最近传输记录；不得在异步文件读取期间持有全局 mutex。
- 每次文件响应创建传输记录，字段包括来源 IP、文件 ID/名称、状态、开始/结束时间、已发送字节、本次预期响应字节和错误。
- 状态固定为 `in_progress`、`completed`、`client_disconnected`、`cancelled`、`failed`。Range 请求完成时按本次 range 长度计为完成。
- 仅保留最近 200 条传输明细；被淘汰记录仍计入 `completed`、`failed`、`cancelled`、总发送字节等累计摘要。
- 进度事件最多每 250ms 发出一次，避免逐 chunk 触发 IPC。事件仅通知状态已变化，完整状态由 `file_sharing_status` 返回。
- 手动停止先从共享状态移除有效令牌，再触发 cancellation token、关闭监听器并中断正在发送的流；所有 `in_progress` 记录转为 `cancelled`。
- 停止后 snapshot 保留文件摘要、传输记录和累计统计，但 `shareUrl` 置空且不再保留令牌或可打开的文件句柄。下一次启动清除旧 snapshot。
- `request_shutdown` 必须是退出路径可直接调用的非阻塞清理入口；托盘退出和 `RunEvent::Exit` 都调用它。主窗口 CloseRequested 仅隐藏窗口，不停止共享。
- 监听任务意外退出时将 `running` 置为 false、撤销令牌、记录 `lastError` 并发出 session 更新事件。

## Tauri 接口

### 命令

1. `file_sharing_networks() -> Vec<FileSharingNetwork>`
2. `file_sharing_start(input: FileSharingStartInput) -> FileSharingSnapshot`
3. `file_sharing_status() -> FileSharingSnapshot`
4. `file_sharing_stop() -> FileSharingSnapshot`

命令错误继续采用仓库现有 `Result<T, String>` 约定，但错误字符串必须区分无文件、路径无效、非普通文件、不可读、网卡失效、绑定失败和已在运行。

### 数据类型

- `FileSharingNetwork`：`id`、`interfaceName`、`address`。
- `FileSharingStartInput`：`networkId`、`paths`。
- `FileSharingFile`：`id`、`name`、`sourcePath`、`size`、`modifiedAt`。
- `FileSharingTransfer`：`id`、`fileId`、`fileName`、`clientAddress`、`state`、`startedAt`、`finishedAt`、`bytesSent`、`responseBytes`、`error`。
- `FileSharingSummary`：`activeTransfers`、`completedTransfers`、`failedTransfers`、`cancelledTransfers`、`bytesSent`、`droppedTransferRecords`。
- `FileSharingSnapshot`：`running`、`sessionId`、`address`、`port`、`shareUrl`、`startedAt`、`stoppedAt`、`files`、`transfers`、`summary`、`lastError`。
- 所有 Rust 类型使用 `serde(rename_all = "camelCase")`；枚举值使用 `snake_case`，与 TypeScript 字面量一致。

### 事件

- 统一事件名：`file-sharing-updated`。
- payload：`{ kind: "session" | "transfer" }`，不携带令牌、路径或完整 snapshot。
- 前端收到事件后合并并节流调用 `file_sharing_status`；组件重新挂载或重新可见时主动校准一次，过期请求不得覆盖新 snapshot。

## 前端实现

### IPC 与状态

- 新增 `src/lib/fileSharing.ts`，定义完整 TypeScript 类型、四个类型化 invoke 包装器、统一错误格式化和事件订阅 helper。
- 新增对应测试，断言命令名、参数 camelCase、错误归一化、事件 payload 和 unsubscribe 行为。
- 浏览器/Vite 非 Tauri 环境显示“需要 OneSpace 桌面端”，禁用文件选择后的启动能力，不尝试创建本地服务器。

### 工具页面

- 新增 `src/components/FileSharingTool.tsx`；只有出现明确的复用或复杂度时，才在 `src/components/fileSharing/` 内拆分纯展示模块。
- 未运行状态提供多文件选择、继续添加、单项移除、清空、网卡 Select、重新扫描和启动按钮。
- 文件选择使用现有 `@tauri-apps/plugin-dialog`：`open({ multiple: true, directory: false })`；取消不改变当前选择。
- 没有文件、没有可用网卡或正在启动时禁用启动；网卡变化时保留文件选择，文件校验失败时保留所有选择并展示错误。
- 运行状态展示醒目的 HTTP 风险警告、运行地址、二维码、复制链接、启动时间、停止按钮、共享文件列表、累计摘要和实时传输表。
- 使用 `qrcode.react = 4.2.0` 生成 QR SVG，并同步更新 `package.json` 与 lockfile；二维码内容只能来自后端返回的 `shareUrl`。
- 正在传输时点击停止需二次确认并说明会中断下载；没有进行中传输时可直接停止。
- 停止后显示本次已结束状态与最终摘要，不再显示可复制链接和二维码；用户可重新选择文件并启动新会话。
- 长路径、文件名、IP、错误和数字不得撑破布局；文件与传输列表使用稳定高度、滚动区域和响应式列布局。

### 导航与可见性

- 使用 nav/tool ID `file-sharing`，接入 `MoreToolsSection`、alias map、More Tools 卡片和详情渲染。
- 在 `moreToolPresentation` 使用 Lucide `Share2` 和符合现有多色工具集合的颜色，不手绘 SVG。
- 在 Launcher 增加默认可见的内部工具项、搜索文本、说明和 `target: "file-sharing"`。
- 在 `launcherToolVisibility` 增加 `file-sharing: true`；读取旧 localStorage 对象缺少该键时补为 true，同时保留用户已有选择。
- 保持现有返回语义：从 Launcher 进入时返回 Launcher，从 More Tools 目录进入时返回工具列表。

### 国际化与文档

- 在 `src/i18n.ts` 同步新增完整中英文资源：工具名、说明、风险、文件选择、网卡、运行状态、二维码、复制、停止确认、传输状态、空状态和错误。
- 更新 `README.md` Developer Utilities 概览。
- 在 `docs/USAGE.md` 的 More Tools 章节新增文件共享说明，不无关重排其他章节。
- 在应用内 `Documentation.tsx` 增加文件共享入口并指向新增使用手册 anchor。
- 更新 `.ai-work-flow/index/feature-navigation.md`、`frontend-navigation.md`、`backend-navigation.md`，记录前后端入口及“临时内存态、独立于 Cloud Drive/同步”的模块边界。

## 测试计划

### Rust 单元测试

- 网卡候选过滤：私有 IPv4、loopback、公网、IPv6、重复项和稳定排序。
- 启动校验：空列表、重复路径、目录、损坏路径、不可读文件、符号链接 canonicalize、过期 network ID 和运行中重复启动。
- 令牌与路由：有效 token、错误 token、停止后 token、未知文件 ID、根路径、额外路径、禁止方法和无路径泄露。
- HTML/header 安全：包含 `<script>`、引号、Unicode、CR/LF 和控制字符的文件名不会产生注入。
- HTTP：完整 GET、HEAD、三种单 range、越界/错误 range、多 range 回退、Content-Disposition、Content-Length、206 和 416。
- 流式行为：大文件在首块后即可被客户端读取，传输内存不包含完整文件，客户端下载结果哈希一致。
- 生命周期：客户端断开、手动停止取消活动流、幂等停止、重新启动清空历史、意外 listener 退出和 `request_shutdown`。
- 记录：进度、完成、失败、取消、Range 字节数、200 条明细上限、淘汰计数和累计摘要。
- HTTP 集成测试使用 loopback 测试入口或注入候选，不依赖 CI 机器真实私有网卡，也不访问公网。

### 前端测试

- IPC 包装器的命令、参数、类型、错误和事件取消订阅。
- 文件选择取消、追加、去重展示、移除、清空和启动失败后选择保留。
- 无网卡、重新扫描、默认选择、网卡失效错误和启动中禁用状态。
- 运行时二维码、复制链接、风险提示、文件列表、summary、实时传输更新和过期 status 请求保护。
- 停止时有/无活动下载的确认行为，停止后隐藏有效链接，重新启动清空旧摘要。
- More Tools 卡片、Launcher 跳转、返回目的地、默认可见性及旧 localStorage 兼容。
- 中英文关键文案存在且不显示裸 i18n key；长文件名和错误信息使用稳定容器。

### 验证命令

```bash
npm test
npm run lint
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

### 真实环境验收

- 在同一局域网使用手机和另一台桌面设备分别扫码/打开链接，下载多个文件并核对哈希。
- 使用包含空格、中文、引号和超长名称的文件验证页面、下载名称和布局。
- 使用大文件验证首块响应、持续进度、完整下载、HEAD、Range 和断点续传。
- 在 Wi-Fi、以太网或 VPN 并存的机器上确认候选地址可选择，链接指向所选私有地址。
- 切换到其他 OneSpace 页面、关闭主窗口使其隐藏，确认共享仍运行；手动停止和退出后旧链接立即返回不可用。
- 检查停止活动下载会中断传输并记录 cancelled，客户端断开记录 client_disconnected。
- 检查本机数据目录、Git/iCloud 同步目录、备份和配置中均没有令牌、文件列表或传输记录。
- 在手机和桌面宽度检查发送端与接收端无重叠、溢出或不可操作控件。

## 实施顺序与阶段门禁

1. 实现 Rust 类型、网卡发现和路径校验；验证候选过滤及原子启动输入测试。
2. 实现运行时、令牌、HTTP 页面、GET/HEAD/Range 和取消；验证本地 HTTP 集成及大文件流式测试。
3. 实现内存传输记录、summary、事件节流和退出清理；验证完整状态机与旧链接失效。
4. 实现 TypeScript IPC 和 FileSharingTool；验证组件状态、二维码、停止确认和竞态处理。
5. 接入 More Tools、Launcher、i18n、文档和三个导航索引；验证所有入口和旧可见性兼容。
6. 运行完整前后端检查并执行双设备真实局域网验收；未完成真实设备下载和停止失效验证前，不宣称功能完成。

## 明确不做

- 不支持接收端上传、拖放上传或指定接收目录。
- 不支持文件夹递归、ZIP 打包、在线预览、媒体播放或文本编辑。
- 不支持公网中继、NAT 穿透、mDNS、账号体系、云同步或跨设备发现。
- 不支持 HTTPS、自签名证书、应用层加密或 PIN 配对。
- 不支持固定端口、保存网卡选择、自动启动、重启恢复或后台持久历史。
- 不复用或改造 Cloud Drive、Protocol Router、AI Request Capture 或 app_store。
