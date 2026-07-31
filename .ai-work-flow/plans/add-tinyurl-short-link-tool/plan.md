# 新增 TinyURL 短链接生成工具

## Plan Metadata

- plan-id: `add-tinyurl-short-link-tool`
- status: `ready-for-implementation`

## Problem Statement

OneSpace 的 More Tools 当前包含密码生成、JSON 解析、文件分享等实用工具，但没有把长 URL 转换为可公网访问短链接的能力。现有文件分享功能只生成依赖应用运行状态和局域网地址的临时链接，不能满足向外部接收者长期分享简短 URL 的场景。目标用户需要离开 OneSpace 或手动访问第三方网站，增加了工作流中断和重复操作；因此需要在桌面端内接入稳定的第三方短链服务，同时明确第三方依赖、凭据安全和本地历史的边界。

## Solution

在 More Tools 中新增“生成短链接”工具，并接入 Launcher。工具在页面内完成 TinyURL API Token 的首次配置、替换和删除，通过 Tauri 命令从 Rust 端调用 TinyURL 官方 `POST https://api.tinyurl.com/create` 接口，成功后展示并复制公网短链接。Token 复用现有加密 secret 存储且不会被读取回前端；生成历史以明文 JSON 保存在当前设备的 `localStorage`，提供再次复制、删除单条和清空能力。首版只支持标准生成流程，不引入自定义 alias、过期时间、统计、远端撤销或多供应商抽象。

## Goals and Success Criteria

- 用户可从 More Tools 卡片和 Launcher 打开“生成短链接”工具，导航、返回和工具可见性设置符合现有行为。
- 未配置 Token 时，工具提供就地配置入口；Token 可保存、替换和删除，输入默认遮罩，已保存明文不会由后端返回前端。
- 输入合法的 `http://` 或 `https://` URL 后，可通过 TinyURL 官方 API 获得并展示短链接；非法、空白或非 HTTP(S) 输入不会发起网络请求。
- 创建请求进行中禁止重复提交；成功后保留原始链接和短链接，并提供明确的复制成功或失败反馈。
- 最近 50 条成功生成记录按时间倒序保存在当前设备的明文 `localStorage` 中，应用重启后仍可读取；用户可复制历史短链、删除单条或确认后清空全部历史。
- 删除本地历史不会调用 TinyURL 删除接口，也不会暗示远端短链接已失效。
- 缺少或失效 Token、限流、TinyURL 拒绝请求、服务端故障、超时、网络失败、异常响应、剪贴板失败及历史读写失败均产生可区分且不泄露 Token 的用户反馈。
- 前端单元测试覆盖核心交互和本地历史，Rust 测试覆盖 URL 校验、TinyURL 请求/响应解析及错误映射；导航相关回归测试、TypeScript 构建、Lint 和 Rust 检查通过。

## User Stories

- 作为 OneSpace 用户，我可以输入一条长的 HTTP(S) 链接并生成 TinyURL，以便在聊天、文档或工单中分享更简短的公网地址。
- 作为首次使用者，我可以直接在工具页面配置 TinyURL API Token，而不需要寻找独立设置入口。
- 作为重复使用者，我可以查看最近生成的短链接并再次复制，而不需要重新调用第三方 API。
- 作为注重凭据安全的用户，我可以确认 Token 被加密保存、默认不可见，并可随时替换或删除。
- 作为维护者，我可以根据稳定错误代码区分配置、输入、网络、限流和服务异常，而不依赖解析后端错误文案。

## Scope

包含：

- 新增 Short Link 工具页面、More Tools 卡片、Launcher 项及对应中英文文案。
- 新增前端 Tauri 调用封装、Token 配置状态、生成表单、结果区和本地历史交互。
- 新增 Rust 短链接模块、专用 Tauri 命令、TinyURL HTTP 客户端和结构化错误类型。
- 复用现有 secret 加密存储保存 TinyURL Token。
- 使用当前设备的 `localStorage` 明文保存最近 50 条生成历史。
- 更新导航、工具展示映射、Launcher 可见性及相关测试。

边界：该工具是 TinyURL 客户端，不在 OneSpace 内托管重定向服务；短链接的可用性、保留策略和服务配额受 TinyURL 控制。

## Implementation Decisions

