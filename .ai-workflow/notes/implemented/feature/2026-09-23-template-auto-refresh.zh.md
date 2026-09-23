# Agent Note: Provider Templates Refresh Automatically on a Persisted Interval

Status: implemented

[English](2026-09-23-template-auto-refresh.md) | 中文

## Problem

带 `models_url` 的服务商模板此前只在操作者点击模板卡上的同步操作时才拉取模型清单。应用保持打开期间上游清单会漂移，每个派生服务商都会继续提供过期清单，直到有人想起来点击。因此自动刷新必须在应用运行期间按计划更新每个有 URL 的模板，但它绝不能成为第二份同步实现：用户禁用的映射必须保持禁用、`ignored_models` 条目绝不能被复活、价格行绝不能被写入。必须先确定两件事——计划运行在哪里（Rust 侧运行时循环，还是已经编排模板同步的前端），以及无人值守的失败如何展示而不打扰操作者——此外间隔本身必须持久化到 `api_gateway.json`、对既有文件默认 60 分钟且无需迁移、接受 `0` 表示禁用，并在前后端都保持校验。

## Decision

间隔持久化为 `GatewayConfig.template_auto_refresh_minutes: u32`（`src-tauri/src/api_gateway/types_config.rs`），带 `#[serde(default = "default_template_auto_refresh_minutes")]` 且始终序列化，因此旧 `api_gateway.json` 以 60 读取、无需迁移，而特性前的构建会忽略这个未知字段。公共常量 `DEFAULT_TEMPLATE_AUTO_REFRESH_MINUTES = 60`、`MIN_TEMPLATE_AUTO_REFRESH_MINUTES = 10` 与 `MAX_TEMPLATE_AUTO_REFRESH_MINUTES = 1440` 固定契约。读取时归一化（`normalize_template_auto_refresh_minutes`）保留 `0`（禁用）与 10–1440 的存储值，其余一律回退 60；保存时校验（`validate_template_auto_refresh_minutes`）只接受 `0` 或 10–1440，其余值在任何写入前以可操作错误拒绝。`src-tauri/src/api_gateway/commands.rs` 中的命令 `api_gateway_template_auto_refresh_get` 与 `api_gateway_template_auto_refresh_save(minutes: i64)` 只读取与替换该字段，注册于 `src-tauri/src/app_runtime/run_app.rs`，保存会返回写后重新读取的持久化值。

设置页 `ai-gateway` 分区（`src/components/SettingsView.tsx`）新增整分钟输入：`0` 禁用，否则 10–1440，并显示持久化值，因此没有该字段的配置显示 60。`parseTemplateAutoRefreshInput` 在调用任何命令前阻断空、非数字与越界输入并提示 `apiGatewayTemplateAutoRefreshInvalid`；后端拒绝被映射为同一本地化消息，而不是原始错误字符串。保存成功后视图重新读取 `apiGatewayTemplateAutoRefreshGet()` 并调用 `notifyTemplateAutoRefreshIntervalChanged()`。包装函数 `apiGatewayTemplateAutoRefreshGet` / `apiGatewayTemplateAutoRefreshSave` 与通知接缝 `notifyTemplateAutoRefreshIntervalChanged` / `subscribeTemplateAutoRefreshIntervalChanged` 位于 `src/lib/apiGateway.ts`。

调度器是 `src/components/ApiGateway/useTemplateAutoRefresh.ts` 中的 `useTemplateAutoRefresh()`，在 `src/App.tsx` 挂载一次，因此与当前挂载的视图无关。它在挂载时以及每次收到通知变化时读取持久化间隔，值不是正整数时停止计时器，否则以 `minutes * 60_000` 调度 `setInterval`。单一批次进行中守卫（`batchInFlightRef.current`）让批次进行期间到达的 tick 成为空操作。批次内先列出一次 `apiGatewayProviderTemplates()`，随后按顺序跳过 `models_url` 为空或全空白的模板，跳过通过 `setTemplateSyncInFlight` / `isTemplateSyncInFlight` / `clearTemplateSyncInFlight` 注册表标记为手动同步进行中的模板（`src/components/ApiGateway/index.tsx` 在自身同步开始与结束时写入该注册表），对其余模板调用未改动的 `apiGatewaySyncProviderTemplate(id)`。因此自动路径逐字复用人工拉取、替换与传播语义：每个模板在配置克隆上只做一次加密原子写入，仅在字段仍等于上一版模板值时增量更新，用户禁用的映射保持禁用，`ignored_models` 绝不被复活，绝不写入任何价格行，人工同步的致命集合（网络失败、超时、非 2xx、非 JSON、缺少模型数组、空有效模型集）让该模板及其派生服务商保持不变，而同一批次中的其他模板成功仍会落盘。

失败绝不弹出 toast。模板列表失败会静默放弃该批次；单个模板的失败经 `setTemplateAutoRefreshFailure(templateId, reason)` 记录在内存中，匹配的卡片通过 `apiGatewayTemplateAutoRefreshFailed` 内联渲染插值后的 `reason`，而 `synced_at` 保持不变，因为只有成功同步才会写入它。`useTemplateAutoRefreshFailures()` 驱动 `ProviderTemplateSection.tsx`，之后该模板的自动成功或人工同步成功都会清除原因，`clearTemplateAutoRefreshFailures()` 清除全部原因，且状态只存在于模块内存，应用重启后即为干净。`src/i18n.ts` 为中英文新增 `apiGatewayTemplateAutoRefreshLabel`、`apiGatewayTemplateAutoRefreshInvalid` 与 `apiGatewayTemplateAutoRefreshFailed`。

