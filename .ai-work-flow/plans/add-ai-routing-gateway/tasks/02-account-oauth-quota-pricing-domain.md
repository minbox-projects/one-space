# 02 - 账号池、OAuth、额度、模型与价格领域

- task_id: `task-02`
- order: `02`
- blocked_by: `task-01`
- source_plan: `../plan.md`
- source_plan_digest: `92f85a7f07acc328e48edf775eae5bfb751f58861b7c1b93de18fb68ed5fd822`
- write_scope: `src-tauri/src/ai_routing_gateway/ 内账号、组、标签、凭据、模型目录、映射、oauth、usage、pricing 领域模块及本阶段领域 DTO、错误、事件载荷`

## Outcome

后端能够完整管理账号池及其模型能力，通过受控官方 Codex OAuth 或第三方 API Key 建立安全凭据，并以动态额度和价格快照提供可测试的路由输入。

## Implementation Checklist

- [ ] 负责 `ai_routing_gateway` 内账号、组、标签、凭据、模型目录、映射、OAuth、额度刷新和价格领域实现及其存储事务。
- [ ] 负责本阶段领域 DTO、错误和事件载荷定义；跨层公共类型按领域拆分，后续任务不得重新定义同义结构。
- [ ] OAuth 仅实现固定官方端点、client ID、scope 和刷新语义，包括 PKCE loopback、手动完整回调 URL、Device Code。
- [ ] 实现稳定账号 ID upsert、默认组规则、删除迁移、永久删除确认令牌校验所需的后端领域能力。
- [ ] 实现 OAuth 动态额度窗口、刷新合并与退避、阈值判定、首页统计口径及模型价格优先级。

## Acceptance Criteria

- [ ] 每个账号恰属一个组且可关联多个标签；默认组唯一且不可删除，删除非空组在同一事务迁移账号。
- [ ] 永久删除账号会删除凭据、额度和模型映射，但不会删除请求历史与聚合快照。
- [ ] 第三方账号只接受允许的两种 OpenAI-compatible 上游协议，API Key 离开调用边界后立即加密且读取接口不返回明文。
- [ ] 公开模型与账号映射严格控制可用模型，未映射模型不可透传。
- [ ] PKCE、state、固定 scope、loopback、手动回调和 Device Code 状态机均严格校验，临时材料只驻留内存并在所有终态清理。
- [ ] 同一稳定 OAuth 账号重新授权时，凭据和元数据原子替换；并发额度刷新合并，失败执行有上限退避。
- [ ] 动态额度支持全局、模型、端点、能力和未知窗口；0/10/100 阈值、继承/覆盖、过期降级及自动恢复符合计划。
- [ ] 官方价格和第三方覆盖具备明确优先级；缺少价格或用量时费用保持不可计算。
- [ ] 本机 mock 测试覆盖 OAuth 三路径、Device Code 全状态、刷新语义、账号事务、额度窗口及价格快照，不访问公网。

## Verification Steps

- [ ] 执行本任务 Acceptance Criteria 对应的账号、OAuth、额度、模型与价格测试并确认全部通过。

## Out of Scope

不实现网关 Key、HTTP 端点、请求路由、Tauri commands、前端或生命周期接线。
