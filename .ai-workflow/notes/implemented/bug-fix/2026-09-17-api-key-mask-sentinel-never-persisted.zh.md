# Agent Note: New Local API Keys Never Persist the Mask Placeholder

Status: implemented

[English](2026-09-17-api-key-mask-sentinel-never-persisted.md) | 中文

## Problem

本地 Key 列表渲染 `maskSecret(key.value)` 而不是已存储的密钥，因此客户端提交的 `value` 字段既可能是真实密钥，也可能是脱敏占位符。任何把占位符按字面落盘的创建路径都会让中继用一个人人皆知的字符串鉴权，并在列表中把它当作该 Key 自身的密钥展示；只有后端能把「未提供值」变成真实熵，因此无论哪个客户端回传掩码，这条保证都必须由后端承担。最初的修复把空白值与完全等于哨兵 `"********"` 都视为「未提供值」；该哨兵契约此后已被彻底移除。

## Decision

仅名称的新建不携带任何密钥材料：前端不再发送掩码，全空白值表示「未提供值」，`ai_gateway_upsert_key` 会把它替换为由后端生成的、携带 128 位 OS 熵的 `sk-gateway-` 密钥（`src-tauri/src/ai_gateway/storage.rs`）。当 id 命中既有 Key 时，空白值保留已存储的密钥，其他任何非空值覆盖它。`ai_gateway_upsert_provider` / `ai_gateway_upsert_key` 中的 `"********"` 哨兵比较与前端 `AI_GATEWAY_KEY_MASK` 常量均已移除（[Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md)）；归一化提交 Key 列表的整配置 `ai_gateway_save_config` 路径同样不复存在，所有写入都经服务商与 Key upsert 完成。前端新建流程位于 `src/components/AiGateway/LocalKeyDialog.tsx`，它只收集名称；`src/components/AiGateway/LocalKeyList.tsx` 只负责 `Add key` 按钮与列表，`src/components/AiGateway/index.tsx` 持有弹框开合状态。

## Alternatives considered

- 按字面存储提交的占位符：未采纳，因为哨兵是公开字符串，存储的密钥会是一个中继接受、界面又当作真实密钥展示的已知值，违反「密钥材料来自后端生成的熵」这一规则。
- 移除前端常量后继续识别完全等于哨兵 `"********"` 的值：未采纳，因为空白值在新建与编辑上都已经表示「未提供值」，而该常量是占位符唯一的生产者；保留比较等于保留一条任何受支持调用方都无法到达的兼容分支。
- 拒绝带空白值的新 Key 并要求调用方提供真实密钥：未采纳，因为新建交互刻意只要求名称，且新建必须在该单一步骤内成功。
- 对每个新 Key 一律生成值并完全忽略提交字段：未采纳，因为命令契约允许显式提供密钥值，丢弃真实值会让有意的调用方写入静默失效。

## Consequences

- 新建本地 Key 只能得到由后端生成的、携带 128 位熵的 `sk-gateway-` 值；字面量 `"********"` 不再被识别，也没有受支持调用方能发送它。
- 编辑既有 Key 的行为保持不变：空白值保留已存储的密钥，显式非空值则覆盖它。原先的完全字符串哨兵比较已不存在，因此仍发送 `"********"` 的客户端会被视为显式提供了值，而不再被当作掩码。
- `MEMORY.md` 的本地 Key 标准现在表述为「新建只需名称、值由后端生成；编辑时留空保留原值」；本次变更已在同一变更中更新该条。
- 回归覆盖位于网关存储测试模块，覆盖不提供值的新建与留空的既有 Key 编辑。
- 部分取代：[Gateway Migration Is Permanent and Version-Gated](../architecture/2026-09-24-version-gated-gateway-migration.md) 只取代上文记录的哨兵比较；「仅名称的新建获得后端生成的熵」与「编辑留空保留已存密钥」的保证仍然成立，[API Fusion Local Key Creation Uses a Name Dialog](../feature/2026-09-17-api-fusion-local-key-name-dialog.md) 仍记录本地 Key 新建流程。