- 固定使用 TinyURL 官方 API，不增加供应商选择器或通用短链 provider 抽象。首版只有一个实现，提前抽象会增加配置、错误模型和测试复杂度。
- 从 Rust 端使用现有 `reqwest` 依赖调用 TinyURL，前端只调用 Tauri 命令。这样避免浏览器 CORS 和 CSP 变更，并使已保存 Token 不需要返回前端或进入浏览器网络请求。
- 提供专用命令读取“是否已配置”、保存、删除 Token 和创建短链；不让 Short Link UI 调用通用 `get_secret` 读取 Token 明文。Token 使用固定 secret key `tinyurl_api_token`。
- 保存 Token 时只做去空白和非空校验，不为测试 Token 创建无意义短链；Token 有效性在首次真实生成请求中验证。失效 Token 映射为专用鉴权错误并保留用户输入。
- 前端与后端都使用结构化 URL 解析并仅允许 `http`、`https`，后端校验作为最终安全边界。拒绝 `javascript:`、`data:`、`file:`、相对地址和缺少有效主机的输入。
- TinyURL 创建请求只发送必需的 `url` 字段，不启用 `alias`、自定义域名、标签或过期配置；成功响应只依赖 `data.tiny_url` 和必要的原始 URL 信息。
- 历史按用户明确选择保存在明文 `localStorage`，key 为 `onespace:short-link-history`。schema 为对象数组，每项包含 `id`、`longUrl`、`shortUrl`、`createdAt`；成功生成时写入，最新记录置顶并截断到 50 条。
- 历史不是同步数据，不进入通用加密内容存储、app store 或云同步。损坏或不符合 schema 的历史数据会被丢弃并提示，不尝试猜测性迁移。
- 删除和清空只操作本地历史。界面文案明确“删除本地记录”，不调用 TinyURL 远端管理 API。
- 使用现有组件、Toast、确认对话框、Lucide 图标和 More Tools 展示约定；不增加新的前端依赖。工具展示图标使用现有 Lucide 链接类图标，并在 `moreToolPresentation` 中配置符合现有工具集合的独立强调色。
- 放弃直接从前端调用 TinyURL，因为这会让 Token 长期进入前端状态并依赖 CORS；放弃复用文件分享本地 HTTP 服务，因为它不能提供稳定公网重定向；放弃首版支持 alias、过期、统计和远端撤销，以避免 TinyURL 套餐能力与管理状态扩张。

## Implementation Changes

1. 建立 Rust 短链接边界。
   - 新增短链接模块并在 Tauri runtime 注册 `short_link_config_status`、`short_link_save_token`、`short_link_delete_token` 和 `short_link_create` 命令。
   - 配置状态命令只返回布尔状态；保存命令接收一次明文 Token 并立即写入现有加密 secret store；删除命令移除固定 secret key。
   - 创建命令解析并校验 URL，从 secret store 读取 Token，使用 Bearer 认证调用 TinyURL `/create`，设置有限请求超时，解析 `data.tiny_url`，且不记录请求头、Token 或包含敏感查询参数的完整 URL。
   - 将错误映射为稳定代码：`not_configured`、`invalid_url`、`authentication_failed`、`rate_limited`、`request_rejected`、`service_unavailable`、`network_error`、`invalid_response` 和 `storage_error`；可附带经过清理的安全详情，但前端不依赖详情做分支。

2. 实现前端工具及本地历史。
   - 新增 Short Link 工具组件和轻量 Tauri 调用封装，加载时并行读取 Token 配置状态与本地历史。
   - 未配置 Token 时显示页内配置区；已配置时只显示状态和“替换”“删除”操作，不回填旧 Token。Token 输入使用密码类型和显示/隐藏按钮，保存成功后清空组件内明文。
   - 提供长链接输入、生成按钮、进行中状态、当前结果和复制操作。输入变化不清除最近一次成功结果；失败时保留输入，便于修正或重试。
   - 成功生成后先保留当前结果，再把记录写入 `localStorage`。若历史持久化失败，仍允许复制本次结果，同时提示该记录未能持久保存。
   - 严格解析历史 schema、按 `createdAt` 倒序展示并限制 50 条；支持复制、删除单条和确认后清空。写入失败时不把操作报告为成功。

