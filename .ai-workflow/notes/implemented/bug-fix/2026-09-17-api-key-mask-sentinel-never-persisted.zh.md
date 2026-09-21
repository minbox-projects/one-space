# Agent Note: New Local API Keys Never Persist the Mask Placeholder

Status: implemented

[English](2026-09-17-api-key-mask-sentinel-never-persisted.md) | 中文

## Problem

本地 Key 列表渲染 `maskSecret(key.value)` 而不是已存储的密钥，前端用 `API_FUSION_KEY_MASK`（`"********"`，`src/lib/apiFusion.ts`）作为表示「保留已存储的值」的哨兵。由于该哨兵与真实密钥共用同一个 `value` 字段，任何可创建 Key 的写入路径都可能把它按字面落盘：此前通过 `api_fusion_save_config` 以 `value: "********"` 或 `value: ""` 新建的 Key 会按提交值原样存储，于是中继会用一个人人皆知的占位符进行鉴权，Key 列表也会把它当作自身的密钥展示。对新建 Key 而言，空值与占位符都表示「未提供值」，而只有后端能把它变成真实熵，因此无论哪个客户端回传脱敏占位符，这条保证都必须由后端承担。

## Decision

两条可新建本地 Key 的写入路径都会把「全空白值」或「与哨兵 `"********"` 完全相等」视为「未提供值」，并替换为 `new_key_value()`——`sk-fusion-` 前缀加 128 位 `uuid::Uuid::new_v4().simple()` 十六进制后缀（`src-tauri/src/api_fusion/storage.rs`）。`api_fusion_upsert_key` 在其新建分支（id 为空或未知）套用该规则；当 id 命中既有 Key 时，空白或哨兵值保留已存储的值，其他任何非空值则覆盖它。`api_fusion_save_config` 以同样方式归一提交的 Key 列表：id 命中则保留已存储的值，未命中（新建）且值为空白或哨兵的则生成随机密钥。前端新建流程位于 `src/components/ApiFusion/LocalKeyDialog.tsx`，它只收集名称并提交 `value: ""`；`src/components/ApiFusion/LocalKeyList.tsx` 只负责 `Add key` 按钮与列表，`src/components/ApiFusion/index.tsx` 持有弹框开合状态。`API_FUSION_KEY_MASK` 仍是前端针对既有记录可发送的「保留已存储值」信号。

## Alternatives considered

- 按字面存储提交的占位符：未采纳，因为哨兵是公开字符串，存储的密钥会是一个中继接受、界面又当作真实密钥展示的已知值，违反「密钥材料来自后端生成的熵」这一规则。
- 让前端发送空值、后端完全不识别哨兵：未采纳，因为任何调用方都可能回传该脱敏占位符，只有后端写入路径能保证该字面量不进入 `api_fusion.json`。
- 拒绝带空白或哨兵值的新 Key 并要求调用方提供真实密钥：未采纳，因为新建交互刻意只要求名称，且新建必须在该单一步骤内成功。
- 对每个新 Key 一律生成值并完全忽略提交字段：未采纳，因为命令契约允许显式提供密钥值，丢弃真实值会让有意的调用方写入静默失效。

## Consequences

- 新建本地 Key 只能得到携带 128 位熵的 `sk-fusion-` 值；字面量 `********` 无法再通过 `api_fusion_upsert_key` 或 `api_fusion_save_config` 成为存储的密钥。
- 编辑既有 Key 的行为不变：空白或哨兵值保留已存储的密钥，显式非空值则覆盖它。哨兵仅在空白检查之外做完全字符串相等比较，因此带额外空白的哨兵会被视为显式提供的值而非脱敏占位符。
- `MEMORY.md` 的本地 Key 标准（新建本地 Key 只需名称，值由后端生成；编辑时留空或回传脱敏占位符保留原值）已描述该行为，因此无需改动 `MEMORY.md` 或导航；本记录说明该防护存在的原因与执行位置。
- 回归覆盖为 `api_fusion::tests::new_keys_with_mask_placeholder_get_a_random_secret` 与 `api_fusion::tests::save_config_generates_secret_for_new_keys_with_blank_or_masked_value`，两者均通过。
- 未取代任何有效记录：`notes list` 只返回终端独立服务商记录（终端同步）、双语三件套记录（记录格式）与 [API Fusion Local Key Creation Uses a Name Dialog](../feature/2026-09-17-api-fusion-local-key-name-dialog.md)（本地 Key 新建流程）；其中名称弹框记录同样涉及本地 Key 新建的名称收集，但两条记录陈述不同决策，彼此没有取代关系。
