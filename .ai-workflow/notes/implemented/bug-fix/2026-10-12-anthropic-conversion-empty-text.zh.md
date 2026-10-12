# Agent Note: Anthropic 转换丢弃空白文本块并拒绝无内容块消息

Status: implemented

[English](2026-10-12-anthropic-conversion-empty-text.md) | 中文

## Problem

`src-tauri/src/ai_gateway/anthropic.rs` 中的 Anthropic 请求转换把每个 Chat 或 Responses 文本值都转成原生 `{"type":"text","text":...}` 块而不检查文本，因此空或仅空白文本会产出空白 `text` 块，且文本全部被过滤掉的消息可能以无内容块的 `content` 数组到达 Messages 上游。Chat 的字符串/数组内容与 Responses 的字符串/数组内容由各自路径处理，任何局部修复都会让某类调用方形态继续发出空白块。

## Decision

- 在 `anthropic.rs` 中新增一个共享的私有 helper `text_block(text)`：空或仅空白文本返回 `None`，否则原样保留文本，包括前导与尾随空白。
- 对 Chat 的 `content` 字符串或内容块数组、Responses 的 `content` 字符串或内容块数组应用同一规则。图片块保持不变：与图片并列的空白文本块被丢弃，图片保留。
- 带工具调用的 assistant 消息在 `content` 为空白时只保留幸存的 `tool_use` 块；`tool_result` 照旧输出。`null` 或缺省的 assistant `content` 保持既有行为，包括存在工具调用时。
- 过滤后没有任何内容块的消息是本地 400：Chat 记为 `messages.content`、Responses 记为 `input.content`，涵盖空的顶层 Responses `input` 字符串、空的逐消息内容，以及空的 Chat `system` 或 `developer` 消息。空的 Responses `instructions` 属可选内容而被跳过。
- 该拒绝经既有 `validate_conversion_request` 转换校验在任何 Anthropic 尝试之前返回，网关错误信封不变：字段仍在 `error.message`，`error.param` 仍为 `null`。

## Alternatives considered

- 仅跳过空的 assistant 字符串，不规范化 Responses 与内容块数组形态。拒绝原因是必须统一规则，使任何调用方形态的转换请求都不携带空白 `text` 块；局部修复仍会让 Responses 与数组内容发出空白块。

## Consequences

- 转换后的 Chat 或 Responses 请求都不携带空或仅空白的 `text` 块，非空文本原样保留。
- 无内容块的消息是本地 400，不触及上游、不结算 key 或探测健康，在任何 Anthropic 尝试之前返回且保持不变的 OpenAI 错误信封。
- 原生 `/messages` 入口、转换的 stream 标志、配置 schema、前端与命令均未改变。
- 本记录补充既有 [原生 Anthropic Messages 入口、协议识别与 OpenAI 入口转换](../architecture/2026-10-11-ai-gateway-anthropic-messages.md) Note，不改变其任何决策，也无需 supersession。
