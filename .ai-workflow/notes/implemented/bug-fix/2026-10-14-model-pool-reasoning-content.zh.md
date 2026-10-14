# Agent Note: Model Pool Portable Contract Recognizes the Assistant reasoning_content Trace

Status: implemented

[English](2026-10-14-model-pool-reasoning-content.md) | 中文

## Problem

保存助手轮次 provider 专有 `reasoning_content` 的 Chat Completions 客户端会在下一轮多轮历史请求中重放它；普通客户端可以把该值作为字符串（含空串）或 `null` 重放，且当前研究表明 OpenCode 会发出并往返空与非空字段。模型池便携契约把每个无法识别的消息键都视为不支持，因此 `openai_conversion::validate_chat_message` 在任何候选被选择之前返回点名该字段的 `400`。于是一次普通的客户端历史重放会失败，尽管该字段是 provider 输出而非调用方控制面，且同协议 Chat 候选本已能忠实承载文本/工具历史。

## Decision

- `openai_conversion::validate_chat_message` 在便携 `role`/`content`/`tool_calls`/`tool_call_id` 键之外恰好再接受一个 provider 专有消息字段：仅限助手消息的 `messages[].reasoning_content`，其值必须是字符串（空、空白或非空）或 `null`。它仍是痕迹而非控制面，因此非助手角色或任何其他 JSON 类型仍在任何候选、SWRR 变更、尝试或探测之前按字段拒绝。
- 同协议 Chat 池候选原样保留该字段。该情形下 `forwarding.rs` 只改写模型，因此入站 `messages` 数组以完全相同的 JSON 值到达上游——字段、取值、键存在与数组顺序都被保留，而原始空白与键顺序字节不被保留——包括值为 `null` 的显式 `reasoning_content` 键。
- 两个方向的转换都省略该痕迹且不臆造推理：`chat_request_to_responses` 构建规范的、无状态的文本/工具输入且绝不发出 Responses `reasoning` 条目，既有 Chat 到原生 Anthropic Messages 的适配器同样不发出 `reasoning` 块。该 provider 专有文本绝不泄漏进可见消息内容。
- Chat 上的 `reasoning_effort`/`reasoningEffort`、不透明的 Responses `reasoning` 状态、未解析的 `item_reference` 以及每个未列出字段都保持其既有的按字段拒绝。不新增 schema、公共符号、服务商能力猜测或会话亲和行为。

## Alternatives considered

- 像此前一样拒绝 `reasoning_content` 并要求每个客户端自行剥离：拒绝，因为让每个客户端定制其历史序列化会破坏普通助手重放，而同协议 Chat 候选本已承载该字段且适配器识别这一唯一被识别的例外；该痕迹仅在该协议上被保留，不能移植到相反协议。
- 把任意明文 `reasoning_content` 映射为加密的 OpenAI Responses `reasoning` 条目：拒绝，因为明文没有忠实的加密推理映射，这样做会臆造 provider 状态。
- 把该痕迹带到转换后的请求上：拒绝，因为相反协议没有公共表示，且把它插入可见内容会破坏答案。
- 在每个角色和任意 JSON 形状上接受该字段：拒绝，因为该字段是仅限助手的类型化痕迹，因此仅助手角色检查与字符串或 `null` 类型检查会拒绝非法角色或非法痕迹取值。

## Consequences

- 读取存储的 Chat 助手 `reasoning_content`（字符串或 `null`）不再以 `400` 中止历史重放；同协议 Chat 路径投递精确历史，而 Responses 与原生 Messages 路径保持规范、不泄漏也不臆造推理。
- 限制：Chat 直发 Chat 的 provider 专有 `reasoning_content` 仍会到达被选中的上游，该上游可能拒绝它自己模型不认识的字段；网关不承诺跨模型的通用推理连续性。
- 验证：`src-tauri/src/ai_gateway/tests/openai_conversion.rs` 中四个新回归通过（模块过滤 `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::tests::openai_conversion::`，4 通过，退出 0）。在无后续产品改动的共享构建上，完整 `openai_conversion` 套件通过（42 通过，退出 0，含上述四个），原生 `anthropic_conversion` 通过（36 通过，退出 0），后端 `model_pools` 通过（22 通过，退出 0）——三个套件共 100 个唯一测试。
- 部分取代：[Model Pools Route One Public Alias Across Explicit Source Candidates](../feature/2026-10-13-free-model-pool.md) 予以保留并交叉引用；本记录只在其便携契约条目上新增唯一被识别的助手 `reasoning_content` 痕迹例外，该记录的其他每项决策仍然成立。
