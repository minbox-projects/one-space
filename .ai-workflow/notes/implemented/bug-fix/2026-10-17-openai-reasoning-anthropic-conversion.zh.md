# Agent Note: OpenAI 推理控制在 Anthropic Messages 转换中被翻译

Status: implemented

[English](2026-10-17-openai-reasoning-anthropic-conversion.md) | 中文

## Problem

当 OpenAI 入口请求（`/v1/chat/completions` 或 `/v1/responses`）被转换为原生 Anthropic Messages 上游时，`src-tauri/src/ai_gateway/anthropic.rs` 中的 `reject_common_conversion_fields` 把每个非空推理选项都视为不可翻译，并在任何 Anthropic 尝试之前按字段以 400 拒绝。因此设置 Chat `reasoning_effort`/`reasoningEffort` 或 Responses `reasoning.effort` 的调用方完全无法触达 Messages 上游，而像 Codex 那样总是发送输出呈现成员 `reasoning.summary` 的客户端也被一个从不改变推理深度的控制挡住。原生 Messages 上游接受 `output_config.effort` 与 `thinking`，所以被识别的调用方控制需要翻译而非拒绝。

## Decision

- Chat 先识别顶层 `reasoning_effort`，再识别兼容拼写 `reasoningEffort`；第一个存在且非空白的字符串生效，与既有 `runtime_http::extract_reasoning_effort` 的日志语义一致。`null` 与空白或仅空白取值表示无控制，存在的非字符串取值是按字段 400。
- Responses 识别 `reasoning` 对象：缺失或 `null` 表示无控制，非对象是按字段 400，`effort` 遵循同一取值规则且错误点名为 `reasoning.effort`，`summary` 为字符串或 `null` 时被接受并忽略，其他形态是以 `reasoning.summary` 点名的 400。任何其他成员是按字段 400，例如 `reasoning.mode`。
- 私有 `insert_reasoning_effort` 表翻译取值：`low`/`medium`/`high`/`xhigh`/`max` 变为 `output_config: {"effort": <同值>}`，`minimal` 变为 `output_config: {"effort": "low"}`，`none` 变为 `thinking: {"type": "disabled"}`。任何其他取值（含 `auto` 与大小写变体）都是点名 `unsupported reasoning effort '<value>'` 的按字段 400。
- 翻译在既有尝试前 `validate_conversion_request` 路径内运行，因此不受支持的控制仍在任何上游尝试之前失败且不结算任何 key 健康。
- 私有 helper 为 `insert_reasoning_effort`、`apply_chat_reasoning_control` 与 `apply_responses_reasoning_control`；`reject_common_conversion_fields` 不再处理推理并去掉了其 `chat` 参数。
- 不改变任何公开符号、配置 schema、持久格式或池路径：池请求仍在转换前剥离其被识别的入口控制并优先注入操作者声明的原生控制。

## Alternatives considered

- 静默丢弃控制并转发请求：拒绝，因为请求更多或更少推理的调用方会收到上游默认值，其显式意图被歪曲。
- 不使用 `minimal` 与 `none` 映射而原样透传客户端取值：拒绝，因为 Anthropic 没有 `minimal` 档位也没有顶层 `reasoning_effort` 字段，原样取值会被上游拒绝或忽略，而无法表达所请求的深度。
- 把档位映射到 `thinking.budget_tokens`：拒绝，因为调用方提供的是档位而不是 token 预算，臆造预算会虚构一个调用方从未选择的取值；该档位改为映射到 `output_config.effort`。
- 拒绝 Responses 的 `reasoning.summary` 成员：拒绝，因为 summary 是输出呈现控制、从不改变推理深度，转换无论如何都无法交付推理摘要，且拒绝它会挡住像 Codex 那样总是发送它的客户端。
- 像此前一样继续拒绝每个推理选项：拒绝，因为原生 Messages 上游能经 `output_config` 与 `thinking` 兑现被识别的档位，一律 400 挡住了上游本已支持的能力。

## Consequences

- 改动路径为 `src-tauri/src/ai_gateway/anthropic.rs`（+122/-21）与 `src-tauri/src/ai_gateway/tests/anthropic_conversion.rs`（新增测试加一条注释）。
- 交付的翻译为：

| Client reasoning control | Native Anthropic control |
| --- | --- |
| Chat `reasoning_effort`/`reasoningEffort` 或 Responses `reasoning.effort` 取值为 `low`/`medium`/`high`/`xhigh`/`max` | `output_config: {"effort": <同值>}` |
| 同一字段取值为 `minimal` | `output_config: {"effort": "low"}` |
| 同一字段取值为 `none` | `thinking: {"type": "disabled"}` |
| Responses `reasoning.summary`（字符串或 `null`） | 被接受并忽略 |
| 未知取值（含 `auto` 与大小写变体）、非字符串、非对象 `reasoning` 或未知成员 | 在任何尝试之前按字段 400 |

- 测试证据：`cargo test --manifest-path <worktree>/src-tauri/Cargo.toml --lib ai_gateway::tests::anthropic_conversion` 退出 0，40 通过；`cargo test --manifest-path <worktree>/src-tauri/Cargo.toml --lib ai_gateway::` 退出 0，971 通过，外加两个既有被忽略的 `core_workflows_perf_*` 测量测试。
- 本记录仅在「非空不可翻译推理选项会按字段以 400 拒绝」这一边界上部分取代 [原生 Anthropic Messages 入口、协议识别与 OpenAI 入口转换](../architecture/2026-10-11-ai-gateway-anthropic-messages.md)；该记录被保留并交叉链接，其所有其他决策继续有效。
- 本记录仅在「`anthropic.rs` 保持不改，普通 Anthropic 适配器保留其拒绝规则」这一边界上部分取代 [Model Pools Own a Bounded Pool-Local Reasoning Policy](../feature/2026-10-15-pool-only-reasoning.md)；该记录被保留并交叉链接，其所有其他决策继续有效。
