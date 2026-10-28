# Agent Note: OpenCode Zen Free Uses an Isolated BoringSSL Loopback Relay Selected by Provider Template

Status: implemented

[English](2026-10-28-opencode-zen-sidecar.md) | 中文

## Problem

AI 网关无法服务 `OpenCode Zen Free` 服务商：用自身 TLS 栈直接发送的请求即使凭据有效，也会对免费模型收到 Zen 免费层返回的 HTTP 429，因此经网关连接的客户端始终无法到达上游。Zen 端点接受 BoringSSL 呈现的客户端 TLS 配置，但拒绝网关此前使用的直连栈。

把该客户端移入主应用进程并不可行。应用以 `vendored-openssl` 链接 `ssh2`，而 wreq 自带静态链接的 BoringSSL；二者都导出 OpenSSL 家族的 C 符号，因此单个二进制无法同时链接二者而不产生符号冲突。传输选择还必须来自持久配置，而不是可变的端点字符串，并且既有用户的服务商需要在不重写凭据、映射或价格的情况下完成迁移。

## Decision

- 传输是封闭的模板策略。`TransportStrategy` 为 `direct`（默认）或 `opencode_zen_sidecar`，持久化为模板的 `transport_strategy`；读取已存值时缺失或未知一律视为 `direct`，新保存的非法值被拒绝。`effective_transport_strategy` 只从服务商绑定的可解析模板解析，因此未绑定服务商、未知或已删除的模板 id、以及其他任何模板都保持直连，绝不查询 URL、名称或图标。
- 解析出的传输是运行时缓存。每个服务商携带 `#[serde(skip)]` 的传输值，由 `normalize_stored_config` 在读路径与写路径同时发布，已准备的模型池候选经同一解析器刷新；该值绝不落盘。仍在被服务商引用的模板删除失败并给出服务商名，且配置保持不变。
- 内置 `opencode-zen-free` 模板声明 Zen base URL `https://opencode.ai/zen/v1`、sidecar 传输、OpenCode 图标与八个启用映射：`zen-free-big-pickle` → `big-pickle`、`zen-free-beam` → `beam-free`、`zen-free-mimo` → `mimo-v2.6-flash-free`、`zen-free-nemotron-vl` → `nemotron-3.5-super-vl-free`、`zen-free-nemotron-lightning` → `nemotron-3.5-lightning-free`、`zen-free-deepseek` → `deepseek-v4.1-flash-free`、`zen-free-glm` → `glm-5.3-flash-free` 与 `zen-free-flint-40b` → `flint-40b-free`，连同各自的显示名与推理档。
- 配置 schema 提升到 5。首次读取更旧文件时，迁移只绑定没有模板、名称 trim 后恰为 `OpenCode Zen Free` 且归一化 base URL 恰为 `https://opencode.ai/zen/v1` 的服务商，保留 key、映射、价格与所有其他服务商，且幂等。改写前 best-effort 创建一次原始字节副本 `ai_gateway.json.bak-pre-schema5`，Unix 权限 `0600`，以 `create_new` 创建因此绝不覆盖已有备份；复制失败不阻断迁移。
- 中继是从嵌套 crate `src-tauri/zen-relay` 构建的独立进程，该 crate 自带 workspace，并固定 `wreq = "=6.0.0-rc.31"`（启用 `socks` 与 `stream`，不带 `prefix-symbols`）。它是仅回环的 hyper 服务，从 stdin 首行读取每进程随机 token，输出 `READY <port>` 或 `ERROR <message>`，在临时端口绑定 `127.0.0.1`，请求体上限 64 MiB，只接受 `/zen` 下的 HTTPS Zen base，拼接路径时不重复 `/v1`，过滤 hop-by-hop 与 sidecar 头以及 `accept-encoding`，原样流式中继响应，日志只含方法、路径与状态。其 wreq 客户端使用被接受的 BoringSSL 密码套件列表与 `X25519:P-256:P-384` 曲线配置。
- `src-tauri/src/ai_gateway/zen_sidecar.rs` 拥有该进程。它在首个 sidecar 尝试时惰性启动，就绪超时十秒，保持 stdin 打开用于生命周期与关闭检测，抽干 stderr，进程退出后重启，并在 `(proxy_url, read_timeout_secs)` 身份变化时重启；`stop` 幂等且两个网关关闭接缝都会调用。定位器依次检查 `ONESPACE_ZEN_RELAY_BIN`、可执行文件同目录、嵌套 debug 与 release 目标以及 `binaries/onespace-zen-relay-<triple>[.exe]`；缺失二进制时以预期路径使该次尝试失败，绝不回退直连。
- sidecar 转发使用专用 no-proxy 客户端与较长读取超时，保留上游路径与查询串，并在转发客户端头之后附加 `x-onespace-sidecar-token` 与 `x-onespace-upstream-base`。这两个头同时被排除在可转发的客户端头之外，因此客户端绝不能覆盖；服务商凭据、请求体改写与其他客户端头保持既有行为。
- 构建与打包是自动的：`src-tauri/scripts/build-zen-relay.mjs` 按环境或宿主三元组构建 dev 或 release，并把发布产物复制到 binaries 目录；`tauri.conf.json` 声明该外部二进制并把脚本前置到 dev 与 build 命令；发布工作流在确保 cmake 与 ninja 后为 macOS `x86_64`/`aarch64` 与 Windows `x86_64` 构建它。

