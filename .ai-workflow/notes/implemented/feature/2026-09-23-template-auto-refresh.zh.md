# Agent Note: Provider Templates Refresh Automatically on a Persisted Interval

Status: implemented

[English](2026-09-23-template-auto-refresh.md) | 中文

## Problem

已配置 `models_url` 的服务商模板只在操作者点击模板卡上的同步操作时才拉取模型列表。应用持续打开时，上游目录发生漂移，每个派生服务商都继续提供过期列表，直到有人记得去点击。因此自动刷新必须在应用运行时按计划更新每个 URL 支持的模板，但它不能成为第二个同步实现：用户禁用的映射必须保持禁用，`ignored_models` 条目绝不能复活，价格行不得被创建或编辑。必须先确定两个决定——计划放在哪里，以及无人值守的刷新失败如何在不骚扰操作者的前提下呈现——并且间隔本身必须持久化、对既有文件默认 60 分钟、接受 `0` 作为禁用并在两端保持校验。本记录最初在前端交付该计划；[Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) 后来把所有权移到 Rust 进程，是当前生命周期的权威。

## Decision

间隔持久化为 `src-tauri/src/ai_gateway/types_config.rs` 中的 `GatewayConfig.template_auto_refresh_minutes: u32`，带 `#[serde(default = "default_template_auto_refresh_minutes")]` 且始终序列化，因此缺少该字段的文件读取为 60（更旧的文件由版本门控迁移一次性升级），功能引入前的构建忽略该未知字段。公共常量 `DEFAULT_TEMPLATE_AUTO_REFRESH_MINUTES = 60`、`MIN_TEMPLATE_AUTO_REFRESH_MINUTES = 10` 与 `MAX_TEMPLATE_AUTO_REFRESH_MINUTES = 1440` 固定契约。读取时归一化（`normalize_template_auto_refresh_minutes`）保持 `0` 与 10–1440 原样，其余回退 60；保存时校验（`validate_template_auto_refresh_minutes`）只接受 `0` 或 10–1440，其余在写入前以可操作错误拒绝。`src-tauri/src/ai_gateway/commands.rs` 中的命令 `ai_gateway_template_auto_refresh_get`、`ai_gateway_template_auto_refresh_save(minutes: i64)` 与 `ai_gateway_template_auto_refresh_status` 注册于 `src-tauri/src/app_runtime/run_app.rs`；save 返回写后重读的持久化值并调用 `auto_refresh::request_rearm()`，后者重新武装进程计划且不运行立即批次。

`src/components/SettingsView.tsx` 的设置 `ai-gateway` 分区提供整分钟输入：`0` 禁用，否则 10–1440，并显示持久化值，因此缺少该字段的配置显示 60。`parseTemplateAutoRefreshInput` 在调用任何命令前阻断空、非数字与越界输入并报告 `apiGatewayTemplateAutoRefreshInvalid`；后端拒绝被映射为同一本地化消息而非原始错误字符串。保存成功后视图重读 `aiGatewayTemplateAutoRefreshGet()` 并调用 `notifyTemplateAutoRefreshIntervalChanged()`。该前端通知接缝保留在 `src/lib/aiGateway.ts`，但进程调度器由后端命令重新武装，而不是由前端订阅。

计划由 `src-tauri/src/ai_gateway/auto_refresh.rs` 中的 Rust 进程拥有。`start_scheduler` 由 `src-tauri/src/app_runtime/run_app.rs` 在网关 autostart 后恰好安装一次；安装时读取持久化间隔，正值武装计划并运行恰好一个立即启动批次，`0` 安装停放的控制环，后续 `request_rearm` 可在不运行批次的情况下武装它。同一时刻只运行一个批次（批次期间到达的 tick 被跳过），按模板顺序串行迭代合格模板（`models_url` 经 trim 后非空），并调用与手动操作相同的 `execute_template_sync` 路径。自动批次与手动 `ai_gateway_sync_provider_template` 共享 `start_template_op` / `TemplateOpOwner` / `TemplateOpFollower` guard：自动竞争者在忙时跳过，手动竞争者等待并复用 owner 的在途结果而不重复拉取，且仅在条目仍指向完成中的操作时才清除。因此批次逐字复用人工同步的拉取、替换与传播语义：每模板一次加密原子写入、仅在字段仍等于上一版模板值时增量更新、用户禁用映射保持禁用、`ignored_models` 绝不复活、绝不创建或修改保留模型的价格行而会删除退役模型的价格行，手动同步的致命集（网络失败、超时、非 2xx、非 JSON、缺模型数组、空有效模型集）使该模板及其派生服务商保持不变，而同一批次中的兄弟模板成功仍然落盘。

失败绝不弹 toast。每模板失败只存进程内存：`set_template_failure` 在失败时记录原因，`clear_template_failure` 在成功时移除，`failure_snapshot` 按模板 id 顺序暴露该 map。它们经 `ai_gateway_template_auto_refresh_status() -> TemplateAutoRefreshStatus { failures: [{ template_id, reason }] }` 与 `AI_GATEWAY_TEMPLATE_AUTO_REFRESH_UPDATED_EVENT = "ai-gateway-template-auto-refresh-updated"` 交付给 UI，且仅在失败集合真正变化时发出；只有成功同步才写 `synced_at`，因此它保持不变，应用重启后从干净状态开始。`src/components/AiGateway/useTemplateAutoRefresh.ts` 现在是只读/订阅适配器：挂载时执行一次状态读取并订阅更新事件，每次替换整张 map，没有计时器、没有同步调用、没有 in-flight 注册表、没有消息构建器；`useTemplateAutoRefreshFailures()` 保持其 `Record<string, string>` 形状。`src/lib/aiGateway.ts` 新增 `aiGatewayTemplateAutoRefreshStatus`、`TemplateAutoRefreshFailure` / `TemplateAutoRefreshStatus` 类型与事件字面量。`src/i18n.ts` 保留中英文键 `apiGatewayTemplateAutoRefreshLabel`、`apiGatewayTemplateAutoRefreshInvalid` 与 `apiGatewayTemplateAutoRefreshFailed`。

