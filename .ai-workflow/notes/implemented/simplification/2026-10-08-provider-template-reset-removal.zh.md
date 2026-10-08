# Agent Note: AI Gateway Provider Template Reset Is Removed Full-Stack

Status: implemented

[English](2026-10-08-provider-template-reset-removal.md) | 中文

## Problem

AI 网关服务商模板的「恢复内置预设」能力已被移除以精简功能面。该操作允许操作者清除全部 `deleted_template_ids` 墓碑并一次性恢复所有内置模板，但其余模板操作都不依赖它，而它唯一的用途——撤销内置模板删除——更适合让删除保持最终。该能力已被端到端移除：两个 UI 工具栏入口、前端命令封装、Tauri 命令、后端实现、`lib.rs` 导出、`generate_handler!` 注册、相关测试以及两个双语 i18n 键。

## Decision

本次为全栈移除，不保留功能开关或回退路径。前端方面，`ProviderTemplateSection` 移除了 `onResetBuiltin` prop 与恢复按钮，`src/components/AiGateway/index.tsx` 移除了对话框工具栏的恢复按钮与 `handleResetBuiltinTemplates` 处理函数；一并删除的还有不再使用的 `RotateCcw` 导入、`template-section-reset-btn` testid 以及 `src/lib/aiGateway.ts` 中的 `aiGatewayResetProviderTemplates` 封装。后端方面，命令模块移除了 `ai_gateway_reset_provider_templates`，`src-tauri/src/ai_gateway/templates.rs` 移除了 `apply_reset_provider_templates` 实现，该命令也从 `lib.rs` 导出与 `src-tauri/src/app_runtime/run_app.rs` 的 `generate_handler!` 注册中删除。测试已更新以固定其缺失：`AiGateway.test.tsx` 现在断言 `template-section-reset-btn` 不存在、新建模板控件仍在；`src-tauri/src/ai_gateway/tests.rs` 断言该命令既不在注册器源码中也不在 `lib.rs` 导出源码中；`test_template_delete_succeeds_when_unused_and_reset_restores` 的 reset 部分被删除，仅保留 `test_template_delete_succeeds_when_unused` 及其墓碑与隐藏视图断言。双语键 `aiGatewayTemplateResetBuiltin` 与 `aiGatewayTemplateResetSuccess` 已从 `src/i18n.ts` 的中英文两侧删除。

保留的 `deleted_template_ids` 墓碑语义不变。删除内置模板仍会把其 id 恰好一次写入 `deleted_template_ids` 并从 `provider_template_views` 中隐藏，`effective_template` 对已删除 id 仍返回失败；被移除的 reset 命令是唯一为恢复已删除内置模板而清除墓碑的路径，因此本次移除后不再有任何 reset 路径清除它们。`apply_upsert_provider_template` 中按 id 的 `retain` 只移除正被显式重新保存的模板 id，与被移除的能力无关、不在范围内且保持不变。

## Alternatives considered

- 仅移除 UI 入口而保留 `ai_gateway_reset_provider_templates` 与 `apply_reset_provider_templates`：未采纳，因为这会留下不可达却仍可调用的命令，它仍会清除墓碑，于是陈旧前端构建可复活已删除的内置模板，且该命令会在没有任何剩余调用方的情况下继续被导出与注册。
- 在一次迁移中清除 `deleted_template_ids`：未采纳，因为这会通过复活操作者已刻意删除的模板而重写持久化的用户意图，而本次移除不涉及迁移或任何持久格式变更。
- 把命令保留在功能开关之后：未采纳，因为该能力已无任何剩余消费者，开关只会让 reset 路径、其注册及其配置面继续存活，而并没有被要求的回滚或兼容性需求。

## Consequences

- 不再有任何恢复入口：服务商模板管理对话框工具栏只渲染展开/折叠与新建模板控件，内嵌区块不渲染任何恢复操作，因此在应用内删除内置模板现在不可逆。
- 既有墓碑保持原样：`deleted_template_ids` 与 `provider_templates` 不被重写，因此此前删除的内置模板按持久化状态保持隐藏，持久格式不变。
- 仍调用 `ai_gateway_reset_provider_templates` 的陈旧前端构建会收到未知命令错误，不写入任何内容且不崩溃，与此前项目移除的行为一致。
- 没有迁移与持久格式变更：既有 `ai_gateway.json` 可加载且行为一致，唯一差别是恢复操作不再存在。
- 没有任何活动 Note 被全部或部分取代：[API Gateway Provider Templates and Incremental Model Sync](../architecture/2026-09-18-api-gateway-provider-templates.md) 与 [Provider Templates Drop Built-in Model Catalogs and Prices](../architecture/2026-09-19-provider-template-manual-model-sync.md) 记录了模板绑定、同步、删除与忽略模型生命周期，但两者都未记录 reset 能力，因此两个记录均保持有效。
