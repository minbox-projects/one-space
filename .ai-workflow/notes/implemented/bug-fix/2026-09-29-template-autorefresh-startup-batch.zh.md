# Agent Note: Template Auto Refresh Runs One Batch at App Start

Status: implemented

[English](2026-09-29-template-autorefresh-startup-batch.md) | 中文

## Problem

由 [Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md) 交付的调度器只在应用启动读取持久化间隔时才安装其计时器。使用默认 60 分钟间隔时，首次自动同步因此要在启动一整个间隔之后才到达，而计时器会跨系统休眠暂停，因此在一次改变生成的终端模型列表的应用升级之后，或长时间休眠之后，工具侧终端记录可能持续过期数小时，直到某个 tick 最终运行。观测到的事件：opencode 配置是在升级数小时后某个模板自动刷新 tick 最终运行时才重新生成。手动（非模板）编辑已由 `pending_sync` 模型选择漂移徽标暴露，但模板驱动的传播没有启动及时性保证。[Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) 后来把计划移入 Rust 进程，现在拥有该启动生命周期；本记录为启动批次决策而保留。

## Decision

`src-tauri/src/ai_gateway/auto_refresh.rs` 中由进程拥有的调度器安装恰好一个启动批次。`start_scheduler` 由 `src-tauri/src/app_runtime/run_app.rs` 在网关 autostart 后调用一次；安装时它读取持久化间隔，当归一化值为正整数时武装计划并随后生成恰好一个立即 `run_batch`。持久化的 `0`（以及非法或读取失败）安装停放的控制环而不运行批次，因此后续 `request_rearm` 可在不运行批次的情况下武装它。安装由进程级槽位守护，因此第二次 setup 调用为空操作，绝不运行第二个启动批次。间隔变化路径是分开的：持久化保存调用 `auto_refresh::request_rearm()`，它只替换截止时间、绝不立即运行批次。

立即运行就是普通批次：它列出合格模板，并通过共享的 `execute_template_sync` 路径串行同步每个 `models_url` 非空白的模板，跳过另一个同模板操作已在刷新的模板，把失败记录到进程内存而不弹 toast，成功同步仍携带 best-effort 终端刷新与 additions-only 的 `template_sync` info 消息。

前端不再拥有任何计时器：挂载在 `src/App.tsx` 的 `useTemplateAutoRefresh` 是只读/订阅适配器，执行一次状态读取与一次更新事件订阅。早先前端的 `batchInFlightRef` StrictMode 去重与 `applyInterval(value, runOnce)` helper 已不存在；单一进程安装取代了它们。

## Alternatives considered

- 也在每次间隔变化通知时立即运行批次：拒绝原因：设置保存是计划变化而非数据变化；它会在每次保存时拉取每个模板，而重新武装计划在无额外流量的前提下保持 tick 语义。
- 保持仅计划启动，并依赖 `pending_sync` 模型选择漂移徽标与手动同步覆盖过期窗口：拒绝原因：该徽标用于暴露手动（非模板）编辑，而模板驱动的变化按设计自动传播；该事件显示模板驱动过期持续数小时，因此计划本身需要启动批次。
- 把启动批次保留在前端挂载副作用中：拒绝原因：在 [Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) 之后进程调度器是唯一 owner；前端批次会重复计划、只在 WebView 挂载时运行，并重新引入进程安装已处理的 StrictMode 去重。

## Consequences

- 启动：有效持久化间隔安装计划并随后运行恰好一个立即批次；`0`、非法值与读取失败都不运行。进程级安装守卫意味着第二次 setup 调用绝不运行第二个批次。
- 间隔变化：设置保存经 `ai_gateway_template_auto_refresh_save` / `request_rearm()` 重新武装后端计划而不立即运行批次；下一次自动运行仍在整整一个间隔之后（当新值为 `0` 时不运行）。
- 及时性：应用升级或长时间休眠后，模板驱动的更新在首次应用启动时到达派生服务商与工具侧记录，而不是等待最多一个间隔；手动编辑仍由 `pending_sync` 徽标覆盖。
- 批次语义不变：URL 支持的模板串行同步，同模板在途工作经共享 guard 被跳过或复用，失败内联渲染而不弹 toast，成功同步保留 best-effort 终端刷新与 additions-only 通知。
- 验证：`src-tauri/src/ai_gateway/tests/auto_refresh.rs` 覆盖启动恰好一次（正间隔运行一个批次且第二次安装为空操作）、仅合格选择、串行顺序、tick 跳过与 re-arm 路径；`src/App.runtimeOwnership.test.tsx` 覆盖渲染器执行一次状态读取与一次订阅、启动时绝不列表或同步模板。
- 取代关系：部分取代。[Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md) 把本记录启动批次的前端挂载副作用实现替换为进程调度器，而本记录的启动批次、间隔变化不运行批次与及时性决策继续有效；本记录保留并交叉链接，不归档。本记录继续部分取代 [Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md) 的启动生命周期，其间隔契约、批次语义与失败显示决策继续有效。
- `MEMORY.md` 与 `.ai-workflow/index/navigation.json` 的 `ai-gateway` 与 `ai-gateway-backend` 条目在同一变更中记录进程启动批次规则，且 `navigation.md` 由权威 JSON 重新生成。
