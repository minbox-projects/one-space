# Agent Note: Source Modules Use Facades over Responsibility Submodules

Status: implemented

[English](2026-10-27-source-module-split.md) | 中文

## Problem

多个非测试源码文件因不断累积互不相关的职责而超过 1,200 行：React 状态与标记、命令封装、协议辅助、持久化、运行时编排与内联测试接缝共处一个文件。这种体积使归属难以辨认，使导航索引指向单体入口，并迫使审阅者同时理解许多关注点。行为保持的拆分必须在不让导入方、测试、Tauri 命令调用方、持久化文件或翻译消费者观察到任何变化的前提下解决该问题，因此任何需要改动逻辑、签名、条件、hook 顺序或序列化的分解都必须停止，而不是被强行实施。

## Decision

每个超过 1,200 行阈值的原始非测试源码文件现在都在原路径上成为门面或容器，且不超过 1,200 行。`src/i18n.ts` 与 `src/lib/aiGateway.ts` 变为目录入口门面 `src/i18n/index.ts` 与 `src/lib/aiGateway/index.ts`；其余每个原始路径都保留为门面或容器，包括与 `src/components/sshTunnels/` 并存的、大小写安全的 `src/components/SshTunnels.tsx`。

Rust 目标沿用既有的 `foo.rs` 加 `foo/` 形式。根文件声明其子模块，再导出消费者或内联 `#[cfg(test)]` 模块可达的每个 `public`、`pub(crate)` 与 `pub(in crate::ai_gateway)` 路径，并保持生成的命令宏与命令注册不变。职责子模块留在同一域内，且可继续成为 `commands/`、`model_pools/`、`openai_conversion/`、`runtime_http/`、`selection/`、`templates/`、`types_config/` 与 `usage_log/` 等目录。

前端纯模块使用目录入口覆盖无环的领域文件。i18n 入口按功能前缀合并语言领域文件为同一份英文与中文翻译资源，AI 网关入口再导出类型、命令、模型池、服务商、价格、用量与模板，使消费者仍导入 `@/lib/aiGateway`。大型 React 组件在原路径保留容器，并组合同一域的模块级展示组件与 hooks，hook 顺序与副作用依赖不变。

恰好一个新文件超过 1,200 行：`src-tauri/src/ai_gateway/runtime_http/streaming.rs` 为 1,300 行，包含逐字节相同、未改动任何语句即搬移的 `attempt_streaming_inner`。其余每个新文件都不超过 1,200 行。

不变更行为是该架构的一部分：没有改动任何业务逻辑、条件、计算、参数、返回形状、签名、序列化属性、命令名或注册、事件名或负载、持久化格式、schema 版本、i18n 键或值、界面布局、文案或测试。当拆分需要上述任一改动时，工作停止并报告边界，而不是改写语义。

## Alternatives considered

- 保留大文件并用分节注释划分：未采纳，因为注释不会减少文件体积、归属歧义或审阅面。
- 把公共符号移到新模块路径并更新导入方：未采纳，因为每个既有说明符、消费者与测试都必须无需编辑即可解析。
- 把每个入口重建为具有不同符号身份的新 barrel：未采纳，因为它可能改变模块身份、生成的命令宏与内联测试接缝。
- 进一步拆分 `attempt_streaming_inner` 以满足 1,200 行上限：未采纳，因为那需要语句级改动，因此改为记录一个函数体逐字节相同的 1,300 行例外。
- 引入共享的通用拆分引擎，或合并 Skills 与 Subagents 的实现：未采纳，因为那会改变行为且超出结构目标。
- 按字母大小写重命名或移动 `SshTunnels.tsx`：未采纳，因为 APFS 大小写不敏感，门面必须与既有 `sshTunnels/` 目录并存。
- 新增或修改测试以便验证分解：未采纳，因为既有测试套件必须保持为未改动的行为门禁。

## Consequences

- 每个原始入口路径解析到同一公共表面，因此既有导入、Tauri 命令注册、事件契约、持久化文件、schema 版本与翻译保持兼容。
- 新实现文件以 1,200 行为上限，流式文件是唯一的 1,300 行例外；每个原路径上的门面或容器都不超过 1,200 行。
- 导航索引现在指向目录入口以及声明公共符号的领域模块文件，而不是已退役的单体路径。
- 没有新增、删除或修改任何测试文件，既有前端与 Rust 测试套件仍是拆分的行为门禁。
- 每个结构步骤都可独立回退，因为消费者依赖门面，而不是另一步骤的新内部模块。
- 验证：针对当前文档状态，`ai-workflow context validate` 与 `ai-workflow notes validate` 通过，且记录的路径与符号和交付树一致。
- 没有活动 Note 被完整或部分取代。关于功能行为、网关路由、持久化与性能的既有记录仍然有效，因为本记录只改变模块边界并保留其已记录的契约。
