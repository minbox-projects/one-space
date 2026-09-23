# Agent Note: Rename API Gateway to AI Gateway

Status: implemented

[English](2026-09-23-ai-gateway-rename.md) | 中文

## Problem

网关此前叫 `API Gateway` / `API 网关`，但它实际是 AI 模型转发服务：它位于侧边栏 `AI 能力` 分组中 `AI 终端服务商` 与 `AI 用量统计` 之间，设置位于 `ai-gateway` 分区。通用的 "API gateway" 名称读起来像通用 API 管理而不是 AI 流量；同时 `api-*` 标识符与产品的 `ai-*` 词汇在组件、命令、事件、导航 id、文件名与终端同步服务商记录中并存，导致同一个功能在代码、文档与 agent 上下文中要背两套名字。

## Decision

所有内部标识符与显示名统一改为 `AI Gateway` / `AI 网关`：

- 前端：`src/components/AiGateway/` 与 `src/lib/aiGateway.ts`；`AiGateway*` 组件名、`aiGateway*` 命令封装与 `AI_GATEWAY_*` 常量；事件 `ai-gateway-status-update` 与 `ai-gateway-config-update`；导航 id `ai-gateway` 与 `ai-gateway-backend`；设置分区 `ai-gateway`。
- 后端：`src-tauri/src/ai_gateway.rs` 及其子模块（`types_config`、`storage`、`selection`、`runtime_http`、`forwarding`、`commands`、`usage_log`、`templates`、`migration`）与 `ai_gateway_*` Tauri 命令。
- 持久化与终端集成：`ai_gateway.json`、`ai_gateway_usage.db`、终端同步标记 `ai_gateway_gateway`，以及同步生成的服务商记录名 `AI Gateway`。
- `migrate_legacy_files()` 在 `get_app_dir()` 下把改名前的配置与用量 SQLite 数据库（含 `-wal`/`-shm` 侧车）各自改名为当前名：仅当目标不存在时执行，已存在的当前文件始终优先且旧文件保持不动，因此绝不覆盖、幂等、错误全部忽略。调用点为配置读取、配置写入、`ai_gateway_autostart` 与 `UsageLogStore::default_store`。
- `has_legacy_gateway_marker()` 识别顶层或 `tool_config` 内的改名前沿用的终端标记；新写入一律使用 `ai_gateway_gateway`，下次同步复用旧记录的 id 并原地升级，不会新建重复记录。
- `src-tauri/src/ai_gateway/migration.rs` 是一版生命的临时模块，也是唯一允许出现改名前后两套标识符的地方；其注释声明下个版本删除，删除条件为所有用户完成升级且删除不改变行为。
- 原 `cleanup_legacy_files()` 与 API Fusion 时代的遗留常量已删除：`api_fusion.json` 与 `api_fusion_usage.db` 不再被自动删除，也不会被读取。

## Alternatives considered

- 保留 `API Gateway` 只改用户可见文本：不采用，因为标识符仍与 `AI 能力` 分组和 `ai-gateway` 设置分区不一致，agent 仍要在两套词汇之间切换。
- 不做迁移模块直接改名：不采用，因为既有安装会从空配置开始并看不到用量历史，而且一旦写入新文件就无法再迁移旧文件。
- 对文件与标记保留双名兼容：不采用，因为它保留了此前"旧名兼容移除"刻意消除的、无文档的第二数据源。
- 连历史决策记录一起改名：不采用，因为被引用的记录都是 `implemented/` 下的 active note，而 notes README 要求已交付的决策按历史保留、不做回溯改写；变更后的决策或理据由新 note 记录，本记录正是如此，旧 note 的文件名则作为历史 slug 保留，使既有交叉链接保持可追溯。

## Consequences

- 既有安装通过这一版的原地改名保留配置与用量历史；两种文件名同时存在时当前文件优先，因此部分迁移的目录可无数据损失地收敛。
- 只带旧标记的服务商在迁移版本期间仍按网关记录识别；下次同步原地升级后，该记录只使用当前标记。
- `api_fusion.json` 与 `api_fusion_usage.db` 继续以孤儿文件留在磁盘上：此前的自动删除被刻意撤回，两个文件也不会被读取，因此本次改名不删除任何用户数据。
- 下个版本删除 `migration.rs` 只有在升级窗口结束后才行为中立；在那之前，它始终是"全部 `ai_gateway` 命名"规则的唯一豁免。
- 部分取代：[API Gateway Deletes Legacy api_fusion Files](2026-09-18-api-gateway-legacy-cleanup.md) 保留并交叉链接；本记录只取代其删除行为与清理调用点，而"遗留文件绝不读取"的前提仍然成立。[API Gateway Removes Legacy api_fusion File Compatibility](2026-09-18-api-gateway-compat-removal.md) 保留并交叉链接；其移除读取兼容的决定仍然成立，只有其命名表中的"当前名"一列被本记录取代。网关的技术记录（[API Gateway Usage Stats and Request Logs](2026-09-17-ai-gateway-usage-logs.md)、[API Gateway Smooth Weighted Round Robin Routes Requests and Fallback Candidates](2026-09-20-api-gateway-weighted-routing.md)、[API Gateway Cache Hit Rate Normalizes Provider Usage Semantics](../bug-fix/2026-09-21-api-gateway-cache-hit-accounting.md)）在标识符改名后仍然有效；没有 active note 被完全取代，也没有任何记录被删除或改写。
