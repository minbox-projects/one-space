# Agent Note: Gateway Reasoning Efforts Sync to OpenCode Model Variants

Status: implemented

[English](2026-09-20-gateway-reasoning-efforts-in-terminal-sync.md) | 中文

## Problem

服务商映射的 `reasoning_efforts` 列表记录该模型对外公布的推理强度档位，操作者在 `ProviderDetailDialog` 中维护它。终端同步此前忽略该字段：`build_gateway_provider`（`src-tauri/src/api_gateway/commands.rs`）把每个 opencode 模型条目只写成 `{ "name": ... }`，`render_opencode` 再把这份 `tool_config.models` 映射原样投影进 `~/.config/opencode/opencode.json`。于是 OpenCode 看到的是一个未声明任何档位的自定义模型，回退到自身目录或按模型 id 的内建推断，因此像 `deepseek-v4.1-flash` 这类模型提供的档位与网关配置的档位不一致。早前的记录出于刻意设计写下了相反的约定（「绝不写入终端同步配置」），而这正是操作者报告为缺陷的行为。

## Decision

终端同步构造 opencode 网关服务商时，凡 `reasoning_efforts` 非空的映射都会把档位写入其模型条目。`build_gateway_provider` 写入 `"reasoning": true`，以及一个以各档位标识符为键、值为 `{ "reasoningEffort": "<effort>" }` 的 `variants` 对象；没有档位的映射保持原有的 `{ "name": ... }` 形态，因此未配置档位的模型不变。档位标识符会被去除首尾空白，空白项被丢弃，重复项通过对象键自然合并。映射键仍为 `local_model`，重复仍保留首个，且只按既有规则输出启用服务商的启用行。codex 不受影响：其网关记录仍只选择单个 `model` 且不带 variants。`reasoning_efforts` 仍不存在于模板中、也绝不由模板同步写入；只是终端同步的输出现在会反映它。

## Alternatives considered

- 保持终端同步原样并依赖 OpenCode 的内建 variants：未采纳，因为工具随后会公布操作者从未配置过的档位，这正是被报告的「不一致」；映射列表才是操作者的唯一事实来源。
- 把档位写成单个模型级 `options.reasoningEffort`：未采纳，因为单值只会固定一个强度，而不是公布可选集合；OpenCode 用 `variants` 建模档位选择。
- 只写 `variants` 而不写 `reasoning: true`：未采纳，因为该标志把模型标记为推理模型并驱动 TUI 的 variant 开关，否则这些 variants 不会在普通自定义模型上出现。
- 同时为 codex 输出 variants：未采纳，因为 codex 网关记录只选择一个模型，并用单个 `model_reasoning_effort` 表达强度；档位列表在那里无处安放，且需求只点名 opencode。

## Consequences

- `api_gateway_sync_terminal` 写出的 opencode 网关服务商现在为该模型精确公布映射配置的档位；空列表产生旧的极简条目，因此旧构建读到的形态不变。
- 持久化网关 schema、命令签名与模板形态都不变：`reasoning_efforts` 仍是带 serde 默认的映射字段，回滚只会在下一次同步时丢弃多出来的模型键。
- `MEMORY.md` 与 `api-gateway` / `api-gateway-backend` 导航条目现在描述同步的 variants；`navigation.md` 已按权威 JSON 重新生成。
- Supersession（取代评估）：部分取代。[API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md) 曾声明映射推理档位绝不写入终端同步配置；该终端同步事实由本记录更正，其称谓、模板绑定、忽略集合生命周期、「仅当未被改动才更新」的合并与星期价格决策继续有效。其余 active 记录互不相关。
