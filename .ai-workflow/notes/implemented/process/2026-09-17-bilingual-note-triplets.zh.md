# Agent Note: Bilingual Note Triplets

Status: implemented

[English](2026-09-17-bilingual-note-triplets.md) | 中文

## Problem

Agent 决策记录实际上是单语言的，中英文贡献者无法共享同一份权威记录。升级后的 `.ai-workflow/notes/README.md` 要求每条记录由英文正文加中文正文加 `.i18n.yaml` 一致性记录组成等权同体，但 `MEMORY.md` 仍缺少该标准，导致工作流约束与笔记治理不一致。

## Decision

为每条记录采用双语三件套：英文正文 `YYYY-MM-DD-topic-title.md` 与中文正文 `YYYY-MM-DD-topic-title.zh.md` 具有同等效力且表述一致，仅翻译自然语言正文，结构元素（`# Agent Note:` 前缀、标题、`Status` 取值、字段名、路径、日期）保持英文。双方一致后仅通过 `ai-workflow notes pairing --write` 记录一致性。`MEMORY.md` 已在同一变更内对齐，使现行标准与 README 一致，本记录说明该决策的理由。

## Alternatives considered

- 保持单语言记录：未采纳，因为这会延续读者割裂，且与升级后的 README 矛盾，而 README 不设历史格式豁免。
- 机器翻译全部历史生成三件套：未采纳，因为当前记录树为空，不存在需要迁移的历史，批量翻译只会产生未经评审的记录，而非针对当前决策的一份已评审三件套。

## Consequences

- 每次非机械变更都在同一变更内维护完整三件套；`MEMORY.md` 记录现行标准，笔记记录原因。
- 以 `ai-workflow notes validate` 与 `ai-workflow notes pairing --list` 作为一致性门禁：缺失对应件或哈希过期都会导致校验失败。
- 起始记录树为空，因此无需迁移或取代；`notes list` 取代检查未返回相关有效记录。
