# 07 - 实现 Responses、Chat 与 SSE 协议层

- task_id: `07-protocol-conversion-sse`
- order: `07`
- blocked_by: `01-shared-sqlite-schema`
- source_plan: `../plan.md`
- source_plan_digest: `037804aa9bfa9cdfc9001966bb673f99116f870c328e29c2f1e5ad7aa4c79d19`
- write_scope: `src-tauri/src/ai_routing_gateway/{protocol/,tests/protocol.rs}`

## Outcome

网关具备独立规范化协议模型，可受控透传同协议请求并完成 Responses 与 Chat Completions 的流式和非流式双向转换。

## Implementation Checklist

- [ ] 建立规范化请求、非流式响应、流事件、用量、工具调用、reasoning 和结束原因模型。
- [ ] 实现 Responses 与 Chat Completions 请求和响应的双向转换。
- [ ] 实现 SSE 事件顺序、工具调用增量、reasoning、用量和结束原因转换。
- [ ] 建立明确能力矩阵，在访问上游前拒绝无法无损表达的字段。
- [ ] 实现 OpenAI-compatible 错误 envelope 和稳定协议错误码映射。
- [ ] 保证同协议路径只做必要校验和受控透传，不进行无意义语义重写。

## Acceptance Criteria

- [ ] 双向 fixture 矩阵覆盖流式、非流式、工具调用、reasoning/推理强度、Token 分项和结束原因。
- [ ] SSE 转换保持事件顺序及增量关联，不合并或重排工具调用片段。
- [ ] 无法无损转换的请求在任何上游调用前返回 `400` 和稳定机器码。
- [ ] 上游兼容错误可映射为无账号信息、无内部堆栈的 OpenAI-compatible envelope。
- [ ] 同协议 fixture 验证字段语义没有被不必要重写。
- [ ] 测试完全使用本地 fixture，不依赖公网或浏览器自动化。

## Verification Steps

- [ ] 执行 Responses/Chat 双向 fixture 测试。
- [ ] 执行 SSE 顺序、增量、取消边界和错误 envelope 测试。
- [ ] 执行能力矩阵拒绝测试，确认 mock upstream 未收到不支持请求。

## Out of Scope

不实现 Anthropic、Gemini、WebSocket、账号选择或 HTTP 监听器。
