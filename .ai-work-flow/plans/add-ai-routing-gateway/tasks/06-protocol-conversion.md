# 06 - 实现双协议转换层

- task_id: `ai-routing-protocol`
- order: `06`
- blocked_by: `ai-routing-storage-security-foundation`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src-tauri/src/ai_routing_gateway/protocol/**；src-tauri/src/ai_routing_gateway/types/protocol.rs；对应 Rust fixture 与单元测试`

## Outcome

Responses 和 Chat Completions 请求、非流式响应及 SSE 事件可通过规范化模型受控透传或双向转换，无法无损表达的请求在访问上游前失败。

## Implementation Checklist

- [ ] 定义规范化请求、响应、流事件、工具调用、reasoning、用量和结束原因模型。
- [ ] 定义客户端与上游协议能力矩阵。
- [ ] 实现同协议受控透传。
- [ ] 实现 Responses 到 Chat Completions 双向请求和响应转换。
- [ ] 实现 SSE 增量转换与稳定事件顺序。
- [ ] 实现 OpenAI-compatible 错误 envelope 和机器码映射。

## Acceptance Criteria

- [ ] 非流式、流式、工具调用、reasoning/推理强度、用量和结束原因均有双向 fixture。
- [ ] SSE 保持事件顺序、工具调用增量顺序和最终用量语义。
- [ ] 同协议路径不执行无必要的字段重写。
- [ ] 能力矩阵判定不能无损表达时，在上游调用前返回 HTTP `400` 和 `unsupported_lossless_conversion`。
- [ ] 不静默删除未知或不兼容字段，不猜测协议语义。
- [ ] 外部错误统一为 OpenAI-compatible `error` envelope，不包含账号、凭据或内部堆栈。
- [ ] 至少定义计划要求的认证、权限、模型、候选、转换、请求、限流、授权、暂时不可用和未就绪错误类别。

## Verification Steps

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::protocol`。
- [ ] 运行完整 Responses/Chat 双向非流式 fixture 矩阵。
- [ ] 运行 SSE、工具增量、reasoning、Token 分项和结束原因 fixture 矩阵。
- [ ] 使用调用计数 mock 验证不兼容输入返回 `400` 时上游调用次数为零。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`。

## Out of Scope

不实现账号候选、HTTP listener、上游健康状态、持久日志或 UI。