## Alternatives considered

- 在主应用内链接 wreq 与 BoringSSL：未采纳，因为 `ssh2` 已链接的 vendored OpenSSL 与 BoringSSL 的 OpenSSL 家族符号在同一二进制内冲突；进程边界是唯一可行的隔离。
- 让 sidecar 在应用生命周期内常驻：未采纳，因为大多数会话不会使用 sidecar 服务商，惰性启动让空闲进程数与攻击面保持为零，同时每个活动会话只需付出一次启动成本。
- 按 URL、名称或图标启发式选择传输：未采纳，因为任何恰好类似 Zen 的端点都会让同一策略缺陷重现；模板绑定显式、可见并在保存时校验。
- 伪造 Bun 专属 TLS 指纹：未采纳，因为既脆弱又不必要；以真实 BoringSSL 客户端呈现被接受的密码套件与曲线配置已足以通过免费层，且不模仿其他客户端。
- 要求用户手动重新绑定服务商、不做迁移：未采纳，因为升级后既有服务商会继续以 429 失败；狭窄的精确名称与精确 URL 绑定保留其数据，一次性备份则让降级仍然可行。
- 让所有服务商都经中继转发：未采纳，因为直连服务商可与既有客户端和重试行为正常工作，通用中继会为不需要的流量增加进程与凭据跳转。

## Consequences

- sidecar 生命周期与安全由网关运行时拥有：进程仅回环、以经 stdin 交付的每进程 token 鉴权、就绪等待有界、在退出或身份变化后重启，并由两个关闭接缝终止，因此停止网关或应用后不留下中继进程。
- 失败是显式的。缺失二进制、握手失败或中继不可达都会以带路径的错误使该次尝试失败，而不是静默改为直连重试，同时其他模型池候选与所有直连服务商照常工作；429 等上游状态原样中继，既有分类与重试因此仍然适用。
- 回滚是配置级的：解绑服务商或改绑直连模板即恢复既有直连行为；降级应用须先恢复 `ai_gateway.json.bak-pre-schema5`，因为旧构建无法读取 schema 5 绑定；若该副本创建失败，降级不受支持。
- 构建桌面应用除 Rust 工具链外现在还需要 cmake 与 ninja，且每个发布目标都会构建并打包该嵌套 crate。主应用仍不依赖 wreq，因此其构建与其他服务商保持不变。
- 运行期证据保持有界：中继、生命周期、迁移、模板与转发行为由单元与行为测试覆盖，针对真实 Zen 端点的 macOS 冒烟是剩余的运行期验证，Windows 运行期冒烟未执行，Windows 证据仅有 CI 构建与跨平台 sidecar 测试。
