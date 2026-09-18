# Project memory

OneSpace 是面向开发者的 macOS 桌面工作台（Tauri 2 + React 19 + TypeScript），把 AI CLI 环境、原生终端会话、MCP、Skills/Subagents、工作流与常用生产力工具收拢到一个窗口。本文件是原生 agent 共享的架构与约定基线；`.ai-workflow/index/navigation.json` 是权威导航索引，`navigation.md` 由其生成。

## 技术栈与构建

- 前端：React 19、TypeScript、Vite 7、Tailwind CSS 3、Radix UI、i18next（中/英）。
- 后端：Rust、Tauri 2（`src-tauri/`），插件含 dialog / shell / process / updater / global-shortcut。
- 前端命令封装集中在 `src/lib/*`，通过 `@tauri-apps/api` 的 `invoke` 调用后端；UI 组件不直接拼装命令字符串。
- 构建命令：`npm run dev`、`npm run build`（`tsc -b && vite build`）、`npm run tauri dev`、`npm run tauri build`。
- 检查命令：`npm run lint`（eslint）、`npm test`（vitest run）、`npm run check:cli-matrix`；后端测试在 `src-tauri/` 下用 `cargo test`。
- 后端测试构建采用 `[profile.test]`：`opt-level = 1`、`debug = 0`、`split-debuginfo = "off"`，`[profile.test.package.sha2] opt-level = 2`；优先改善运行时间，不承诺增量重编提速。保持生产密码学参数（PBKDF2 100,000 次迭代，对应 `src-tauri/src/crypto.rs` 的 `PBKDF2_ITERATIONS`）、`[lib] crate-type` 和 vendored OpenSSL / bundled SQLite 不变；macOS 系统库候选暖重编收益 0.80%，未达 15% 采用门槛。
- 后端测试隔离：仅依赖 `get_app_dir()` 的测试可使用 `cfg(test)` 线程局部应用目录 guard；直接修改 HOME 或使用全局 server 状态的测试保留串行，不能假定线程局部覆盖会跨线程传播。内容哈希与 mtime 无关的测试用 `File::set_modified` 显式改变 mtime；重试/退避测试使用 paused 时间与到达间隔断言，真实 socket header / OS connect 行为保留真实时间。
- 测试计时分别记录 harness 与原始 wall 时间，争用扣除估算不能当作实测 wall；stable 不支持 `--report-time` 时使用已授权的等价计时。当前 581 例为 579 通过、两个既有 ignored（新增一例并发隔离回归、按用户后续明确范围修订删除一例冗余 E2E，删除不作为原指标达标手段）；原 AC-003 单例 <100ms（实测 0.18–0.37s）、AC-004 增量改善 ≥10%（5.94→6.54s，慢 0.60s）未达标，用户最终“同意”明确接受两项偏差并授权修复全部文档后合并。一次双轴审查已完成，不再审查；当前实施完成待集成，不表示已合并。完整授权、命令级实测与保留覆盖见 [Cargo test speedup](.ai-workflow/notes/implemented/testing/2026-09-18-cargo-test-speedup.md)。
- 版本号三处保持一致：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`。

## 模块根与职责

导航索引登记四个模块根，feature 的 `owner_role` 必须与所属模块根一致：

- `src`（owner `frontend`）：React 界面、状态、Tauri 命令封装与 i18n。
- `src-tauri/src`（owner `backend`）：Rust 命令、存储引擎、运行时与外部集成。
- `docs`（owner `documentation-maintainer`）：用户手册、CLI/MCP/Skills 文档与设计报告。
- `tools`（owner `backend`）：本地校验脚本。

## 前端架构（`src/`）

- `src/App.tsx` 是外壳与总控：侧边栏、页面切换、全局状态与快捷键；`src/lib/navigation.ts` 负责旧标签到新导航目标的解析（`resolveNavigationTarget`）。
- 每个业务域是一个 `src/components/<Domain>/` 目录或同名组件；每个组件目录通常包含 `index.tsx`、子组件、`*.test.tsx`，复杂域再拆分 `components/`、`hooks/`、`helpers/`、`types.ts`（参见 `Workspaces/`）。
- 命令封装与领域类型放在 `src/lib/`，按域一文件（如 `workflows.ts`、`skills.ts`、`subagents.ts`、`sshTunnels.ts`、`fileSharing.ts`、`shortLink.ts`、`aiAssistant.ts`、`apiFusion.ts`）。
- 文案统一走 `src/i18n.ts`，新增界面文本必须同时提供中英文；`en_keys.txt` / `zh_keys.txt` 为键清单。
- 共享基础组件在 `src/components/ui/`，Provider（主题、Toast、确认框、错误边界）在 `src/components/` 顶层。

## 后端架构（`src-tauri/src/`）

- `lib.rs` 声明模块并由 `app_runtime::run` 启动；`app_runtime/` 负责窗口、托盘、全局快捷键、CLI 入口与 OAuth。
- 每个业务域一个根文件加同名子目录（如 `ai_sessions.rs` + `ai_sessions/`、`skills.rs` + `skills/`、`protocol_router.rs` + `protocol_router/`）。子目录按 `commands`、`types`、`runtime`、`tests` 等拆分。
- `app_store/` 是统一存储与迁移核心：`storage_engine.rs`、`migration.rs`、`provider_projection/`、`sync.rs`、`types/`；会话、provider 与 launcher 命令都在此汇聚。
- 配置与密钥：`config.rs`、`runtime_profiles.rs`、`claude_profiles.rs`、`secrets.rs`、`crypto.rs`。
- CLI 探测与版本：`cli_probe.rs`、`cli_updates.rs`、`version_detect.rs`。

## Skills 统一目录与兼容

- `~/.agents/skills` 是所有 Skills 安装、扫描、同步与显示的规范目录；迁移后不再按工具维护独立 Skills 目录。
- `~/.claude/skills` 是指向 `~/.agents/skills` 的兼容符号链接；若该路径被普通文件/目录或错误、损坏的符号链接占用，则保持原样并返回可操作的失败，绝不覆盖。
- 同名冲突以统一目录版本为准；工具特定版本备份到 `~/.agents/skills/.backups/<tool>/<skill>/<content-hash>/`，按来源工具、Skill 名与内容哈希做幂等键，重复初始化不产生重复备份。
- 兼容性矩阵由后端记录 Claude / OpenCode / Codex / Antigravity 对 `~/.agents/skills` 的读取行为；无法直接读取的工具显式标记为依赖兼容路径或不受支持。

## API 网关模块与边界

- API 网关是独立模块：前端域 `src/components/ApiFusion/` 加命令封装 `src/lib/apiFusion.ts`，后端 `src-tauri/src/api_fusion.rs` 加同名子目录（`types_config`、`storage`、`selection`、`runtime_http`、`forwarding`、`commands`、`usage_log`）。
- 导航 id 固定为 `api-fusion`（feature 归属模块根 `frontend`，owner `frontend`）与 `api-fusion-backend`（模块根 `tauri-backend`，owner `backend`）。`api-fusion` 是左侧「AI 能力」分组的顶层页签，位于 `ai-environments` 与 `ai-usage` 之间；`resolveNavigationTarget("api-fusion")` 解析为顶层 tab。API 网关不作为工具存在：已从 `MoreToolsSection`、`moreToolPresentation`、`launcherToolVisibility`、`MoreToolsHub.tsx` 卡片与 `Launcher.tsx` 内部工具清单移除，仅保留在 `Launcher` 的导航目标中。新增工具 id 必须同时接入 `navigation.ts`、`moreToolPresentation.ts`、`launcherToolVisibility.ts`、`MoreToolsHub.tsx`、`Launcher.tsx` 与 `App.tsx`，否则页签不可达或启动器清单不一致。
- 上游服务商保留 `protocol` 字段（`chat_completions` 默认 / `responses`）作为继承来源；每条模型映射可另行声明 `protocol`，缺省或 `null` 即继承所属服务商协议；映射还可选填 `display_name`（`Option<String>`，缺省省略序列化字段），作为网关为该本地模型展示的名称，未填写时回退到远端模型名；每条映射新增持久化 `enabled`（`#[serde(default = "default_true")]`，字段缺省（不出现）视为启用、始终序列化，旧 `api_fusion.json` 无需迁移），禁用映射不参与候选、不成为命中、其 `upstream_model` 绝不参与转发，请求只命中某服务商的禁用映射时该服务商不得回退 `default_model`（等价于未命中），完全未命中任何映射仍按既有规则在协议一致时回退 `default_model`；映射级 `enabled` 与服务商级 `enabled` / `auto_disabled` 相互独立，手动禁用/重新启用服务商（`set_user_enabled` / `manual_reenable`）绝不修改任何映射的启用状态，被用户禁用的映射只能由对该映射的显式操作恢复。终端同步写入的模型清单只取服务商 `enabled && !auto_disabled` 且映射 `enabled` 的行，`opencode` 以映射的 `local_model` 为键写入 `tool_config.models`（值中的名称取 `display_name`，缺省回退远端模型名），`codex` 的 `model` 取首个非空启用映射 `local_model`，没有任何启用映射时才回退首个非空 `default_model`，仍为空则省略该键。候选选择依据映射行协议（缺省继承服务商协议）能否服务入站协议，不再用服务商 `protocol` 字段硬过滤；命中映射但协议不一致的服务商不参与本次请求且不回退 `default_model`，未命中映射才回退 `default_model` 并要求服务商协议一致。`/v1/models` 与前端聚合视图（`aggregateModels` / `AggregatedModelsDialog`）同样只包含服务商 `enabled && !auto_disabled` 且映射 `enabled` 的本地模型；前端 `ProviderDetailDialog` 为每个映射行提供启用开关，禁用行以 `data-disabled="true"` 与弱化样式直接可见，`resolveMappingPreview` 在只命中禁用映射时返回 `null`。中继不做请求体转换，调用方协议需与服务商或映射行协议一致；`/v1/messages`（Anthropic）不受支持属已知边界。
- 中继接受 `/chat/completions`、`/responses` 及其无 `/v1` 形式并统一成 `/v1/...` 上游路径，拼接上游 URL 时折叠重复的 `/v1` 版本段，因此 `base_url` 带不带 `/v1` 均可。转发按拒绝列表透传客户端入站请求头（如 `x-opencode-session`），hop-by-hop 与传输头以及 `content-type`、`accept` 丢弃；本地中继凭据（本地 Key 的 `authorization` / `x-api-key`）一律不得透传给上游，上游凭据由中继显式设置为 `authorization: Bearer <provider api_key>`。
- 新建本地 Key 只需名称，值由后端用 OS 熵随机生成（`sk-fusion-<128bit hex>`）；编辑既有 Key 时留空或回传脱敏占位符保留原值。
- 本地服务固定监听 `127.0.0.1` 加配置端口（默认 `17688`），bind 失败即返回包含端口与原因的可操作错误，不回退到其他端口；启用状态持久化，重启后按上次状态自动恢复监听。对外展示、复制与终端同步使用的本地 Api 地址统一为带 `/v1` 后缀的 `http://127.0.0.1:<端口>/v1`（`FusionStatus.local_base_url` 与前端 `localBaseUrl` 同源），中继本身仍同时兼容无 `/v1` 的入站路径。
- 上游服务商、本地 Key 与终端同步台账保存在独立加密文件 `api_fusion.json`（经 `crate::crypto` 加密并临时文件加 rename 原子写入），与 Protocol Router、AI Environments 的存储互不共享，密钥不得以明文落盘。
- 请求日志与用量另存于 `get_app_dir()` 下独立、不加密的 SQLite 文件 `api_fusion_usage.db`（`api_fusion/usage_log.rs`，`UsageLogStore`，表 `usage_logs`）；只记录 UTC+8 时间戳、本地/上游模型名、服务商 id 与名称、结果 `success|failure|cancelled`、HTTP 状态、输入/缓存读/缓存写/输出四档 token、总 token、金额与耗时，绝不包含请求/响应正文、请求头或任何凭据。仅通过本地鉴权并进入 `/chat/completions` 或 `/responses` 的请求会写入；本地 401、`GET /v1/models` 与未知路径/方法不记录。非流式从缓冲响应体解析 usage，流式由 `SseUsageAccumulator` 从 SSE `usage` 分片只读累积且不改写转发字节；缺失字段记 0，无 usage 仍记录，只有写入失败被记录并吞掉，绝不影响响应。
- 金额在写入记录时按本次实际转发的服务商与上游模型名优先精确匹配用户自维护价格表（`compute_cost_at_time` / `match_price_for_provider`，输入/缓存读/缓存写/输出四档单价，美元/百万 tokens，支持可选配置 UTC+8 峰谷时段 `off_peak`，未命中或无服务商时回退模型名精确匹配，无峰谷时段按标准价格计费）并固化进该行，之后修改价格永不重算历史；未定价模型存 `None`、界面显示 `—`、不计入合计，界面另显示未定价请求数。价格维护入口只位于「用量统计」页签内的 `ModelPriceDialog`，已维护价格按服务商分组显示，模型直接从各服务商配置的可用模型列表选择，支持内联展开配置峰谷时段与四档优惠单价，绝不出现在服务商表单或设置页。
- `FusionConfig` 新增 `usage_retention_days`（默认 90，范围 1–365，`#[serde(default)]`）与 `model_prices`（`#[serde(default)]`），旧 `api_fusion.json` 无需迁移仍可反序列化；价格与保留天数保存先读取现有配置、只替换目标字段再写回，绝不清空服务商、本地 Key 或 `terminal_syncs`，非法保留天数返回可操作错误且不落盘。保留天数在设置页独立分区 `ai-gateway` 配置，每次写入新日志时永久删除超期记录。六个命令 `api_fusion_usage_stats`、`api_fusion_request_logs`、`api_fusion_model_prices_get`、`api_fusion_model_prices_save`、`api_fusion_usage_retention_get`、`api_fusion_usage_retention_save` 提供查询与配置，聚合、分组、筛选与分页（每页固定 50）均在后端按 UTC+8 完成。`api_fusion_request_logs` 的不分组规范请求值为 `group_by: "none"`（前端即如此发送），`null`/缺省/空串兼容为不分组别名，分组值为 `"model"` 与 `"day"`，其他值返回可操作错误；不分组响应的 `group_by` 为 `null`，并返回覆盖已解析范围、独立于分页与模型筛选的去重非空 `local_model` 有界 `models` 列表以驱动前端模型筛选；只有 2xx 上游响应计入 tokens 与花费，非 2xx 记 `failure` 且四档 token 全为 0，`unpriced_count`（与 `—` 显示）只统计已到达上游模型但没有匹配价格行的请求，取消/无上游失败按 0 成本且不计入未定价。
- 终端写入边界：仅在用户主动「添加服务商 / 同步」时写入。同步不改写既有 `opencode`/`codex` 服务商记录，而是为每个受支持工具（`opencode`、`codex`）创建或更新一条名为 `API Gateway` 的独立网关服务商记录，携带本地 Api 地址、解析后的默认本地 Key 值（存储位指向禁用 Key 或缺省时按列表顺序兜底，与前端 `resolveDefaultKeyId` 一致）与网关的模型映射列表，并写入 `api_fusion_gateway` 标记（顶层或 `tool_config` 内均可）；`opencode` 网关记录统一使用 `provider_key = "apigateway"`。同步成功后自动激活 `opencode` 下的网关服务商并投影写入 `~/.config/opencode/opencode.json`（含 `options.apiKey`），使网关条目与 Key 真正落盘（`codex` 保持手动激活与手动投影）。再次同步优先复用 `terminal_syncs` 台账的 provider id，但仅当该 id 指向同一工具且带网关标记的服务商时才复用；指向未标记的用户记录视为过期台账，改为新建带全新 UUID 的独立网关记录，绝不改写用户记录；台账不可用时再取该工具已带标记的网关记录，都没有才新建 id。`api_fusion_terminal_targets` 对每个受支持工具各返回一个目标并套用同一标记规则（未标记的服务商不算已同步），`api_fusion_configure_terminal` / `api_fusion_sync_terminal` 以 `target_tools`（前端封装键 `targetTools`）传入工具名；工具名大小写不敏感校验，统一以小写 `opencode` / `codex` 存储。不改写 Protocol Router 的 route 数据，不触碰 `claude`/`antigravity` 记录。终端同步按钮按工具隔离进行中状态：仅当前操作的按钮禁用并转圈，其他工具按钮仍可点击。
- 失败分类集中在 `selection::classify_failure`，HTTP 状态语义优先于错误响应体是否为 JSON：401/403 立即禁用并切换；404 仅跳过该服务商，本请求不再尝试它，继续其他候选；408/429/5xx 可重试；其余 4xx（含 400/413/422）作为 `FailureClass::ReturnToClient` 回传调用方——正文是含 `error` 对象的合法 JSON 时按字节原样透传，否则保留上游状态并包装成标准错误信封（message 含上游状态与可读正文，绝不含本地/上游凭据或请求头），非流式与流式分支规则一致。网络错误、非 JSON 成功响应、无效 SSE、空流和输出前读取失败可重试；未被重定向处理的 3xx 沿用既有成功/响应格式规则。
- 网关自身生成的错误（请求解析失败、配置读取失败、未知路径、未授权、`/v1/models` 或其他转发路径上的方法错误、请求体非法、无候选）统一使用 `{"error":{"message","type","code","param":null}}` 信封（`error_envelope`；message、type、code 均非空，`param` 恒为 `null`），无候选取 `code: all_providers_unavailable`。
- `runtime_http::attempt_non_streaming` / `runtime_http::attempt_streaming` 采用 fallback-first：按本请求随机候选顺序先完成不等待退避的初轮，成功立即返回；随后按各服务商最早冷却截止时间串行重试，同一截止时间保持初轮顺序，每请求最多一个上游进行中。每个可重试服务商首次请求加最多 5 次重试（合计最多 6 次）；仅当可服务候选数大于 1 时才把可重试失败加入重试队列，恰好一个可服务候选时初轮仅尝试一次即立即以标准错误终止——不排队、不应用 `Retry-After` 等待、不消耗 120 秒预算，非流式与流式一致。多候选耗尽后沿用 `all_providers_unavailable` 格式：非流式与任何在写出首字节前确定的流式失败都返回 502 + `application/json` 标准信封，流式不再返回 200 SSE 错误事件加 `[DONE]`。请求日志语义不变：无候选仍记 502，流式耗尽仍记最后一个可确定的上游状态（无则 0），绝不改写为固定 502。
- 重试延迟由 `selection::retry_header_delay` / `selection::default_retry_delay` 决定：只读每个响应头的第一个值，有效 `retry-after-ms` 优先，其次 `Retry-After` 的非负有限秒数或未来 HTTP 日期，无效高优先级值继续向下回退；零表示立即，负数、非有限值、非法或过期日期无效。默认第 n 次重试等待 `min(2000ms * 2^(n-1) * (1 + random[0,1] * 0.25), 30000ms)`，即 2 秒起倍增、0–25% 正抖动、30 秒封顶；有效头指定等待不加抖动、不受 30 秒限制。
- 每次暂时失败完成后设置该服务商在本请求内的单调时钟冷却截止时间；长冷却不阻塞更早就绪候选。单请求仅累计实际退避等待，预算 120 秒，下一等待超过余额即终止，恰好等于余额允许；上游网络耗时不计入此预算，仍保留 10 秒连接与 60 秒空闲读取超时。冷却和预算不持久化、不跨请求共享；配置 schema 与公开 API 不变。
- `runtime_http::RequestHealth` 按入站请求汇总各服务商最终结果，在请求正常结束时各应用一次：A 失败而 B 成功仍给 A 记一次，同一服务商重试恢复则清零；仅 429/404 不累计健康失败，期间另有网络/408/5xx 等健康失败则记一次；401/403 即时禁用且结算不重复计数。连续 3 个失败请求自动禁用。流式仅在上游正常读完后成功清零；写出首字节后上游读取失败时，先补全 SSE 事件边界（最后一个转发字节非换行时补一个换行），再追加一个独立 `data: {"error":{...}}` 分片（type `server_error`、code `upstream_stream_error`）并关闭，绝不发送 `[DONE]`、绝不重试或切换候选，随后记一次失败（状态 502）并保留已累积 usage；下游取消不额外计为上游故障。`enabled` 表示用户意图、`auto_disabled` 表示运行状态，二者独立持久化，手动重新启用只清理运行状态。
- `runtime_http::handle_connection` 在转发期间检测下游连接关闭或读取错误，取消未完成上游请求、退避等待及后续尝试；下游写错误亦停止。开始向下游写出响应头/体后禁止切换或透明重放；取消支持普通完整 HTTP 客户端断开，不增加半关闭协议支持。
- 上述架构决策的由来见本地 ADR 历史（`.ai-workflow/adr/`）；ADR 已停止新增，不再由 `ai-workflow` 子命令维护或读取，不要扫描目录或维护独立索引。

