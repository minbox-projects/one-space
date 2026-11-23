# Agent Note: CommandCode Plan Templates Split Sync, Quota and Migration by Official Subscription Plan

Status: implemented

[English](2026-11-23-commandcode-plan-templates.md) | 中文

## Problem

- 单一内置 `commandcode` 服务商模板会同步整份官方模型清单，于是每位 CommandCode 订阅者都能看到其套餐无法服务的模型；普通清单携带成员、名称与端点声明，而只有官方客户端目录声明每个模型的 `minimumPlan` 订阅档位。
- CommandCode 出售四个递增套餐，其可用模型集合是累积的，且官方目录持续变化，因此套餐限定必须保留该档允许的普通条目，而不是发布一份会偏离官方目录的手工逐套餐模型清单。
- 既有安装把服务商与持久化模板状态绑定在遗留 `commandcode` 身份上，其模型集合等同于整份目录，因此拆分必须移除这个重复的内置模板，同时保留操作者维护的每个字段。

## Decision

- 内置快照在 OpenCode 模板之前按升序发布四个 CommandCode 套餐模板：`commandcode-go`、`commandcode-goat`、`commandcode-pro` 与 `commandcode-max`，它们共用 `base_url` `https://api.commandcode.ai/provider/v1`、`protocol` `chat_completions`、`sync_strategy` `generic`、`quota_strategy` `commandcode`、`error_strategy` `generic`、`source` 与 `models_url` `https://api.commandcode.ai/provider/v1/models`、`icon` `commandcode` 以及空的 `models` 列表；遗留的内置 `commandcode` 已移除。
- 套餐门控位于 `src-tauri/src/ai_gateway/templates/source.rs`：这四个 id 之一且 trim 后 `models_url` 恰为规范 URL 时，经同一不带凭据的 15 秒客户端先取普通清单、再取 `https://api.commandcode.ai/provider/v1/models?view=client`，其他任何 URL 或模板 id 只按配置抓取一次。
- 客户端目录与每个条目的 `minimumPlan` 在任何发布之前校验：缺省、`null` 与字符串声明被接受，trim 后空白字符串视为无套餐，其他形态使整次同步失败。
- 档位对普通条目累积：Go 保留 `individual-go`，GOAT 保留 Go 与 `individual-goat`，Pro 保留 GOAT 与 `individual-pro`，Max 保留全部普通条目。
- 对 Go、GOAT 与 Pro，普通清单中缺少该条目或声明缺失、空白、null、未知或更高档位的条目被丢弃，仅客户端目录存在的条目绝不新增模型，过滤后为空即失败且不落盘。
- 精确 id 的档位合并、五值推理强度过滤、价格合并与 GOAT 文档页 `contextTiers` 回退保持既有行为。
- `GATEWAY_CONFIG_SCHEMA_VERSION` 为 10。版本门控迁移把 `template_id` 恰为 `commandcode` 的服务商与已持久化模板状态重绑定为 `commandcode-max`，保留每个用户维护字段并只把默认模板名 `CommandCode` 改为 `CommandCode Max`；`commandcode-max` 已存在时丢弃遗留状态，从 `deleted_template_ids` 移除 `commandcode` 且不新增任何套餐 id，仅按 URL 或绑定其他模板的服务商绝不迁移，第二次 schema 10 读取字节稳定，未来版本不降级，改写失败保留旧的加密字节。
- 额度监控、批量刷新与图标解析对四个 id 继续按 `commandcode` 配额策略与图标工作。

## Alternatives considered

- 为每个套餐发布静态模型清单：未采纳，因为官方目录会随档位变化，发布的清单会偏离端点并错报套餐可服务的范围。
- 在套餐模板之外保留遗留 `commandcode`：未采纳，因为它的模型集合等同于 Max 套餐，第二个具有该集合的内置模板会与其中一个重复。
- 新增四个 `SyncStrategy` 变体：未采纳，因为它会扩展持久化枚举与界面表面，相对四个共用同一同步策略的内置模板没有额外价值。

## Consequences

- 交付代码为 `src-tauri/src/ai_gateway/provider_templates.json`、承载套餐档位、客户端目录门控与过滤的 `src-tauri/src/ai_gateway/templates/source.rs`、承载 schema 10 重绑定的 `src-tauri/src/ai_gateway/migration.rs` 与承载 `GATEWAY_CONFIG_SCHEMA_VERSION` 的 `src-tauri/src/ai_gateway/types_config/config.rs`。
- 现在 CommandCode 套餐模板只同步其订阅可服务的模型，而 `commandcode-max` 保留未过滤的目录；既有 `commandcode` 绑定在下一次读取时变为 `commandcode-max`，无需重置模板或建立新的套餐绑定。
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::tests::templates` 通过 99 例，`...--lib ai_gateway::quota` 通过 2 例，`...--lib ai_gateway::tests::quota` 通过 39 例，`...--lib ai_gateway::tests::go_usage` 通过 26 例，`...--lib ai_gateway::tests::context_terminal_sync` 通过 23 例，`...--lib ai_gateway::tests::migration` 通过 45 例；三个前端 smoke 文件在禁用 Node 实验性 web-storage 全局的情况下通过 222 个测试，更广泛的 `--lib ai_gateway` 运行通过 1229 例，另有两条既有失败（`opencode_zen_sidecar_transport_resolution_red`，一条过期的 Zen base URL 断言，以及一个依赖网络的 blackhole 超时）在未改动的基线提交上同样复现。
- `MEMORY.md` 在同一变更中记录四个套餐模板及其规范字段、带 fail-closed 边界的累积 `minimumPlan` 过滤与 schema 10 重绑定。
- 取代关系：部分取代。本记录就内置目录成员部分取代 [Provider Templates Own Closed Strategies and Bind Providers by Template Id](../architecture/2026-10-10-provider-template-bound-strategies.md)，就客户端目录门控身份部分取代 [Provider Template Reasoning Capability Sync](2026-10-16-provider-template-reasoning-sync.md)，并就当前 schema 版本部分取代 [Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md)，而它们的封闭策略字段、绑定校验、精确 id 合并语义、缺失与清空语义以及迁移永久性继续有效；[CommandCode Provider Cards Show Account Quota](2026-09-23-commandcode-provider-quota.md) 与 [OpenCode Go Provider Cards Show Usage](2026-09-24-opencode-go-provider-usage.md) 保留且无取代关系，因为其来源、守卫与缓存未变。
- 重新引入条件：套餐限定建立在已发布的 `minimumPlan` 声明之上，因此被移除或改名的声明绝不能从模型名推断，而无法访问客户端目录的套餐模板必须失败，绝不回退到未过滤清单。
