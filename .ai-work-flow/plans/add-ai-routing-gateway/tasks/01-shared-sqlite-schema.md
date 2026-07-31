# 01 - 建立共享 SQLite 与网关 Schema

- task_id: `01-shared-sqlite-schema`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `385b139e1c25f8e8112982ed63ac3c3f0282be095c8322006f82f45d9070cf6d`
- write_scope: `src-tauri/src/shared_sqlite/（新建）、src-tauri/src/ai_routing_gateway/{mod.rs,storage.rs,types.rs,tests/storage.rs}、SQLite 相关依赖配置`

## Outcome

应用具备可供未来子系统共享的 SQLite bootstrap 和事务迁移机制，并完成 `ai_routing_gateway` 首版全量 Schema，且不迁移或修改任何现有数据源。

## Implementation Checklist

- [ ] 创建固定指向 `~/.config/onespace/data/onespace.sqlite3` 的共享数据库初始化、受控连接和事务迁移接口。
- [ ] 启用 WAL、foreign keys 和明确的 busy timeout，创建以 `(subsystem, version)` 唯一标识的 `app_schema_migrations`。
- [ ] 创建计划规定的全部 `ai_gateway_` 表、约束、外键删除策略和查询索引。
- [ ] 创建唯一默认分组及默认设置，端口、全局阈值和保留期分别默认为 `17688`、`10%` 和 90 天。
- [ ] 为后续安全、账号、OAuth、额度、Key、日志和运行时模块提供稳定存储接口及类型边界。

## Acceptance Criteria

- [ ] 全新初始化会创建父目录、数据库、迁移表、完整网关 Schema、默认分组和默认设置。
- [ ] 重复初始化和两个初始化器并发竞争均收敛到同一版本，不重复执行迁移。
- [ ] 模拟迁移中途失败时整版事务回滚、不写版本记录，前一版本仍可使用。
- [ ] WAL、foreign keys 和 busy timeout 在实际连接中生效；不以 `PRAGMA user_version` 作为唯一迁移标识。
- [ ] 默认组唯一，外键删除行为满足业务数据级联删除、历史日志及聚合快照保留规则。
- [ ] 未知表以及 JSON/app_store、Protocol Router 数据均未被读取、写入或迁移。

## Verification Steps

- [ ] 执行共享 SQLite 与网关存储 Rust 测试，确认首建、升级、回滚、重复执行和并发初始化用例通过。
- [ ] 执行 Schema/索引检查测试，确认关键筛选和排序查询使用预期索引。
- [ ] 执行 `cargo check`，确认新增存储边界可编译。

## Out of Scope

不接入 Keychain、不启动 HTTP 服务、不实现领域命令，也不修改现有独立数据库。