## Alternatives considered

- Tauri 后端中的 Rust 侧进程内间隔调度器：当时被拒绝，因为后端需要自己的运行时循环来启动、停止并在配置变化时重新武装，而批次所需的编排已位于前端命令层。[Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) 后来采纳该选项并接管所有权，因此该替代方案不再被拒绝。
- 仅在 `api-gateway` 视图可见时挂载的调度器：拒绝原因：操作者在其他视图工作时刷新必须继续；计划由进程拥有，独立于所挂载页面。
- OS 级调度（launchd 或其他系统计时器）：拒绝原因：它反正只在应用运行时触发，并会在应用的锁定配置存储与内存失败面之外执行同步，为同样的写入增加第二条不可观测的执行路径。
- 每模板开关或每模板间隔：拒绝原因：刷新频率是部署偏好而非模板属性；一个全局间隔保持单一计划、单一禁用值（`0`）与单一校验规则，而不应被拉取的模板可以让其 `models_url` 留空。
- 与手动同步错误通道一致的后台失败 toast：拒绝原因：无人值守批次会为每个失败模板弹出一个 toast，而操作者无可执行动作；卡片上的内联原因让失败紧邻其所属同步，无需弹窗且不持有持久状态。

## Consequences

- 间隔契约：缺少该字段的配置读取为 60，当前版本读取不重写文件，更旧的文件由版本门控迁移一次性升级；存储的 `0` 与 10–1440 原样报告，其余存储值归一化为 60；保存只接受 `0` 或 10–1440，以命名可接受值的错误拒绝 `5`、`-1`、`1441` 与非整数且不写入；`0` 禁用计划，删除该字段恢复 60 分钟启用默认值而非禁用。
- 自动刷新是计时器下的手动同步：每次模板拉取、替换与派生服务商传播都经共享的 `execute_template_sync` / `apply_template_sync_with` 路径，因此用户禁用映射绝不重新启用、`ignored_models` 条目绝不复活、绝不创建或修改保留模型的价格行而会删除退役模型的价格行；失败模板保持其存储条目与派生映射不变，而同一批次中的兄弟模板仍然应用；失败刷新绝不动 `synced_at`。
- 计划生命周期由进程拥有：正持久化间隔的安装在启动时运行恰好一个立即批次；设置保存只重新武装而不立即运行批次；`0` 停放控制环；进行中批次期间到达的 tick 被跳过；同模板自动竞争者在手动操作在途时被跳过，而手动竞争者复用该在途结果；模板按列出顺序串行运行；隐藏窗口不停止计划。
- 失败显示：自动失败仅存内存，并以 `Auto refresh failed: <reason>` / `自动刷新失败：<原因>` 内联渲染在匹配模板卡上，绝不作为 toast；后端在该模板下一次成功时清除其原因，应用重启后不显示过期原因。
- 监听器生命周期与关闭：`RunningServer` 携带单调 `generation`，因此出错的监听器发布 `running=false` 并可在同一端口重启，而旧的退出任务绝不清除替换后的监听器；`shutdown_runtime_services` 幂等地停止网关监听器与模板计划。
- 验证：交付的行为测试为 `src-tauri/src/ai_gateway/tests/auto_refresh.rs`（启动恰好一次、仅合格模板、串行、tick 跳过、自动忙碌跳过并手动 follower 复用、失败快照事件、reset/re-arm）、`src-tauri/src/ai_gateway/tests/runtime_lifecycle.rs`（故障/重启、旧任务不能清理替换后的监听器）、更广的 `src-tauri/src/ai_gateway/tests.rs` 间隔与命令覆盖、`src/components/AiGateway/useTemplateAutoRefresh.test.ts` 的适配器、`src/components/AiGateway/ProviderTemplateSection.test.tsx` 的内联失败渲染、`src/App.runtimeOwnership.test.tsx`（一次读取与一次订阅、启动不列表/同步）以及 `src/lib/aiGateway.test.ts` / `src/components/SettingsView.test.tsx` 的封装、事件与编辑器。
- 取代关系：部分取代。[Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) 替换本记录的前端调度器所有权、前端失败存储及其被拒绝的 Rust 侧替代方案，而本记录的间隔契约、设置编辑器、批次语义、additions-only 消息决策与失败显示继续有效；本记录保留并交叉链接，不归档。本记录也如原记录那样部分取代 [API Gateway Provider Templates and Incremental Model Sync](../architecture/2026-09-18-api-gateway-provider-templates.md) 并扩展 [Provider Templates Drop Built-in Model Catalogs and Prices](../architecture/2026-09-19-provider-template-manual-model-sync.md)；其中引用的退役与终端重同步记录继续有效。
- `MEMORY.md` 与 `navigation.json` 的 `ai-gateway` 与 `ai-gateway-backend` 条目在同一变更中记录进程所有权、状态命令与事件以及适配器，且 `navigation.md` 由权威 JSON 重新生成。
