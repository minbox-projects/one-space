# Agent Note: OpenCode Zen Free Uses an Isolated BoringSSL Loopback Relay Selected by Provider Template

Status: implemented

[English](2026-10-28-opencode-zen-sidecar.md) | 中文

## Problem

AI 网关无法服务 `OpenCode Zen Free` 服务商：用自身 TLS 栈直接发送的请求即使凭据有效，也会对免费模型收到 Zen 免费层返回的 HTTP 429，因此经网关连接的客户端始终无法到达上游。Zen 端点接受 BoringSSL 呈现的客户端 TLS 配置，但拒绝网关此前使用的直连栈。根因是 base URL：官方 OpenCode 内置服务商 API 是 `https://opencode.ai/inference/openai/v1`，而网关持久化的是遗留 `https://opencode.ai/zen/v1`，后者即使携带真实 `x-opencode-*` 头与 agent 请求体也返回 HTTP 429；官方 CLI 对推理 base 的退出码为 0。

把该客户端移入主应用进程并不可行。应用以 `vendored-openssl` 链接 `ssh2`，而 wreq 自带静态链接的 BoringSSL；二者都导出 OpenSSL 家族的 C 符号，因此单个二进制无法同时链接二者而不产生符号冲突。传输选择还必须来自持久配置，而不是可变的端点字符串，并且既有用户的服务商需要在不重写凭据、映射或价格的情况下完成迁移。

## Decision

- 传输是封闭的模板策略。`TransportStrategy` 为 `direct`（默认）或 `opencode_zen_sidecar`，持久化为模板的 `transport_strategy`；读取已存值时缺失或未知一律视为 `direct`，新保存的非法值被拒绝。`effective_transport_strategy` 只从服务商绑定的可解析模板解析，因此未绑定服务商、未知或已删除的模板 id、以及其他任何模板都保持直连，绝不查询 URL、名称或图标。
- 解析出的传输是运行时缓存。每个服务商携带 `#[serde(skip)]` 的传输值，由 `normalize_stored_config` 在读路径与写路径同时发布，已准备的模型池候选经同一解析器刷新；该值绝不落盘。仍在被服务商引用的模板删除失败并给出服务商名，且配置保持不变。
- 内置 `opencode-zen-free` 模板声明 Zen 推理 base URL `https://opencode.ai/inference/openai/v1`、保持不变的模型清单 URL `https://opencode.ai/zen/v1/models`（`source` 与之相同）、generic 同步策略、sidecar 传输、OpenCode 图标与八个启用映射：`zen-free-big-pickle` → `big-pickle`、`zen-free-beam` → `beam-free`、`zen-free-mimo` → `mimo-v2.6-flash-free`、`zen-free-nemotron-vl` → `nemotron-3.5-super-vl-free`、`zen-free-nemotron-lightning` → `nemotron-3.5-lightning-free`、`zen-free-deepseek` → `deepseek-v4.1-flash-free`、`zen-free-glm` → `glm-5.3-flash-free` 与 `zen-free-flint-40b` → `flint-40b-free`，连同各自的显示名与推理档。
- 免费模板是唯一按上游价格过滤模型清单同步的模板。`src-tauri/src/ai_gateway/templates/source.rs` 跳过所有纯字符串条目，只保留真实上游嵌套价格形状证明其免费的对象条目：存在的 `cost.text.input.price` 与 `cost.text.output.price` 为 0，且 `input.cache` 中出现的读、写价格为 0。缺失、畸形、非对象或正价条目均排除。已知上游 id 保留其本地别名、显示名、启用状态、推理档与上下文窗口；新免费模型派生 `zen-free-<去掉末尾 -free 的上游 id>`，别名冲突时回退为纯上游 id。其他模板保持既有同步行为，过滤后空集仍是致命错误。
- schema 5 门控在首次读取更旧文件时只绑定没有模板、名称 trim 后恰为 `OpenCode Zen Free` 且归一化 base URL 恰为 `https://opencode.ai/zen/v1` 的服务商，保留 key、映射、价格与所有其他服务商，且幂等。改写前 best-effort 创建一次原始字节副本 `ai_gateway.json.bak-pre-schema5`，Unix 权限 `0600`，以 `create_new` 创建因此绝不覆盖已有备份；复制失败不阻断迁移。
- 配置 schema 现为 9。schema 7 门控保留：当已持久化的内置模板状态的外层 `template_id` 与内嵌 `template.id` 均为 `opencode-zen-free`，且 `transport_strategy` 缺失、为 null 或 `direct` 时，仅把该字段改为 `opencode_zen_sidecar`。schema 8 门控只修复同一规范状态缺失、为 null 或空白的 `models_url`，插入 `https://opencode.ai/zen/v1/models`；非空 URL、模型、其他模板字段、同步元数据、服务商、key、映射与价格均保持不变。schema 9 门控只把同一规范模板快照以及任何绑定 `opencode-zen-free` 且 base URL 归一化后为遗留 `https://opencode.ai/zen/v1` 的服务商改为 `https://opencode.ai/inference/openai/v1`；自定义 base URL、模型、元数据、key、价格、传输、`models_url` 与无关模板及服务商均保持不变。所有修复都幂等且绝不触碰其他模板。门控逐级累积、绝不跳步：schema 9 及更新文件原样读取，schema 8 文件只补 base URL，schema 7 文件补 URL 与 base URL，schema 6 文件补传输、URL 与 base URL，schema 5 文件先移除废弃网关 proxy URL 再补三项。已绑定该模板的服务商因此在读取修复后的快照后解析为 sidecar 传输与官方推理 base，模板也变得可同步。
- 中继是从嵌套 crate `src-tauri/zen-relay` 构建的独立进程，该 crate 自带 workspace，并固定 `wreq = "=6.0.0-rc.31"`（启用 `socks` 与 `stream`，不带 `prefix-symbols`）。它是仅回环的 hyper 服务，从 stdin 首行读取每进程随机 token，输出 `READY <port>` 或 `ERROR <message>`，在临时端口绑定 `127.0.0.1`，请求体上限 64 MiB，只接受 `opencode.ai` 主机上路径恰为 `/inference/openai` 或遗留 `/zen` 或其 `/...` 子路径的 HTTPS 上游 base，其他主机、协议、路径、查询串与片段一律拒绝，拼接路径时不重复 `/v1`，过滤 hop-by-hop 与 sidecar 头以及 `accept-encoding`，原样流式中继响应，日志只含方法、路径与状态。其 wreq 客户端使用被接受的 BoringSSL 密码套件列表与 `X25519:P-256:P-384` 曲线配置。
- `src-tauri/src/ai_gateway/zen_sidecar.rs` 拥有该进程。它在首个 sidecar 尝试时惰性启动，就绪超时十秒，保持 stdin 打开用于生命周期与关闭检测，抽干 stderr，进程退出后重启。其身份为全局代理 generation、由快照派生的显式 proxy URL 与读取超时：新 generation 创建新子进程，已受理的普通与 SSE 响应持有 retiring 子进程的租约，最后一个响应体结束后子进程退出；等价保存或仅检查间隔变化保持 generation 并复用当前子进程。`stop` 幂等，两个网关关闭接缝都会调用，并终止全部 current 与 retiring 子进程。定位器依次检查 `ONESPACE_ZEN_RELAY_BIN`、可执行文件同目录、嵌套 debug 与 release 目标以及 `binaries/onespace-zen-relay-<triple>[.exe]`；缺失二进制时以预期路径使该次尝试失败，绝不回退直连。
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

