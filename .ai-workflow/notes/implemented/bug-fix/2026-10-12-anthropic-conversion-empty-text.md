# Agent Note: Anthropic Conversion Drops Blank Text Blocks and Rejects Block-less Messages

Status: implemented

English | [中文](2026-10-12-anthropic-conversion-empty-text.zh.md)

## Problem

The Anthropic request conversion in `src-tauri/src/ai_gateway/anthropic.rs` turned every Chat or Responses text value into a native `{"type":"text","text":...}` block without checking the text. An empty or whitespace-only value therefore produced a blank `text` block, and a message whose text all filtered away could reach a Messages upstream as a block-less `content` array. Chat string/array content and Responses string/array content were handled by separate paths, so a partial fix would have left some caller shape still emitting a blank block.

## Decision

- Add one shared private helper `text_block(text)` in `anthropic.rs`: it returns `None` for empty or whitespace-only text and otherwise keeps the text verbatim, including leading and trailing whitespace.
- Apply the same rule to a Chat `content` string or content-block array and to a Responses `content` string or content-block array. Image blocks are unchanged: a blank text block beside an image is dropped while the image is retained.
- An assistant message with tool calls keeps only its surviving `tool_use` blocks when its `content` is blank; a `tool_result` is emitted as before. A `null` or absent assistant `content` keeps its existing behavior, including when tool calls are present.
- A message that filters down to zero content blocks is a local 400 naming `messages.content` for Chat or `input.content` for Responses, which includes a blank top-level Responses `input` string, blank per-message content and a blank Chat `system` or `developer` message. Blank Responses `instructions` is optional and skipped instead.
- The rejection is returned through the existing `validate_conversion_request` conversion check before any Anthropic attempt, and the gateway error envelope is unchanged: the field stays in `error.message` and `error.param` stays `null`.

## Alternatives considered

- Only skip a blank assistant string and leave the Responses and content-array shapes unnormalized. Declined because one unified rule is required so that no converted request carries a blank `text` block for any caller shape; a partial fix would still emit blank blocks from Responses and array content.

## Consequences

- No converted Chat or Responses request carries an empty or whitespace-only `text` block, and nonblank text is preserved verbatim.
- A block-less message is a local 400 that reaches no upstream and settles no key or probe health, delivered before any Anthropic attempt with the unchanged OpenAI error envelope.
- The native `/messages` entry, the converted stream flag, the configuration schema, the frontend and the commands are unchanged.
- This record supplements the existing [Native Anthropic Messages Entry, Protocol Recognition and OpenAI Entry Conversion](../architecture/2026-10-11-ai-gateway-anthropic-messages.md); it changes none of that note's decisions and requires no supersession.
