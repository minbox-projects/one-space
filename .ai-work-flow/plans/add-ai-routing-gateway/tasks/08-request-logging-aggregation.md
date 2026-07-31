# 08 - 实现请求日志、尝试、聚合与保留

- task_id: `08-request-logging-aggregation`
- order: `08`
- blocked_by: `03-account-pool-model-mapping, 05-quota-threshold-pricing`
- source_plan: `../plan.md`
- source_plan_digest: `037804aa9bfa9cdfc9001966bb673f99116f870c328e29c2f1e5ad7aa4c79d19`
- write_scope: `src-tauri/src/ai_routing_gateway/{logging.rs,aggregates.rs,maintenance.rs,tests/logging.rs}`

## Outcome

每个逻辑请求和账号尝试都有脱敏结构化记录，费用按请求时价格固化，每日聚合、筛选分页、保留清理和重建校验均可执行。

## Implementation Checklist

- [ ] 在请求入口生成请求 ID，并固化 Key、账号、模型、端点、价格和本机时区的非敏感上下文。
- [ ] 实现账号尝试记录以及完成、失败、客户端取消和流式中断的逻辑请求结果。
- [ ] 统一输入、输出、缓存读、缓存写和总 Token，保存价格快照、费用或不可计算原因。
- [ ] 在请求完成事务中增量更新本机日期每日聚合，并实现 7/15/30 日连续日期补零查询。
- [ ] 实现多维筛选、稳定游标分页、尝试明细、默认 90 天及 7/30/90/180 天或永久保留。
- [ ] 实现后台手动清空、批量清理、适度维护、聚合重建和校验。

## Acceptance Criteria

- [ ] 每个逻辑请求及每次账号尝试均保存规定字段、输出流标记、错误类别和健康影响。
- [ ] 账号或 Key 删除后，历史名称和稳定标识快照仍可查询。
- [ ] 请求完成日志与每日聚合在同一事务中提交，失败时不会形成部分聚合。
- [ ] 跨日和夏令时测试遵循请求发生时的本机时区归属，缺日补零不把未知费用变成零。
- [ ] 默认及全部可选保留策略、批次失败回滚、手动清空、重建和校验均有测试。
- [ ] 敏感标记扫描确认数据库、tracing 和测试输出中没有正文、提示词、工具参数、Authorization、Cookie、Token 或 API Key。
- [ ] 清理和维护不在请求热路径执行。

## Verification Steps

- [ ] 执行日志字段、尝试记录、实体删除快照和敏感数据扫描测试。
- [ ] 执行冻结时区下的聚合、补零、筛选、费用和重建校验测试。
- [ ] 执行保留期、手动清空和事务失败回滚测试。

## Out of Scope

不实现 HTTP 请求转发、前端图表或完整请求和响应正文存储。