## 数据与存储不变量

- local-first：运行时以本地镜像为主，再按配置同步到 `local / iCloud / Git`。
- 主密码用于加密敏感数据（`secrets.rs` / `crypto.rs`），密钥不得写入日志或提交到仓库。
- 设置页按分区独立保存与重置；每个分区应保持幂等。
- 会话历史、用量与 CLI 配置文件由后端解析后回填前端，前端不直接读取这些文件。

## 代码约定

- 只做必要改动，匹配现有风格；不引入未使用的抽象或依赖。
- 新增库前先确认 `package.json` / `Cargo.toml` 已存在该依赖。
- 前端行为测试使用 vitest + @testing-library，测试文件与被测文件同目录；后端测试位于对应域的 `tests.rs` 或 `tests/` 子目录。
- 保持 `navigation.json` 与 `navigation.md` 同步，并确保 `ai-workflow context validate` 通过。

## 工作流约束

- Planning 逐条澄清业务影响问题并冻结 `spec.md` / `plan.md`；Plan-to-tasks 生成不可变任务文件。
- Coding 以 TDD 在项目内临时 worktree 实现单个已批准任务，完成后依次通过 Spec Review 与 Standards Review 才可合并。
- 架构、归属、公共符号、路径或工作流规则变化时，必须同步更新 `MEMORY.md` 与 `navigation.json` 并重新生成、校验 `navigation.md`。
- 架构、模块边界与归属、公共协议或 schema、跨领域标准、工作流或 agent 规则、难以回退的技术选型发生变更时，不再新增 ADR；决策原因与生命周期记录到 Agent Notes（`.ai-workflow/notes/`，以 `.ai-workflow/notes/README.md` 为格式与治理唯一来源）。每条 Note 均为英文正文加中文正文加 `.i18n.yaml` 一致性记录的三件套等权同体；仅自然语言正文做翻译，结构元素（`# Agent Note:` 前缀、标题、表格表头、`Status` 及其取值、字段名、路径、日期）保持英文，配对经 `ai-workflow notes pairing --write` 记录；`output_language` 只约束规划产物与会话表述，不约束 Note 语言。本文件记录当前标准（怎么做），与 Notes 不一致即为缺陷，须在同一变更内一起更新。
