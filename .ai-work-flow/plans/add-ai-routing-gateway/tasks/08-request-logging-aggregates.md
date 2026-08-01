# 08 - 实现日志、费用与每日聚合

- task_id: `ai-routing-logging-aggregates`
- order: `08`
- blocked_by: `ai-routing-router-http`
- source_plan: `../plan.md`
- source_plan_digest: `4fd0f21d03c9fd3060e030ce14e81636333e3060e47dee01c6720c5983f985ce`
- write_scope: `src-tauri/src/ai_routing_gateway/logging/**；src-tauri/src/ai_routing_gateway/storage/{request_logs.rs,aggregates.rs,maintenance.rs}；src-tauri/src/ai_routing_gateway/types/logging.rs；src-tauri/src/ai_routing_gateway/runtime/http.rs（仅接入日志事件）；对应 Rust 测试`

## Outcome

每个逻辑请求和账号尝试均生成脱敏、可筛选的持久记录，并以请求时价格快照事务化更新本机时区每日聚合。

## Implementation Checklist

- [ ] 在请求入口生成请求 ID 并固化 Key、模型、端点、价格和时区上下文。
- [ ] 记录每次账号尝试、流输出状态、错误类别和健康影响。
- [ ] 记录完成、失败、客户端取消和流式中断的逻辑结果。
- [ ] 实现 Token 分项、费用计算和不可计算原因。
- [ ] 在完成事务中增量更新每日聚合。
- [ ] 实现筛选、稳定游标分页、缺日补零和趋势查询。
- [ ] 实现保留期清理、手动清空、聚合重建与校验后台批次。

## Acceptance Criteria

- [ ] 每个逻辑请求有一条最终日志，每次上游尝试有独立且有序的尝试记录。
- [ ] 日志保留 Key、账号、分组和模型快照，相关实体删除后仍可读取。
- [ ] 不保存请求/响应正文、提示词、工具参数、Authorization、Cookie、OAuth token 或 API Key。
- [ ] 输入、输出、缓存读、缓存写和总 Token 保持独立字段及缺失语义。
- [ ] 价格和用量任一必要项缺失时费用为不可计算；只有已知零用量才是零费用。
- [ ] 请求完成日志与每日聚合在同一事务提交。
- [ ] 每日聚合使用请求发生时的本机日期和时区，不因后续时区变化重写。
- [ ] 7/15/30 日查询补齐缺失日期，但不把不可计算费用补为已知零。
- [ ] 筛选覆盖时间、账号快照、分组、公开/上游模型、状态、错误类别和 Key。
- [ ] 保留策略只允许 7/30/90/180 天或永久，默认 90 天；清理不进入请求热路径。
- [ ] 聚合重建结果可与增量聚合校验一致。

## Verification Steps

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::logging`。
- [ ] 覆盖成功、尝试切换、候选耗尽、客户端取消和流式中断。
- [ ] 使用敏感标记 fixture 扫描 SQLite、tracing 和序列化输出。
- [ ] 冻结时钟与时区，测试跨日、夏令时、缺日补零和维度筛选。
- [ ] 测试四类 Token 单价、价格覆盖、改价不追溯和不可计算费用。
- [ ] 测试保留期、手动清空、批次失败回滚、聚合重建和校验。
- [ ] 运行 `cargo check --manifest-path src-tauri/Cargo.toml`。

## Out of Scope

不实现 Tauri 命令、应用生命周期接线或前端首页和日志页面。
