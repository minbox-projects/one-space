# Agent Note: A Single Global Network Proxy Snapshot Drives Generation-Scoped Transport and Relay Leases

Status: implemented

[English](2026-10-29-shared-network-proxy-lifecycle.md) | 中文

## Problem

代理行为此前分散在各个消费者中。AI 网关拥有独立的 `settings.proxy_url`，普通功能各自临时构建 HTTP 客户端，一些长串行与回退路径复用为整个批次捕获的客户端，ACME 账户构建在创建账户时捕获传输，JavaScript 更新插件则在托管传输之外自行抓取。保存可能在没有单一已校验来源的情况下发布运行时状态，因此 device 配置写入或客户端准备失败时，已保存设置与活动传输可能不一致。Zen 中继也以代理字符串和超时标识自身，切换时替换它可能中断已受理的 SSE 响应或留下未使用的子进程。

交付行为需要单一已校验的全局快照、无需重启应用的下一请求生效、显式清除继承代理的直连模式、回环旁路、保存失败后状态不变、应用托管 HTTP(S) 的完整覆盖，以及不保留第二份网关自有代理来源的迁移。

## Decision

- `src-tauri/src/proxy.rs` 拥有全局来源。保存先校验生效配置、准备传输、持久化 device 配置，然后才发布运行时快照；竞争的发布被串行化。`ProxyManager` 发布不可变 `ProxySnapshot`，其 generation 只在生效身份变化时递增：enabled、类型、host、port、username 或解密后的 password。等价保存或仅修改 `check_interval` 保持 generation 与已发布传输。共享构建策略先清除自动、系统与环境代理，启用时应用当前显式代理，禁用或缺失时保持显式直连。`localhost`、`127.0.0.1` 与 `::1` 直连。`apply_process_proxy_env` 与应用自有 Git 子进程 helper 从同一快照派生大小写环境变量与回环排除项；用户 Git 配置绝不被改写。`current_client()` 是生产接缝，以共享默认请求与连接超时从当前全局或测试快照构建客户端。成功发布后 `save_storage_config` 发出不含凭据的 `proxy-config-updated` 事件，仅携带新 generation。`proxy_generation` 暴露当前 generation，`PROXY_CONFIG_UPDATED_EVENT` 是对应更新事件的名称。
- 每个托管 HTTP 操作与每次重试都经共享构建策略解析当前快照。串行 RSS 来源、CLI 更新主请求与回退、AI Environments 模型列表、MCP npm 包版本元数据、HTTP 工具、协议路由、Uptime 探测与告警 Webhook、SSL DoH 与 Cloudflare 调用、Webhook 重放与镜像、OAuth/Gmail 桥、应用更新以及新启动的 Skills 与 Subagents Git 任务都不再复用为长批次捕获的客户端。草稿连接测试构建自己的传输且不发布任何状态。
- AI Gateway 客户端按代理 generation 缓存，并在每次尝试时解析当前传输。额度、OpenCode Go 用量与模板操作的 single-flight 身份在精确来源或模板身份之外加入 generation，因此同一 generation 内的匹配请求仍共享一次航班，而切换后受理的刷新独立开始。旧 generation 的迟到结果结算其原调用方，但绝不覆盖更新的缓存、模板或失败状态。网关配置推进到 schema 6；版本门控迁移只移除废弃的 `settings.proxy_url`，让 schema 5 文件跳过更早门控，保留其他所有值，不重写当前 schema 的读取，也绝不降级未来版本。
- Zen 中继身份为全局 generation、由快照派生的显式 proxy URL 与读取超时。新 generation 启动新子进程；已受理的普通与 SSE 响应持有 retiring 子进程的租约，最后一个响应体结束后子进程退出。等价保存复用当前子进程，网关或应用关闭会终止全部 current 与 retiring 子进程。传输缺失或不可用时该次尝试显式失败，绝不回退到直连 Zen 流量。
- SDK 边界遵循同一来源。`AcmeHttpClient` 实现 instant-acme 的 `HttpClient`，为每个 SDK HTTP 操作构建传输，两处账户构建点均注入它。原生更新使用官方 `tauri-plugin-updater` 2.9.0，并以 `configure_client` 在每次检查或下载客户端构建时解析当前快照；应用命令持有 typed `Update` 句柄与已验签字节，在关闭时清理，并保留官方验签与安装。前端更新门面使用这些原生命令与下载事件，GitHub 回退与版本说明请求经托管 Rust HTTP 桥，代理 generation 变化时使旧检测节流失效。JavaScript 更新包及其权限被移除。
- `ProxySettingsPanel` 是唯一代理编辑器。其文案说明下一请求生效、覆盖的应用托管流量、回环行为与外部进程边界。网关 UI 没有独立代理编辑器。SSH/SFTP、原生 TCP、Ping 与原始 DNS 探测、直连 TLS 检查、本地监听、应用 IPC 以及由外部 MCP CLI 进程执行的 MCP HTTP/SSE 连接探测仍在 HTTP 代理边界之外，该边界只覆盖 MCP npm 包版本元数据。

