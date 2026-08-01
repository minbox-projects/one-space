# 03 - 网关 Key、HTTP Runtime、路由健康与双协议

- task_id: `task-03`
- order: `03`
- blocked_by: `task-02`
- source_plan: `../plan.md`
- source_plan_digest: `92f85a7f07acc328e48edf775eae5bfb751f58861b7c1b93de18fb68ed5fd822`
- write_scope: `src-tauri/src/ai_routing_gateway/ 内网关 Key、runtime、router、protocol 模块及其测试`

## Outcome

独立 loopback HTTP 网关可通过网关 Key 安全提供四个固定端点，并按权限、健康和额度确定性选择上游，完整处理 Responses 与 Chat Completions 的流式和非流式转换。

## Implementation Checklist

- [ ] 负责网关 Key 的安全材料、权限和生命周期，以及 `runtime`、`router`、`protocol` 子系统主体实现。
- [ ] 负责 HTTP 内部 runtime 状态机、loopback listener、输入限制、端口预检/受控重绑能力；不在应用生命周期中自动启动。
- [ ] 负责候选过滤、确定性排序、最多三次尝试、OAuth 刷新重试、冷却、熔断、单次探测和首字节切换门禁。
- [ ] 负责规范化协议模型、同协议受控透传、Responses/Chat 双向转换、SSE、工具调用、reasoning、usage、finish reason、取消和错误 envelope。
- [ ] 唯一负责 HTTP 对外错误码及四个端点行为；不得增加计划外端点或网络绑定方式。

## Acceptance Criteria

- [ ] 网关 Key 使用高熵随机源，数据库仅保存前缀与加盐哈希或等效材料，明文只在创建或重生成响应中返回一次。
- [ ] Key 的组/模型权限、禁用、撤销、过期和重生成即时影响新请求。
- [ ] 服务仅绑定 `127.0.0.1`，默认端口为 `17688`；端口冲突保持停止且不循环抢占。
- [ ] `/health` 匿名且仅返回状态和版本；其余三个端点验证 Bearer Key 并返回 OpenAI-compatible 错误 envelope。
- [ ] `/v1/models` 只返回当前 Key 有权访问且至少一个可路由账号支持的公开模型。
- [ ] 候选过滤和排序严格遵循计划顺序，结果确定；每个逻辑请求最多尝试三个不同账号。
- [ ] 流式请求只允许首字节前切换；首字节后固定账号，客户端断开会取消上游且不记健康失败。
- [ ] 401/403、额度耗尽、429、网络错误、5xx、熔断和探测行为符合计划定义。
- [ ] Responses/Chat 双向 fixture 覆盖非流式、SSE、工具调用、reasoning、usage、结束原因和错误转换。
- [ ] 不可无损转换的请求在访问上游前返回 HTTP 400 `lossless_conversion_unsupported`，loopback mock 确认未收到请求。
- [ ] HTTP 与路由测试使用本机 loopback mock，不访问公网或启动浏览器。

## Verification Steps

- [ ] 执行本任务 Acceptance Criteria 对应的网关 Key、HTTP、路由与协议测试并确认全部通过。

## Out of Scope

不修改 `run_app.rs`、前端 facade、导航或 UI；不在应用生命周期中自动启动；不增加计划外端点或网络绑定方式。
