# Refinery 共享 SQLite 迁移规格

## 规格元数据

- plan-id: `refinery-shared-sqlite`
- status: `approved`
- source_context_id: `onespace-app-refinery-shared-sqlite-20260806`
- source_context_digest: `refinery-shared-sqlite-confirmed-requirements-v1`

## 问题陈述

当前共享 SQLite 的 AI 路由网关迁移由 `shared_sqlite/migrations.rs` 使用
`app_schema_migrations(subsystem, version)` 和即时事务直接执行
`schema_v1.sql` 至 `schema_v4.sql`。需要引入 Rust `refinery` crate，使其成为该
网关 schema 的唯一迁移执行机制，同时保证已有 `onespace.sqlite3` 中 v1-v4 数据可
原地、安全升级。旧迁移历史不能导致已存在对象或数据被破坏性重放。

## 目标与成功标准

- Refinery 是 AI 路由网关 schema 迁移的唯一执行机制与唯一迁移入口。
- 应用启动时先完成共享数据库迁移，成功后才初始化 AI 路由网关。
- 新库由迁移建立完整网关 schema 和默认数据；旧库从任一受支持 v1-v4 状态无损升级。
- 迁移重复执行幂等；默认数据不重复，schema 不报重复创建错误。
- 迁移失败会回滚受控事务内的改动、提供可诊断错误，并阻断网关初始化。
- 数据库路径、连接配置、PRAGMA、忙锁重试及按操作/请求打开连接的行为不变。

## 用户与用户故事

- 作为已有桌面应用用户，我升级应用后希望现有 AI 路由网关配置、密钥和业务数据仍可用。
- 作为新用户，我首次启动时希望自动获得可用的网关 schema 和默认数据。
- 作为维护者，我希望网关 schema 演进只通过一套可追踪的 Refinery 迁移历史执行。
- 作为排障人员，我希望迁移失败能明确指出阶段、数据库状态和底层原因，且应用不会以半初始化网关继续运行。

## 功能需求

1. 在 `src-tauri` 的共享 SQLite 层定义唯一的 AI 路由网关迁移入口；其他模块不得直接执行网关 schema SQL 或另行维护迁移版本。
2. 将既有 `schema_v1.sql` 至 `schema_v4.sql` 组织为由 Refinery 嵌入式迁移加载的迁移集合。具体 Refinery 依赖版本、宏和 API 在实施前按 Cargo 兼容性核验，不在本规格中预设。
3. 该入口必须在打开共享数据库并完成既有连接初始化后运行，使用与当前迁移等价的受控事务边界和锁定语义。
4. 对全新数据库，入口直接运行完整 Refinery 迁移序列，建立完整 schema、索引、约束和默认数据。
5. 对已有数据库，入口必须先读取 `app_schema_migrations` 的网关历史（若存在），并检查实际 schema 是否满足对应版本的已迁移状态；不得仅凭表存在与否推断版本。
6. 旧库兼容/基线桥接必须在同一受控事务中完成：确认历史记录与实际 schema 一致后，向 Refinery 的历史表登记已安全应用的 v1-v4 对应版本，再仅执行尚未应用的后续迁移。
7. 当旧历史、实际 schema 与目标基线不能可靠对应时，必须停止迁移并返回诊断错误；禁止盲目重放 v1-v4 SQL、猜测版本或继续启动网关。
8. 对历史表不存在但实际 schema 已符合某个受支持旧版本的数据库，允许依据经验证的 schema 指纹/必要对象和数据状态执行等价基线登记；判断规则必须可测试且不接受不完整 schema。
9. 对 `app_schema_migrations` 中已声明版本高于支持范围、版本缺口、跨 subsystem 混淆或与实际 schema 不一致的情况，必须失败而非修复性重放。
10. 迁移成功后，网关初始化才可继续；迁移失败时不得创建、初始化或暴露 AI 路由网关运行时能力。
11. `app_store` onboarding 条件和文件迁移流程必须保持原行为，外部 OpenCode SQLite 的读取路径也不得改变。

## 非功能需求

- 共享数据库路径固定为 `$HOME/.config/onespace/data/onespace.sqlite3`。
- 连接继续使用 `READ_WRITE`、`CREATE`、`FULL_MUTEX`，并保持 5 秒 busy timeout、WAL、`foreign_keys=ON` 及 BUSY/LOCKED 重试语义。
- 继续按操作或请求打开连接；不引入连接池、长生命周期句柄或连接所有权模型变更。
- 迁移操作应具备并发启动安全性：同一数据库不会被重复迁移，也不会留下部分 schema 或不一致的历史登记。
- 错误信息需包含迁移阶段（检查、基线、执行或提交）、数据库路径、已识别旧版本/目标版本及底层数据库或 Refinery 原因；不得输出密钥或业务敏感值。

