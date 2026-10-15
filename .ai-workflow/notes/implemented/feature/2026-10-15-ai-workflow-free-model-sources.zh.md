# Agent Note: AI Workflow Model Switcher Adds Discovered Free OpenCode Models to Column Candidates

Status: implemented

[English](2026-10-15-ai-workflow-free-model-sources.md) | 中文

## Problem

`ai_workflow_get_model_sources` 过去只从 `~/.config/opencode/opencode.json` 中 `provider.<name>.models` 显式声明的模型构建 OpenCode 列候选。因此操作者必须把想切换的每个模型都声明出来，即便 OpenCode 已经在本地发布其当前模型目录，OpenCode 发现并刷新的免费模型对切换器仍然不可见，除非用户逐条把 id 复制进 `opencode.json`。OpenCode 的能力推导本已读取该本地目录的 `reasoning_options`，但候选列表本身从未使用它。

## Decision

OpenCode 列候选 = 显式声明模型与动态发现的当前免费模型的并集，并去重、排序。

- 动态来源为 `~/.cache/opencode/models.json` 中的 `opencode` 与 `opencode-go` 两个 provider。
- 目录模型携带 `cost` 对象且其 `input` 与 `output` 均为 `0`、`status` 不为 `"deprecated"` 时判定为免费。
- 动态候选的能力档位经 `opencode_catalog_capability` 从目录 `reasoning_options` 中 `type: "effort"` 的条目解析，复用既有能力回退所读取的同一目录。
- 显式声明模型的 `variants` 能力优先，绝不被目录派生的能力覆盖。
- 目录缺失时静默退化为仅声明候选：无动态条目、无新错误或新警告。目录存在但不可读或损坏时沿用既有 `capability_warning` 行为。
- 不硬编码任何模型 id，也不发起网络请求；切换器只读取本地 OpenCode 快照。

## Alternatives considered

- 硬编码一份精选免费模型 id 清单：拒绝，因为目录会变化，快照清单会过期，并带来与零维护目标相矛盾的维护成本。
- 从网络拉取模型目录：拒绝，因为会增加延迟与网络依赖、使切换器离线不可用，且与既有的仅本地理由一致。
- 用名称启发式（例如 id 片段）判定免费：拒绝，因为名称不能证明计费，而已声明的 `cost` 值才是权威依据。
- 保持候选仅限声明模型、要求用户手动声明免费模型：拒绝，因为 OpenCode 已维护能力路径所读取的本地目录，要求手动重复违背了该发现的意义。
- 目录缺失时抛出新的错误：拒绝，因为本地目录缺失是正常状态，静默的仅声明结果能让该列保持可用。

## Consequences

- OpenCode 列同时提供显式声明模型与当前免费目录模型，去重并排序，已声明条目始终可用。
- 声明模型的显式能力权威，绝不被目录派生的 effort 列表替换。
- 目录数据缺失时静默；目录数据损坏时沿用既有 `capability_warning`。
- 免费清单是一份动态本地快照：随 OpenCode 目录刷新而变化，因此模型可能在两次读取之间出现或消失，且不承诺任何模型 id 保持免费。
- `.ai-workflow/index/navigation.json` 的 `ai-workflow-model-switcher` 与 `ai-workflow-backend` 条目、生成的 `.ai-workflow/index/navigation.md` 与 `MEMORY.md` 中的 AI Workflow 标准记录交付契约。
- 部分取代：本记录部分取代 [AI Workflow Model Switcher Delivers 9-by-3 Matrix with Backend Profile Commands](2026-09-22-ai-workflow-model-switcher.md) 的 OpenCode 候选来源枚举，该记录保留并互链；其 9×3 矩阵、逐列降级、profile 管理与 Save/Activate 理由仍然成立。[AI Workflow Model Switcher Derives Reasoning Efforts from Declared Local Model Capabilities](../bug-fix/2026-10-15-ai-workflow-model-reasoning-efforts.md) 记录能力推导，保持不变。
