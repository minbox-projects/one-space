# Agent Note: Provider Template Reasoning Capability Sync

Status: implemented

[English](2026-10-16-provider-template-reasoning-sync.md) | 中文

## Problem

- CommandCode 服务商模板此前只抓取普通模型清单 `https://api.commandcode.ai/provider/v1/models`，该清单携带成员、名称与端点声明但不含推理强度元数据，因此同步会保留模板已有的档位，而每条派生的映射的 `reasoning_efforts` 始终是一个手动值。
- CommandCode 另外发布官方客户端目录 `https://api.commandcode.ai/provider/v1/models?view=client`，它声明 `reasoningEfforts`（字符串数组或 `null`）且不含端点；操作者希望这些显式声明的档位随模型清单一起经既有的手动与定时流程刷新，同时不覆盖操作者的独立编辑，也不新增持久字段、调度器或界面。

## Decision

- 规范来源门控位于 `src-tauri/src/ai_gateway/templates.rs`：仅当模板 id 为 `commandcode` 且 `models_url` 经 trim 后恰为 `https://api.commandcode.ai/provider/v1/models` 时，才通过既有的 15 秒 `reqwest` 客户端再发起一次不带凭据的客户端目录请求；其他任何 URL（包括配置了不同 URL 的 CommandCode 模板）只按配置抓取一次，绝不追加官方查询。
- 两次请求、客户端 schema 校验与精确 id 的合并都位于 `fetch_template_models_with` 接缝内，该接缝返回一份已并入的普通清单 JSON 体；`commands.rs` 与既有调用签名保持不变。
- 普通清单仍是模型成员、名称与端点协议的唯一权威；档位按精确上游 id 合并，客户端专有或隐藏目录条目绝不新增模型，客户端目录也绝不用于推断协议。
- 对象条目可声明 `reasoningEfforts`（与 `reasoning_efforts` 同时存在时前者优先）：数组按源顺序 trim、去空、去重，显式 `null` 或 `[]` 清空该集合，字段缺失或客户端目录缺少对应普通 id 时保留上一版值；任何其他声明形态都会让整次同步失败。
- CommandCode 只并入 Provider API 的五档 `low`、`medium`、`high`、`xhigh` 与 `max`；CLI 控制项 `off` 绝不导出为上游档位。其他模板逐字保留其显式声明的 token。
- 每条既有匹配映射的归一化集合（trim、去空、去重、排序）等于上一版模板对应模型集合时都跟随新集合，包括同一上游模型的每个别名、用户禁用与自动禁用行、以及已存在映射的禁用模板模型；独立自定义的集合保留不动，启用与健康标志绝不改变，忽略模型与禁用模板模型绝不新建映射，新建或恢复的映射继续经既有路径复制当前档位。
- 两份响应都在任何发布之前被抓取并解析，请求失败、客户端 `schemaVersion` 不受支持、目录 `data` 形态非法、客户端 id 重复或缺失、声明的档位值非法或持久化失败时，模板、派生服务商与 `synced_at` 均保持不变，并经受既有同步错误面报告。

## Alternatives considered

- 从模型名、其他服务商或第三方目录推断档位，或在运行时抓取 CLI 包：未采纳，因为这是猜测而非显式来源元数据，且与项目「绝不隐式猜测」的边界冲突。
- 新增持久档位字段、快照迁移或读路径归一化：未采纳，因为 `ProviderTemplateModel.reasoning_efforts` 已存在，同步只需写入显式值并保持 schema 3 不变。
- 用模板集合无条件覆盖每条派生映射的档位：未采纳，因为它会摧毁本变更要求保留的操作者独立编辑。
- 把 CLI 控制项 `off` 翻译成上游档位：未采纳，因为 Provider API 只接受五个值，而 `off` 是绝不应导出的 CLI 专有控制项。
- 把缺失的档位字段或缺失的补充 id 当作清空指令：未采纳，因为缺失必须保留上一版值，只有显式 `null` 或 `[]` 才清空。

## Consequences

- 交付代码为 `src-tauri/src/ai_gateway/templates.rs`、`src-tauri/src/ai_gateway/types_config.rs` 中的 `ModelMapping.reasoning_efforts` 注释与 `src-tauri/src/ai_gateway/tests/templates.rs` 中十四个新测试；`commands.rs` 未改动。
- 既有存储的规范 CommandCode 模板在下一次手动或定时同步时获得声明的档位，无需重置模板或迁移配置；没有元数据的通用来源保留上一版值；禁用的服务商、映射与健康状态绝不改变。
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::tests::templates` 退出 0、86 通过；`...ai_gateway::tests::auto_refresh` 退出 0、7 通过；`...--lib ai_gateway` 退出 0、966 通过，另有两条既有被忽略的性能用例；`cargo check --manifest-path src-tauri/Cargo.toml --tests` 退出 0。
- `MEMORY.md`、权威的 `.ai-workflow/index/navigation.json`（`ai-gateway-backend`）与重新生成的 `navigation.md` 在同一变更中记录显式元数据来源、缺失与清空语义、继承所有权规则与失败边界。
- 取代关系：部分取代。本记录仅就推理能力的所有权与来源部分取代 [Gateway Reasoning Efforts Sync to OpenCode Model Variants](../architecture/2026-09-20-gateway-reasoning-efforts-in-terminal-sync.md)，因为模板同步现在会把 `reasoning_efforts` 写入继承的映射；该记录的变体投影决策仍然有效。本记录未取代 [Provider Templates Refresh Automatically on a Persisted Interval](../feature/2026-09-23-template-auto-refresh.md)、[Template Auto Refresh Runs One Batch at App Start](../bug-fix/2026-09-29-template-autorefresh-startup-batch.md) 与 [Core Workflows Cleanup and Optimization](../architecture/2026-10-09-core-workflows-cleanup-and-optimization.md)，它们的调度器与核心工作流决策仍然有效，因为这是显式来源元数据同步，而非普通读取或写入期的推断。