3. 接入工具导航和展示体系。
   - 为 More Tools section、别名映射、展示信息和 Launcher 可见性联合类型增加稳定 ID `short-link`，默认可见性与现有实用工具保持一致。
   - 在 More Tools Hub 添加工具卡片和详情分发，在 Launcher 添加中英文可搜索条目，并补齐 App 的导航标题/面包屑所需映射。
   - 在集中 i18n 资源中增加工具名称、简介、字段标签、配置状态、确认文案、复制反馈和全部错误代码对应文案，避免在组件中硬编码中英文分支。

4. 补齐测试与回归验证。
   - 为 Short Link 前端组件添加独立测试，并更新 More Tools、Launcher、导航、展示映射和工具可见性测试中的穷举集合。
   - 为 Rust 模块把 HTTP 客户端/base URL 作为内部可替换依赖，使用本地 mock HTTP server 验证请求方法、Bearer 头、JSON body、成功解析、超时和状态码映射，不向真实 TinyURL 发测试请求。
   - 运行聚焦测试后执行全量前端测试、Lint、构建及 Rust 测试/检查。

## Public Interfaces

- 新增 More Tools/Launcher 稳定工具 ID：`short-link`。
- 扩展 `MoreToolsSection`、Launcher 工具可见性类型及展示映射，使其包含 `short-link`。
- 新增 Tauri 命令：
  - `short_link_config_status() -> { configured: boolean }`
  - `short_link_save_token(token: string) -> { configured: true }`
  - `short_link_delete_token() -> { configured: false }`
  - `short_link_create(url: string) -> { longUrl: string, shortUrl: string }`
- Tauri 失败结果使用结构化错误 `{ code, message? }`；`code` 使用 Implementation Changes 中定义的稳定集合，`message` 只承载安全的诊断摘要。
- 新增本地明文持久化契约：`localStorage["onespace:short-link-history"]`，值为最多 50 项的 JSON 数组；每项 schema 为 `{ id: string, longUrl: string, shortUrl: string, createdAt: string }`，时间使用 ISO 8601 字符串。
- 新增内部 secret key：`tinyurl_api_token`。该值不属于可导出配置、历史 schema 或前端读取接口。
- 外部服务契约固定为 TinyURL Bearer Token 认证和 `POST https://api.tinyurl.com/create`。没有面向 OneSpace 外部调用方的新 API、CLI、事件或数据库 schema。

## Data Flow and Failure Modes

1. 工具加载时，前端调用配置状态命令并解析 `localStorage` 历史；两者相互独立，单项失败不阻止另一项显示。
2. 用户保存 Token 时，明文仅通过一次 Tauri IPC 进入 Rust，随后写入现有加密 secret 文件；Rust 只返回配置状态，前端立即清空 Token 输入。
3. 用户提交长链接时，前端先校验 URL 并锁定提交按钮；Rust 再次校验、读取 Token、构造 Bearer 请求并调用 TinyURL。
4. TinyURL 成功响应经 Rust 提取为最小结果返回前端；前端展示结果并把新记录写入本地明文历史。复制动作独立发生，复制失败不删除结果或历史。
5. 缺少 Token 时返回 `not_configured` 并展开配置区；401/403 映射为 `authentication_failed`；429 映射为 `rate_limited`；其他 4xx 映射为 `request_rejected`；5xx 映射为 `service_unavailable`；连接和超时映射为 `network_error`；缺少合法 `data.tiny_url` 映射为 `invalid_response`。
6. 所有错误反馈都不得包含 Authorization header、Token、原始 TinyURL 响应中的敏感调试数据或完整长链接。失败后释放进行中状态并保留用户输入。
7. 历史 JSON 损坏时清除该 key、显示一次恢复提示并从空历史继续；`localStorage` 读写被拒绝或配额不足时，当前会话中的生成结果仍可使用，但明确提示历史未加载或未保存。
8. 删除 Token 不删除历史和既有短链接；删除或清空历史不删除 Token，也不改变 TinyURL 远端链接。应用退出、离线或回滚不会影响已经创建的 TinyURL 重定向。

## Testing Decisions

