# Agent Note: AI Workflow Model Switcher Derives Reasoning Efforts from Declared Local Model Capabilities

Status: implemented

[English](2026-10-15-ai-workflow-model-reasoning-efforts.md) | 中文

## Problem

AI Workflow 模型切换器对 9×3 角色/宿主矩阵的每个单元格都套用同一份固定的六值推理强度列表（`low`、`medium`、`high`、`xhigh`、`max`、`ultra`）。因此它会呈现宿主或模型并不支持的档位，而 `ai_workflow_save_profile` 只校验 profile schema，可能把 schema 合法但模型并不支持的 effort 持久化。切换器既没有逐模型的本地能力感知，也没有为无法在本地确定能力的模型提供显式手动入口，也无法提示发现的取值超出了 profile schema。上游 AI Workflow profile schema 保持既有的六值 effort 枚举，因此超出该枚举的发现值不能写入 profile。

## Decision

切换器从显式声明的本地模型能力推导推理强度，同时以六值 profile schema 作为可写边界。

后端模型源发现（`ai_workflow_get_model_sources`）新增一个按精确 host 本地模型 id 键控的可加性 `model_capabilities` 映射，以及一个与列 `error` 分离的独立 `capability_warning`。公开 `ModelCapability` 类型携带 `efforts: Vec<String>` 与可选 `default_effort`；省略的映射或条目表示该精确模型的能力未知，显式空条目表示已知该能力没有可写档位，空的 `model_capabilities` 映射因序列化省略而不出现。

- Codex 读取 `~/.codex/models_cache.json`，匹配 `models[].slug` 并收集非空 `supported_reasoning_levels[].effort` 与声明的 `default_reasoning_level`。slug 缺失或 effort 元数据为空即为未知。
- OpenCode 读取 `~/.config/opencode/opencode.json` 中显式声明的模型 `variants`（其值声明字符串 `reasoningEffort`），排除 `disabled: true`。读取的是 effort 取值而非 variant 名称，且显式声明优先于缓存目录，包括全部已声明 effort variant 都被禁用时（得到已知空列表，绝不回退缓存）。没有此类声明时，`~/.cache/opencode/models.json` 中精确 `provider/model` 匹配可提供 `type: "effort"` 且 `values` 为字符串的 `reasoning_options` 条目。含斜杠的完整复合 id 予以保留。
- Claude 与任何没有显式适用来源的自定义模型为未知。标量配置的 effort 是所选默认值，不是能力列表。
- 缺失文件属正常情况；解析或读取失败把该来源降级为未知并附带 `capability_warning`，同时保持模型候选与健康列可用，且绝不暴露配置内容或凭据。

类型化前端领域库 `src/lib/aiWorkflowProfiles.ts` 导出 `ModelCapability`、`ColumnModelSource`、`isKnownEffort`、`getModelCapability`、`resolveWritableEfforts`、`resolveExcludedEfforts` 与 `normalizeEffortForModel`。

- 已知模型只提供声明列表与 profile 枚举的交集，按 profile 枚举顺序去重（`resolveWritableEfforts`）。被 schema 排除的发现值（如 `none` 或 `minimal`）以 schema 限制说明展示（`resolveExcludedEfforts`）。交集为空时禁用 effort 控件、说明不兼容并让该已填单元格在 Save 前无效。
- 未知模型使用带可见未知能力提示的手动输入，并按六个 profile 取值校验（`isKnownEffort`）。
- 每条模型编辑路径——单元格模型选择、手动输入模型、整行填充与整列填充——共用 `normalizeEffortForModel`：保留当前可写值，否则取声明默认值，再取可写的 `medium`，再取首个可写值。未知模型保留已输入文本，新填的未知单元格以 `medium` 起步但绝不宣称支持。已知但没有可写值的模型没有自动默认值并保持无效。
- 加载保留存储的矩阵与 dirty baseline（含无效值）；只要任一已填单元格无效，Save 即被阻止。
- 批量推理强度的目标是当前已填且已知的单元格，菜单提供每个目标可写集合的交集。未知单元格被跳过并显示数量，缺失单元格绝不创建，交集不相交、已知空集或无目标时禁用 Apply 并给出说明。模型或方案变化会按当前目标与交集重算并清除过期选择，因此被取代的 effort 无法被应用。

