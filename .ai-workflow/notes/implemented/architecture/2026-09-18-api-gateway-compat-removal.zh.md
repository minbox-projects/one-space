# Agent Note: API Gateway Removes Legacy api_fusion File Compatibility

Status: implemented

[English](2026-09-18-api-gateway-compat-removal.md) | 中文

## Problem

此前变更为改名前的 `api_fusion` 存储名增加了文件级读取兼容：`LEGACY_CONFIG_FILE`（`api_fusion.json`）配存储回退读取、`LEGACY_USAGE_DB_FILE`（`api_fusion_usage.db`）配用量日志回退打开，以及与新标记 `api_gateway_gateway` 并行识别的 `LEGACY_GATEWAY_MARKER_KEY`（`api_fusion_gateway`）。该兼容没有决策记录，而用户现已明确要求不兼容：网关必须只使用新名。保留回退会为配置、用量历史与终端同步标记留下无文档的第二事实来源，而删除旧文件不在范围内——它们只是被遗留在原处。

## Decision

所有旧文件级兼容全部移除；网关只读写新名。`LEGACY_CONFIG_FILE` 及其存储回退读取被删除，因此只含 `api_fusion.json` 的目录会加载为默认空配置，旧文件永不被读取、写入或删除。`LEGACY_USAGE_DB_FILE` 及其用量日志回退打开被删除，因此存于 `api_fusion_usage.db` 的用量历史留在旧文件中，网关打开新的 `api_gateway_usage.db`。`LEGACY_GATEWAY_MARKER_KEY` 与双标记识别被删除，因此只带旧 `api_fusion_gateway` 标记的服务商被视为未标记：再次同步绝不复用其 id，也绝不改写它，而是按既有过期台账规则新建带全新 UUID 的独立网关记录。

已移除的旧名及其替换：

| Removed legacy name | Current name |
| --- | --- |
| `api_fusion.json` (`LEGACY_CONFIG_FILE`) | `api_gateway.json` (`CONFIG_FILE`) |
| `api_fusion_usage.db` (`LEGACY_USAGE_DB_FILE`) | `api_gateway_usage.db` (`USAGE_DB_FILE`) |
| `api_fusion_gateway` (`LEGACY_GATEWAY_MARKER_KEY`) | `api_gateway_gateway` (`GATEWAY_MARKER_KEY`) |

历史名说明：下述相关记录中出现的 `api_fusion.json`、`api_fusion_usage.db` 与 `api_fusion_gateway` 标记指的正是这些已移除的旧名；现行事实为上表中的新名。

## Alternatives considered

- 保留只读旧回退：未采纳，因为用户已明确要求不兼容，静默的第二读取来源会让配置、用量与标记状态保持含混。
- 首次读取时把旧文件复制或迁移为新名：未采纳，因为迁移会写入用户并未要求搬移的数据，旧文件应留在磁盘上被遗留，而不是被搬移。
- 丢弃文件回退但继续识别旧 `api_fusion_gateway` 标记：未采纳，因为半截移除的兼容会在终端同步复用判断上保留同样的双源含混。
- 删除被遗留的旧文件：未采纳，因为删除用户数据超出范围；旧文件原样保留且永不被读取。

## Consequences

- 只有旧文件的用户从默认空配置启动；`api_fusion.json` 中的服务商、本地 Key 与 `terminal_syncs` 不会被带过来，旧文件在磁盘上原样保留。
- `api_fusion_usage.db` 中的用量历史对网关不可见；新请求记录进 `api_gateway_usage.db`，旧库永不被打开、写入或删除。
- 只带 `api_fusion_gateway` 标记的服务商被视为未标记的用户记录：终端再次同步按既有过期台账规则新建带全新 UUID 的独立 `API Gateway` 服务商，绝不改写旧记录。
- 部分取代：[API Gateway Usage Stats and Request Logs](2026-09-17-ai-gateway-usage-logs.md)、[Gateway Per-Model Mapping Disable Is an Explicit Exclusion](2026-09-18-gateway-per-mapping-disable.md)、[Gateway Single-Candidate Fast Fail and Standard Error Responses](2026-09-18-gateway-fast-fail-and-standard-errors.md)、[New Local API Keys Never Persist the Mask Placeholder](../bug-fix/2026-09-17-api-key-mask-sentinel-never-persisted.md) 与 [API Fusion Terminal Sync Writes an Independent Gateway Provider](../feature/2026-09-17-api-fusion-terminal-independent-provider.md) 予以保留并交叉链接。本记录只取代旧兼容面——任何读取回退或双标记前提——而它们的决策（带记录时计价与保留的 SQLite 日志、按映射排除、带标准错误信封的单候选快速失败、后端生成的 Key 熵，以及独立终端同步网关服务商）仍然有效；没有任何有效记录被完全取代，也没有记录被删除或重写。