- 前端组件测试覆盖：未配置/已配置状态、Token 输入遮罩与替换/删除、保存后不回填明文、空 Token 校验、HTTP(S) URL 验证、提交防重复、成功结果、复制成功/失败和各稳定错误代码的文案映射。
- 历史测试覆盖：成功生成即写入、重载恢复、最新在前、50 条截断、schema 损坏恢复、写入失败提示、复制历史项、删除单条、清空确认以及本地删除不触发远端命令。
- 导航测试覆盖：More Tools 卡片、详情分发、返回行为、Launcher 搜索/打开、工具可见性默认值与持久化、`moreToolPresentation` 穷举映射。
- Rust 单元/集成测试覆盖：空 Token、secret 状态与删除、URL scheme/host 校验、正确的 `POST /create`、Bearer 认证和请求 JSON、成功响应解析、401/403、429、其他 4xx、5xx、超时、连接失败及畸形成功响应。
- 安全回归检查：测试错误序列化和日志中不出现测试 Token；前端 mock 断言配置状态接口不返回 Token。
- 验收命令：先运行新增测试的 Vitest 聚焦命令和 Rust 模块测试，再运行 `npm run test`、`npm run lint`、`npm run build`、`cargo test` 与 `cargo check`（Rust 命令在 `src-tauri` 目录执行）。不使用真实 TinyURL Token，也不在自动化测试中访问生产 API。

## Rollout and Compatibility

- 新工具通过新增 ID 接入现有 More Tools 和 Launcher，不改变既有工具 ID、路由或持久化数据；默认可见并沿用现有 Launcher 可见性迁移/默认合并行为。
- 不需要数据库迁移、服务端部署、DNS、CSP 白名单或前端依赖变更。Rust 使用仓库已有 `reqwest`、Serde 和 secret store 能力。
- 首次使用要求用户自行从 TinyURL API Settings 获取 Token；没有 Token 时其他 OneSpace 功能不受影响。
- 历史 key 为全新 key，不迁移书签、文件分享或其他工具数据；历史明确为当前设备明文数据且不进入同步、备份扩展范围或远端服务。
- 发布时应在支持的桌面平台验证 Tauri 网络访问和 secret 存储；TinyURL 不可用时工具降级为可查看/复制既有本地历史，不能生成新链接。
- 回滚到不含该工具的版本不会影响其他数据，也不会让已生成短链接失效；新增的 secret 和 `localStorage` key 可保留，后续重新升级时继续使用。若必须人工清理，可删除 `tinyurl_api_token` secret 与 `onespace:short-link-history`，不需要回滚 schema。

## Out of Scope

- 在 OneSpace 仓库中建设或部署公网短链重定向服务、数据库、域名或 DNS。
- 支持 TinyURL 之外的供应商、供应商切换、账户 OAuth 或团队账户管理。
- 自定义 alias、品牌域名、标签、过期时间、批量生成、二维码、访问统计或套餐管理。
- 从应用远端删除、禁用或编辑 TinyURL；本地历史删除不会撤销链接。
- 短链接历史加密、跨设备同步、云备份、导入导出或与 Bookmarks 合并。
- 自动展开短链接、检查目标内容安全性、恶意链接检测或可用性持续探测。
- 重构现有通用 secret API、Random Password 历史或其他 More Tools 组件。

## Assumptions

- 用户拥有可调用 TinyURL 官方 API 的有效账号和 Bearer Token，并接受 TinyURL 的服务条款、配额及可用性约束。
- TinyURL 的创建契约在实施时仍为 Bearer Token 认证的 `POST /create`，请求至少包含 `url`，成功响应包含 `data.tiny_url`；实现者应按官方 OpenAPI 定义建立严格但最小的响应类型。
- 用户已明确接受短链接历史在当前设备以明文 `localStorage` 保存；API Token 不适用这一决定，仍必须进入现有加密 secret store。
- 首版界面提供中文和英文文案，命名分别为“生成短链接”和“Short Link”。
- 每次成功创建均形成一条历史记录；不按原始 URL 或短链接自动去重，以保留真实生成事件，超过 50 条时淘汰最旧记录。
- 系统剪贴板权限和 TinyURL 网络连通性可能不可用，工具必须反馈失败但不负责修改操作系统权限或网络代理设置。

## Further Notes

TinyURL 官方文档来源为 `https://api.tinyurl.com`。实施和测试不得使用仓库内硬编码 Token；开发者手工验证时也应使用个人测试 Token，并确保控制台、截图和提交内容不包含凭据。