- sidecar 生命周期与安全由网关运行时拥有：进程仅回环、以经 stdin 交付的每进程 token 鉴权、就绪等待有界、在退出或身份变化后重启、由最后一个响应租约维持 retiring 子进程，并由两个关闭接缝连同全部 retiring 子进程一起终止，因此停止网关或应用后不留下中继进程。
- 失败是显式的。缺失二进制、握手失败或中继不可达都会以带路径的错误使该次尝试失败，而不是静默改为直连重试，同时其他模型池候选与所有直连服务商照常工作；429 等上游状态原样中继，既有分类与重试因此仍然适用。
- 回滚是配置级的：解绑服务商或改绑直连模板即恢复既有直连行为。降级应用须先恢复 `ai_gateway.json.bak-pre-schema5`，因为早于 schema 5 的构建无法读取该绑定，旧构建不能被假定能保留或理解 schema 9，且修复前的中继会拒绝 `/inference/openai` base；若该副本创建失败，降级不受支持。
- 生产适配是版本门控的发布流程，而非手工改数据。已安装且早于免费过滤的构建绝不可手工补 `models_url`，因为其通用同步与自动刷新会导入共享上游列表中的全部模型（含付费）。修复前的构建（包括 schema 6 的 0.1.56）也绝不可手工改 base URL，因为其中继只接受 `/zen`，会拒绝 `/inference/openai`。受支持顺序是先备份 `~/.config/onespace/ai_gateway.json`，再安装并启动包含本修复的构建，使首次配置读取把 schema 6 迁移到 9，并更新已持久化 Zen 模板与绑定服务商的 base URL、传输与规范 `models_url`，随后打开 AI Gateway → Provider Templates → Zen Free → 同步模型列表，确认恰好 8 个免费模型，并确认本地 `GET /v1/models` 返回 8 个 `zen-free-*` id。升级后须验证 `zen-free-glm` 或 `free-model` 的网关推理，中继必须显示上游 `/inference/openai/v1`；独立中继与真实 OpenCode CLI 的冒烟已通过，但生产网关冒烟仍待执行，因此不宣称生产已成功。若新构建未安装，生产数据保持不变。
- 模型清单的抓取、同步与展示成功仍不代表推理成功。端点与传输决定可达性：遗留 `/zen/v1` base 即使凭据真实也返回 429，而经中继的官方 `/inference/openai/v1` base 在独立冒烟中返回了成功流。托管免费层的客户端准入与既有 429 分类及重试行为其余不变。
- 构建桌面应用除 Rust 工具链外现在还需要 cmake 与 ninja，且每个发布目标都会构建并打包该嵌套 crate。主应用仍不依赖 wreq，因此其构建与其他服务商保持不变。
- 运行期证据保持有界：中继、生命周期、迁移、模板与转发行为由单元与行为测试覆盖，包括推理 base URL 与中继允许清单；新的独立中继二进制加真实 OpenCode CLI 以 11 个工具经 `/inference/openai/v1` 完成一次请求并收到 200 `text/event-stream` 响应。生产网关冒烟仍待升级后执行，Windows 运行期冒烟未执行，Windows 证据仅有 CI 构建与跨平台 sidecar 测试。
- Supersession：[A Single Global Network Proxy Snapshot Drives Generation-Scoped Transport and Relay Leases](2026-10-29-shared-network-proxy-lifecycle.md) 部分取代本记录的中继身份、重启与关闭陈述。全局代理 generation 与由快照派生的显式 proxy URL 现在决定子进程身份，响应租约排空 retiring 子进程，等价保存复用当前子进程。BoringSSL 进程边界、模板绑定、备选、迁移备份与已知平台缺口继续有效；本记录保留并交叉链接。