## Alternatives considered

- 在 Tauri 后端实现 Rust 侧进程内间隔调度器：未采纳，因为后端需要自己的运行时循环来启动、停止并按配置变化重新武装，而批次所需的编排（列出模板、排序批次、推迟手动同步）已经在命令层前端；把计划留在 `App.tsx`、把持久化留在后端能以同一条执行路径获得相同行为，且无需与应用启停协调第二套生命周期。
- 仅在 `api-gateway` 视图可见时挂载调度器：未采纳，因为操作者使用其他视图时刷新也必须继续；该 hook 在 `App.tsx` 挂载一次，其计时器不依赖当前挂载页面。
- 操作系统级调度（launchd 或其他系统计时器）：未采纳，因为它反正只在应用运行时触发，还会在应用的加锁配置存储与内存失败界面之外执行同步，为同样的写入增加第二条不可观测的执行路径。
- 按模板的开关或按模板的间隔：未采纳，因为刷新频率属于部署偏好而非模板属性；单一全局间隔保持单一计划、单一禁用值（`0`）与单一校验规则，而某个不应被拉取的模板可以让其 `models_url` 留空。
- 沿用人工同步错误通道的后台失败 toast：未采纳，因为无人值守批次会为每个失败模板各弹一个 toast，而操作者无事可做；卡片上的内联原因让失败紧邻其所属同步，无需弹窗也不持有持久化状态。

## Consequences

- 间隔契约：没有该字段的配置读取为 60 且无需迁移，读取不会改写文件；存储的 `0` 与 10–1440 原样报告，其他存储值归一化为 60；保存只接受 `0` 或 10–1440，`5`、`-1`、`1441` 与非整数会被拒绝并返回指明可接受值的错误且不写入任何内容；`0` 停止计划，删除该字段恢复 60 分钟的启用默认值而不是禁用。
- 自动刷新就是计时器下的人工同步：每个模板的拉取、替换与派生服务商传播都走未改动的 `api_gateway_sync_provider_template` / `apply_template_sync_with` 路径，因此用户禁用的映射绝不被重新启用，`ignored_models` 条目绝不被复活，绝不写入任何价格行，失败模板的存储条目与派生映射保持不变，而同一批次中的其他模板仍会应用；失败的刷新绝不触碰 `synced_at`。
- 计划生命周期：计时器在启动时由持久化间隔推导，并在设置保存通知接缝后立即重算；`0` 停止它；批次进行期间到达的 tick 被跳过；手动同步进行中的模板被推迟到之后的 tick；模板按列出顺序串行执行。
- 失败展示：自动失败只在内存中，并在匹配的模板卡上以 `Auto refresh failed: <reason>` / `自动刷新失败：<原因>` 内联渲染，绝不作为 toast；该模板的下一次成功（自动或人工）清除原因，应用重启后不显示陈旧原因。
- 验证：交付的行为测试为 `src-tauri/src/api_gateway/tests.rs`（默认与缺字段兼容、读取归一化、越界拒绝且不写入、边界持久化与命令注册）、`src/components/ApiGateway/useTemplateAutoRefresh.test.ts`（间隔后首批、空白 URL 跳过、`0` 从不运行、tick 跳过、失败隔离且无 toast、失败清除、手动同步推迟与变化后重排）、`src/components/ApiGateway/ProviderTemplateSection.test.tsx`（内联失败渲染、`synced_at` 保持与双语文案）、`src/components/SettingsView.test.tsx` 与 `src/lib/apiGateway.test.ts`（编辑器加载与默认、非法输入阻断、保存值、重读与通知，以及包装函数与通知接缝）；Step 4 的目标测试套件由编排器执行。
- `MEMORY.md` 与 `navigation.json` 的 `settings`、`api-gateway` 和 `api-gateway-backend` 条目在同一变更中记录间隔字段与命令、设置编辑器、调度器与失败展示，`navigation.md` 由权威 JSON 重新生成。
- Supersession（取代评估）：部分取代。本记录部分取代 [API Gateway Provider Templates and Incremental Model Sync](../architecture/2026-09-18-api-gateway-provider-templates.md)：交付其被否决的「后台自动或按计划同步」替代方案并修订其「同步为手动、按模板」的前提，该记录已就这一被否决的替代方案就地更正；其称谓、模板绑定、忽略集合生命周期、「仅当未被改动才更新」的增量合并与价格决策继续有效，并被自动路径原样复用。本记录也以调度、持久化与失败展示决策扩展 [Provider Templates Drop Built-in Model Catalogs and Prices](../architecture/2026-09-19-provider-template-manual-model-sync.md)；该记录的人工同步语义、[Gateway Template Model Retirement](../architecture/2026-09-20-gateway-template-model-retirement.md) 的退役规则与 [Gateway Synced Model Names Cover the Whole Identifier](../bug-fix/2026-09-20-gateway-synced-model-name-completion.md) 的命名规则继续有效，并被自动路径原样复用，所有记录保留并交叉链接；[Template Sync Best-Effort Refreshes Previously Synced Terminal Tools](2026-09-23-template-terminal-resync.md) 之后部分取代本记录「命令未改动」的前提，因为共享命令的每次成功同步现在还会 best-effort 刷新已同步终端工具，而本记录的间隔、调度器与失败展示决策继续有效。
