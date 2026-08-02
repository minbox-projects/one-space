# 04 - 请求日志、尝试、费用、每日聚合与维护

- task_id: `task-04`
- order: `04`
- blocked_by: `task-03`
- source_plan: `../plan.md`
- source_plan_digest: `92f85a7f07acc328e48edf775eae5bfb751f58861b7c1b93de18fb68ed5fd822`
- write_scope: `src-tauri/src/ai_routing_gateway/ 内请求日志、attempt、费用、统计与维护模块；任务 03 runtime/router 中仅记录生命周期和维护触发的接入点`

## Outcome

每个逻辑请求及其上游尝试均形成脱敏、可查询的结构化记录，并能够按本机日期生成、重建和校验长期每日聚合及不可计算费用。

## Implementation Checklist

- [x] 负责请求日志、attempt、价格快照、Token 用量、费用计算、统计查询和后台维护实现。
- [x] 负责将日志事务接入任务 03 的请求执行路径；对 runtime/router 的修改仅限记录生命周期和维护触发，不改变路由或协议规则。
- [x] 负责时间、账号/组、公开/上游模型、状态、错误、Key 等筛选及稳定游标分页。
- [x] 负责保留策略、手动清空、批量清理、SQLite 维护、聚合重建与校验。

## Acceptance Criteria

- [x] 请求入口固化 request ID、Key、模型、端点、价格快照、本机日期和时区上下文。
- [x] 每次上游调用记录 attempt；成功、最终失败、客户端取消和流中断均记录 logical request。
- [x] 请求完成事务同时提交最终日志与每日聚合；日志写入失败不会伪报请求或聚合成功。
- [x] Token 分项支持输入、输出、缓存读、缓存写和总量，未知值保持缺失。
- [x] 缺少价格或用量时费用为不可计算，OAuth 费用明确标记为公开 API 单价等效预估。
- [x] 日志在账号或 Key 删除后仍可通过不可变快照读取，且不包含正文、提示词、工具参数或凭据。
- [x] 保留策略支持 7/30/90/180 天和永久；清理不进入请求热路径，每日聚合长期保留。
- [x] 趋势查询支持 7/15/30 天连续日期；只对真正无请求日期补零，未知费用不转换为零。
- [x] 测试覆盖本机时区、跨日/DST、筛选分页、清空回滚、保留、重建校验、价格不追溯和敏感 fixture 扫描。

## Verification Steps

- [x] 执行本任务 Acceptance Criteria 对应的日志、统计、费用与维护测试并确认全部通过。
  - `cargo test --lib ai_routing_gateway::request_logs::tests -- --nocapture`：退出状态 `0`；`7 passed, 0 failed`，覆盖原子提交/回滚、并发聚合、筛选分页、DST、保留/清空、补零、重建校验和敏感字段扫描。
  - `cargo test --lib shared_sqlite::tests::attempt_limit_upgrade_preserves_v1_rows_and_allows_oauth_refresh_attempts -- --nocapture`：退出状态 `0`；v1 attempt 数据保留、v2 前向迁移和 OAuth 刷新调用上限通过。
  - `cargo test`：退出状态 `0`；Rust 全量 `426 passed, 0 failed, 2 ignored`，Protocol Router 既有回归通过。
  - `cargo check`：退出状态 `0`；Rust 全量编译检查通过。
  - `cargo fmt --check`：退出状态 `0`；无格式差异。
  - `npm run build`：退出状态 `0`；TypeScript 与 Vite 生产构建通过（仅既有 chunk size/Browserslist 提示）。
  - `npm run lint`：退出状态 `0`；`0 errors, 386 warnings`，均位于本任务未修改的既有前端代码。
  - `npm run test`：退出状态 `0`；Vitest `30 files, 261 tests passed`。
  - `npm run check:cli-matrix`：退出状态 `0`；项目既有 CLI matrix 检查通过。
  - `git diff --check`：退出状态 `0`；无 whitespace error。

## Out of Scope

不实现 IPC 命令包装、前端页面或应用生命周期调度接线；不改变既有路由或协议规则。