## Alternatives considered

- 保留各消费者代理覆盖并留下网关独立 URL：未采纳，因为同一用户操作会产生不同路由，废弃的网关值可能静默压过全局设置。
- 仅通过环境变量传播变更：未采纳，因为自动代理在客户端构建时被捕获，既有客户端不会观察到之后的环境变化，且禁用模式必须显式清除继承设置，而不是仅省略覆盖。
- 为每个请求无条件重建所有客户端：未采纳，因为它会丢弃同一 generation 内的连接复用并增加构建成本，却不改变路由结果。
- 新 generation 一发布就终止旧 Zen 中继：未采纳，因为已受理的普通或 SSE 响应会被中断；响应租约让旧工作排空，同时新工作使用新子进程。
- 实现替代更新协议或签名路径：未采纳，因为官方 updater 已拥有验签与安装，只需适配其客户端构建钩子与资源所有权。
- 引入按功能的代理框架或通用客户端工厂：未采纳，因为每个消费者都需要同一构建策略，而超时、重定向、头与 TLS 仍由调用方决定；共享策略加 generation 接缝更小并保持既有行为。

## Consequences

- 成功保存在不重启应用的情况下影响下一受理请求或重试，而已受理请求可在其现有传输上完成。等价保存或仅检查间隔变化不会重启传输、网关客户端或中继。
- 直连模式不继承操作系统、环境或此前代理。回环保持直连。有效但不可达的代理经该代理失败，校验、准备或持久化失败则保留最后有效的已保存与活动状态。
- 旧 generation 的航班与迟到结果不能覆盖更新的网关缓存、模板或失败状态，retiring 中继子进程在最后一个租约响应后退出。网关或应用关闭会停止所有自有中继子进程。
- Schema 6 只移除网关自有代理 URL。恢复该被移除值需要变更前的加密网关备份或手动重新输入；当前数据无法重建它。
- 覆盖范围仅限应用托管 HTTP(S)。原生 SSH/SFTP、TCP、Ping、原始 DNS、直连 TLS 检查、本地监听、IPC、由外部 MCP CLI 进程执行的 MCP HTTP/SSE 连接探测以及其他外部进程不在覆盖范围内，而 MCP npm 包版本元数据在覆盖范围内，设置文案已说明该边界。
- 原生 macOS 与 Windows GUI、ACL 与系统代理集成、真实服务商、证书机构、DNS 与服务行为，以及真实更新安装仍未执行；测试使用本地 fixture 与 mock。
- Supersession：本记录部分取代 [OpenCode Zen Free Uses an Isolated BoringSSL Loopback Relay Selected by Provider Template](2026-10-28-opencode-zen-sidecar.md) 中继身份、重启与关闭相关陈述，以及 [AI Gateway Avoids Redundant Configuration, History, Database, Stream Framing and Management Fetch Work](2026-10-19-ai-gateway-performance.md) 的额度、Go 用量与模板航班身份及逐请求传输陈述。二者的历史理由、备选、测量与已知缺口继续有效；两条记录均保留并交叉链接。