后端 Save（`ai_workflow_save_profile`）保留既有六值 schema 校验，并在其通过之后、原子写入之前，按同一 home 解析独立重新读取本地能力，拒绝已知模型中声明不支持的 effort（即便该值 schema 合法），返回点名 role、host 与 model 的错误。profile 与 active-profile 配置均保持不变，未知模型的 schema 合法手动值仍可保存。Save 与激活保持分离。

所有新增提示与校验消息在 `src/i18n.ts` 中以英文与中文提供，并有匹配的 `en_keys.txt` 与 `zh_keys.txt` 条目：`aiWorkflow.effortSchemaExcluded`、`aiWorkflow.effortIncompatibility`、`aiWorkflow.effortUnknownHint`、`aiWorkflow.effortInvalid`、`aiWorkflow.batchEffortSkippedUnknown`、`aiWorkflow.batchEffortNoCommon` 与 `aiWorkflow.batchEffortNoTargets`。

交付行为由 `src-tauri/src/ai_workflow_profiles/tests.rs` 中基于 fixture 的公开 Rust 模型源与 Save 测试覆盖（`cargo test --manifest-path src-tauri/Cargo.toml --lib ai_workflow_profiles`，提交 `9a1f6fdf7ece5977b79502cfaebc65d268e7d957`，退出 0，35 例通过，含 26 例保留用例），并由 React Testing Library 的编辑器、模型变更、批量与 Save 交互覆盖（`npm test -- src/components/AiWorkflowModelSwitcher/AiWorkflowModelSwitcher.test.tsx src/lib/aiWorkflowProfiles.test.ts src/i18n.test.ts`，提交 `faa50667fcc6f7e329196028976112cebdabb5fa`，退出 0，151 例通过）。`npm run build` 与限定范围的 `npx eslint` 命令退出 0。未进行任何实时服务商探测、网络目录刷新或 CLI 安装。

## Alternatives considered

- 保留固定六值选择器而不具备本地能力感知：未采纳，因为它会呈现宿主或模型并不支持的档位，并可能持久化 schema 合法但不被支持的 effort；要求的行为是已知模型只提供受支持取值，且后端在写入前拒绝已知不支持的 effort。
- 从模型名子串启发式推断支持：未采纳，因为相似名称并不能证明支持，且 `provider-a/deepseek/r1` 与 `provider-b/deepseek/r1` 是两个独立身份。
- 实时服务商探测、网络目录刷新或 CLI 安装：未采纳，因为它们引入延迟、网络依赖与凭据暴露风险；读取本地声明快照让该工具保持离线并完全不触碰凭据。
- 扩展上游 AI Workflow profile schema，例如新增 `none` 或 `minimal`：未采纳，因为 profile 格式保持版本 `1.0.0` 与既有六个取值；发现的非枚举值以 schema 排除形式呈现，绝不写入 profile YAML。
- 新增能力覆盖设置、依赖、迁移或通用模型目录服务：未采纳，对于本地声明快照而言没有必要。

## Consequences

- 已知模型只提供声明的可写档位并解释被 schema 排除的值；未知模型使用显式手动输入；已知但没有可写值的模型可见地无效并被阻止 Save。
- 后端仍是 profile YAML 的唯一权威：它独立于前端快照重新读取当前本地声明，并以原子方式拒绝已知不支持的 effort，使 profile 与 active-profile 配置保持不变。因此编辑与 Save 之间发生声明变化会返回拒绝，而不会持久化无效值。
- profile CRUD、Save/Activate 分离、已安装模型快照、服务商投影与模型名候选语义均不变；IPC 变更是可加性的，省略的能力映射被视为未知，profile YAML 保持 `1.0.0`。
- 本地缓存与配置声明是快照，不承诺实时新鲜度，且未知模型上的手动值不构成该模型确实支持它的证据。
- `.ai-workflow/index/navigation.json` 中的 `ai-workflow-model-switcher` 与 `ai-workflow-backend` feature 条目、生成的 `.ai-workflow/index/navigation.md` 以及 `MEMORY.md` 中的 AI Workflow 领域标准记录该已交付的能力契约。
- 部分取代：本记录部分取代 [AI Workflow Model Switcher Delivers 9-by-3 Matrix with Backend Profile Commands](../feature/2026-09-22-ai-workflow-model-switcher.md) 的静态六值推理强度选择器决策，该记录予以保留并交叉链接；其 9×3 矩阵、模型源、方案管理与 Save/Activate 理由仍然成立。[AI Workflow Profile Save and Activation Are Separate Actions](../feature/2026-09-23-ai-workflow-profile-save-activation.md) 记录独立的 Save/Activate 决策且保持不变。
