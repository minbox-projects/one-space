# Agent Note: 模板模型清单同步优先采用 Responses 端点

Status: implemented

[English](2026-11-25-template-sync-prefers-responses.md) | 中文

## Problem

模板模型清单同步依据声明的 `supported_endpoints` 推导每个模型的上游协议，并且此前让 Chat Completions 优先于 Responses。因此同时声明 `/responses` 与 `/chat/completions` 的目录条目会同步为 `chat_completions`，源声明的 Responses 能力无法经模板派生服务商被 Codex 这类 Responses 调用方使用，派生映射的协议也与其来源矛盾。CommandCode 官方目录中就存在这样的条目，例如 `gpt-5.6-luna` 与 `gpt-5.6-sol`。

## Decision

- `src-tauri/src/ai_gateway/templates/source.rs` 把声明的 `supported_endpoints` 数组按 Responses > Chat Completions > Anthropic Messages 解析：`/responses` 优先于 `/chat/completions`，`/chat/completions` 优先于 `/messages` 与 `/v1/messages`，因此同时声明 `/responses` 与 `/chat/completions` 的模型现在同步为 `responses`。
- 识别规则的其余部分不变：仅 Chat 的条目仍为 `chat_completions`，仅 Responses 的条目仍为 `responses`，`/messages` 与 `/v1/messages` 仍为 `anthropic_messages`，字段缺失或非数组时保留条目且不带协议并继承模板协议，数组不含上述任何端点时仍丢弃该条目。
- 该偏好把来源声明的更强能力放在前面，因为声明的 Responses 端点必须对 Codex 这类 Responses 调用方保持可用，而 OpenCode 终端同步已为真正仅 Responses 的本地模型提供 `@ai-sdk/openai` npm 覆盖，因此最终只经 Responses 服务的模型仍保有终端覆盖。

## Alternatives considered

- 保持 Chat Completions 优先：未采纳，因为这会隐藏来源声明的能力，并挡住上游本可服务的 Responses 调用方。
- 依据 GPT 或模型名启发式选择协议：未采纳，因为模型名不是协议权威，只有声明的端点才是。
- 新同步保持 Chat，只在服务商已配置为 `responses` 时尊重 Responses：未采纳，因为新同步的模型仍会以 `chat_completions` 起步，声明的 Responses 支持要等操作者手工改行后才可达。

## Consequences

- 同时声明 `/responses` 与 `/chat/completions` 的模型现在同步为 `responses`，而仅 Chat 的模型保持 `chat_completions`；示例 `gpt-5.6-luna` 与 `gpt-5.6-sol` 现在同步为 Responses。
- 该变更在下一次模板同步时到达既有派生服务商：协议仍等于上一版模板值时才被更新；被操作者改过协议的行不再匹配，因此保留自己的值。
- OpenCode 终端既有的按模型 `@ai-sdk/openai` npm 覆盖让真正仅 Responses 的本地模型保持可用，因此该偏好不新增任何终端侧规则。
- 手动逐行协议覆盖保持既有传播语义：`local_model` 绝不被改动，映射生效协议等于服务商协议时不存协议值，已编辑的值绝不覆盖。
- 本记录仅在偏好顺序上部分取代 [API Gateway Provider Templates and Incremental Model Sync](../architecture/2026-09-18-api-gateway-provider-templates.md)、[Provider Templates Drop Built-in Model Catalogs and Prices](../architecture/2026-09-19-provider-template-manual-model-sync.md) 与 [Native Anthropic Messages Entry, Protocol Recognition and OpenAI Entry Conversion](../architecture/2026-10-11-ai-gateway-anthropic-messages.md)；这些记录被保留并交叉链接，其所有其他决策继续有效。
