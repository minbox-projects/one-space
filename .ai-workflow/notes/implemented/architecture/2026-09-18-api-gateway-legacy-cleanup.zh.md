# Agent Note: API Gateway Deletes Legacy api_fusion Files

Status: implemented

[English](2026-09-18-api-gateway-legacy-cleanup.md) | 中文

## Problem

[API Gateway Removes Legacy api_fusion File Compatibility](2026-09-18-api-gateway-compat-removal.md) 移除了旧读取回退，并记录旧文件永不删除：删除被遗留的用户数据超出范围，因此 `~/.config/onespace` 下的 `api_fusion.json` 与 `api_fusion_usage.db` 被原样保留。用户现已明确要求删除这两个旧文件。保留前提与已交付事实矛盾，双评审要求同一变更内新增记录。

## Decision

`api_gateway` 新增 `cleanup_legacy_files()`，只删除上述两个旧路径，且绝不读取它们。现行文件绝不动，每次删除均为 best-effort，错误忽略。

| Deleted legacy path | Current file left untouched |
| --- | --- |
| `api_fusion.json` | `api_gateway.json` |
| `api_fusion_usage.db` | `api_gateway_usage.db` |

清理在四个点运行：autostart 无条件调用 `cleanup_legacy_files()`，并在 config 写后、usage store 打开后、以及 config 读到新配置后再次调用。

- `autostart` 无条件调用 `cleanup_legacy_files()`。
- config 写后再次运行 `cleanup_legacy_files()`。
- usage store 打开后再次运行 `cleanup_legacy_files()`。
- config 读到新配置后再次运行 `cleanup_legacy_files()`。

仅旧文件场景由无条件的 autostart 调用兜底。

## Alternatives considered

- 继续将被遗留的旧文件原样保留：未采纳，因为用户已明确要求同步删除，此前的保留前提不再成立。
- 删除前把旧文件复制或迁移为新名：未采纳，因为用户要求的是删除而非迁移，读取旧文件会重新引入已被移除的第二事实来源。
- 扩大删除范围、连同现行文件一起删除：未采纳，因为现行文件绝不动，只删除上述两个旧路径。

## Consequences

- 一旦删除，旧配置与用量历史不可逆丢失；旧文件只删不读、不迁移。
- 旧标记不在本次删除范围内：旧标记的识别已在上轮移除，因此这里没有需要删除的标记文件。
- 现行的 `api_gateway.json` 与 `api_gateway_usage.db` 不受清理影响。
- 部分取代：[API Gateway Removes Legacy api_fusion File Compatibility](2026-09-18-api-gateway-compat-removal.md) 予以保留并交叉链接。本记录只取代其中的非删除前提；其独有理由仍然有效，包括移除旧读取回退、拒绝静默迁移，以及把只带旧标记的记录视为未标记处理。
