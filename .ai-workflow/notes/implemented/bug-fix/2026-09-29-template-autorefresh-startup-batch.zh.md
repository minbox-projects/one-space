# Agent Note: Template Auto Refresh Runs One Batch at App Start

Status: implemented

[English](2026-09-29-template-autorefresh-startup-batch.md) | 中文

## Problem

[Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md) 交付的调度器只在应用启动读取持久化间隔时安装计时器。默认 60 分钟间隔下，首个自动同步要等到启动满一个间隔之后，而计时器在系统休眠期间暂停，因此应用升级改变生成的终端模型清单（例如为 OpenCode Responses-only 模型新增的按模型 `provider.npm` 覆盖）之后，或长时间休眠之后，工具侧终端记录可能保持陈旧数小时，直到某个 tick 终于运行。实际观测到的事件：opencode 配置直到 08:26 才重新生成，那是一个模板自动刷新 tick 恰好运行的时刻，距 0.1.40 升级已过去数小时。手动（非模板）编辑已由 `pending_sync` 模型选择漂移徽标提示（[Gateway Terminal Sync Flags Model-Selection Drift as Pending](2026-09-29-gateway-terminal-pending-model-drift.md)），但模板驱动的传播没有任何启动及时性保证。

## Decision

`src/components/AiGateway/useTemplateAutoRefresh.ts` 的 `applyInterval(value, runOnce = false)` 现在在 `runOnce` 为真且归一化间隔为正整数时，先安装间隔计划、再立即运行恰好一个 `runBatch()`。挂载读取（`readAndApplyInterval(true)`）传入 `runOnce = true`；`subscribeTemplateAutoRefreshIntervalChanged` 通知只重读并重设计划，绝不启动立即批次。间隔 `0`、非法值与间隔读取失败仍然既不运行批次也不安装计划。既有 `batchInFlightRef` 守卫去重 React StrictMode 双挂载，因此立即批次恰好运行一次。

立即运行就是普通批次：列出模板，按顺序通过未改动的 `ai_gateway_sync_provider_template` 同步每个 `models_url` 非空白的模板，推迟手动同步进行中的模板，失败只记录在内存中、绝不弹 toast，成功同步仍携带 best-effort 终端刷新（[Template Sync Best-Effort Refreshes Previously Synced Terminal Tools](../feature/2026-09-23-template-terminal-resync.md)）与合格变化消息（[Automatic Template Sync Notifies the Message Center on Real Mapping Changes](../feature/2026-09-24-ai-gateway-template-auto-sync-notification.md)）。

## Alternatives considered

- 每次间隔变化通知也立即运行一个批次：未采纳，因为设置保存是计划变更而不是数据变更；那样会在每次保存时拉取全部模板，而重设计划在无额外流量的前提下保持 tick 语义。
- 保持只安装计划的启动行为，把陈旧窗口交给 `pending_sync` 模型选择漂移徽标与手动同步：未采纳，因为该徽标用于暴露手动（非模板）编辑，而模板驱动的变化按设计自动传播；事件表明模板驱动的陈旧持续了数小时，因此计划本身需要启动批次。

## Consequences

- 启动：有效的持久化间隔会安装计划，随后立即运行恰好一个批次；`0`、非法值与读取失败两者都不运行。StrictMode 双挂载仍只产生一个批次，因为进行中守卫让第二次调用成为空操作。
- 间隔变化：通知接缝的设置保存会重读持久化间隔并替换计时器，但不立即运行批次；下一次自动运行仍相距一个完整间隔（新值为 `0` 时不运行）。
- 及时性：应用升级或长时间休眠后，模板驱动的更新在首次应用启动就到达派生服务商与工具侧记录，而不必等到下一个间隔；手动编辑仍由 `pending_sync` 徽标覆盖。
- 批次语义不变：有 URL 的模板按顺序同步，手动同步进行中的模板被推迟，失败内联显示、绝不弹 toast，成功同步保留 best-effort 终端刷新与合格变化消息通知。
- 验证：`npx vitest run src/components/AiGateway/useTemplateAutoRefresh.test.ts` exit 0（28 通过）覆盖 `runsOneBatchImmediatelyAtMountThenOnlyOnTheIntervalAndSkipsBlankModelsUrl`、`anIntervalChangeNotificationNeverRunsAnImmediateBatch` 与重新基于挂载批次的调度器及通知 `AC-*` 测试；`npx vitest run src/components/AiGateway` exit 0（390 通过，16 个文件）；`npx eslint src/components/AiGateway/useTemplateAutoRefresh.ts src/components/AiGateway/useTemplateAutoRefresh.test.ts` exit 0；`npx tsc -b --pretty false` exit 0。
- Supersession（取代评估）：部分取代。本记录部分取代 [Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md) 的启动生命周期：挂载读取现在还会立即运行一个批次，而只有间隔变化通知保持「重读并重设计划、不运行批次」的行为；该记录的间隔契约、批次语义与失败展示决策继续有效并被原样复用，该记录已就修订后的启动生命周期就地更正。不取代任何其他记录。
- `MEMORY.md`、`docs/USAGE.md` 与 `.ai-workflow/index/navigation.json` 的 `ai-gateway` 条目在同一变更中记录启动批次规则，`navigation.md` 由权威 JSON 重新生成。