## 范围

包含：

- `shared_sqlite` 的迁移入口与启动调用顺序。
- AI 路由网关 `schema_v1.sql` 至 `schema_v4.sql` 的 Refinery 映射。
- `app_schema_migrations` 至 Refinery 历史的兼容/基线桥接。
- 新库、旧库、幂等、事务回滚、并发迁移及既有路径/PRAGMA 行为的测试。

不包含：

- `app_store` JSON 或加密文件存储 SQLite 化。
- 外部 OpenCode SQLite 数据库及其迁移。
- 连接池、长生命周期数据库句柄。
- AI 路由网关业务 schema 的非必要改造。

## 接口与数据

- 共享 SQLite 层向启动流程提供一个可失败的迁移完成边界；调用方必须等待该边界成功后再初始化网关。
- Refinery 历史表是 AI 路由网关 schema 版本的权威执行记录；`app_schema_migrations` 仅作为旧库识别与一次性桥接输入，不再作为新迁移执行机制。
- 迁移版本与旧 v1-v4 的映射必须一一明确、稳定且有测试覆盖。基线登记只记录经实际 schema 验证的连续版本。
- 默认数据由其所属迁移以幂等语义处理；已有用户数据、密钥和语义不得被覆盖、清空或重置。

## 失败模式

| 情况 | 必需行为 |
| --- | --- |
| 打开数据库、设置 PRAGMA 或获取迁移锁失败 | 返回带路径和底层原因的错误，阻断网关初始化。 |
| 旧历史与实际 schema 不一致 | 返回兼容性/基线诊断错误，不登记历史、不重放旧 SQL。 |
| 历史版本有缺口、超出支持范围或无法识别 | 返回不受支持的数据库状态错误，阻断启动中的网关初始化。 |
| Refinery 执行任一迁移失败 | 回滚该受控事务内的 schema、数据和历史写入，返回迁移版本与底层原因。 |
| 并发启动竞争迁移 | 复用既有 BUSY/LOCKED 重试和事务/锁语义；最终仅一个一致的迁移结果可见。 |
| 网关初始化失败 | 不改变已成功提交的迁移结果；将初始化错误与迁移成功区分报告。 |

## 验收标准

1. 全新数据库启动后，Refinery 建立完整 AI 路由网关 schema 和默认数据。
2. 带有受支持 v1、v2、v3 或 v4 历史与对应实际 schema 的数据库可原地升级至最新版本，业务数据、默认值和密钥语义保持正确。
3. 现有 `app_schema_migrations` 记录会被安全识别并桥接；已应用 v1-v4 不会被破坏性重放。
4. 仅具有可验证旧 schema 而无旧历史表的受支持数据库可按明确基线规则迁移；不完整或矛盾 schema 必须失败。
5. 重复启动和重复调用迁移入口均幂等，不重复默认数据，不出现 schema 或历史冲突。
6. 人为注入的迁移失败会使事务内改动回滚，且网关初始化不发生；错误包含可诊断阶段和原因。
7. 并发 bootstrap/启动测试证明不会重复执行迁移、产生部分 schema 或产生不一致版本历史。
8. 数据库路径、连接标志、busy timeout、WAL、foreign keys 与 BUSY/LOCKED 重试的既有测试继续通过。
9. `app_store` 与外部 OpenCode 的读取路径和现有文件迁移行为不变。

## 兼容性与迁移

升级路径仅覆盖现有共享 `onespace.sqlite3` 中 AI 路由网关受支持的 v1-v4 状态。实施时须为每个旧版本定义可验证的 schema 条件和对应 Refinery 版本；只有历史与 schema 均满足条件时才可在同一事务内建立 Refinery 基线。基线完成后由 Refinery 执行剩余迁移。不得删除旧业务数据，不得迁移或重置外部数据库，且不得依赖用户手动导出/导入。

## 范围外事项

- 将所有应用存储统一迁移至 SQLite。
- 修改 OpenCode 数据库的路径、schema 或生命周期。
- 调整 AI 路由网关的业务模型、功能策略或 API。
- 为数据库访问引入连接池或共享连接。
- 在未验证 Cargo 兼容性前固定 Refinery 的具体版本、宏或调用 API。

## 假设

- Refinery 指 Rust `refinery` crate，并通过 `rusqlite` 集成执行嵌入式迁移。
- 当前 `rusqlite` 0.31 bundled 依赖可在实施阶段与选定 Refinery 集成方案核验兼容性。
- 现有 `app_schema_migrations` 历史需要兼容桥接或基线策略，不能用于触发 v1-v4 的盲目重放。
- `run_app.rs` 当前的 app_store 文件迁移与网关异步初始化流程可在不改变 app_store 语义的前提下调整为先迁移、后初始化网关。

## 开放问题

N/A
