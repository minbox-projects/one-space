# AI Smart Assistant Composer Controls Design

## Goal

调整 `AI智能助手` 页面右侧对话区结构，把会话级切换与控制收敛到底部输入区，减少顶部操作噪音，并补齐 `Provider` 与模型联动切换能力。

## Approved Direction

- 移除 `ChatTopBar`，不保留其中任何全局操作。
- 顶部仅保留现有会话信息栏，继续显示主题标题、消息数、更新时间。
- 底部输入区工具行最左侧新增三个会话级选择器：`助手`、`Provider`、`模型`。
- `Provider` 切换后自动跳到该 `Provider` 的第一个可用模型，并允许用户再手动修改模型。
- 切换 `助手` 时，当前会话切换到该助手预配置的 `Provider + 模型` 组合；若预配置模型不可用，则回退到该 `Provider` 的第一个可用模型。
- 底部现有能力/会话控制保留，但全部改成纯图标按钮，不再显示文字描述。
- 图标语义必须去重，避免不同能力共用同一个图标。
- 所有新增切换与控制只影响当前会话，不改动全局默认值。

## Scope

本次设计只覆盖 `智能工作台 -> 对话` 子页右侧会话面板的布局与交互。

包含：

- `ChatTopBar` 移除与顶部信息栏保留
- 底部三连选新增
- 会话级 `assistant_id / model_override_id` 联动
- 现有能力/会话控制改为纯图标入口
- 空态、异常态、无可用模型时的回退与禁用逻辑

不包含：

- `助手库`、`自动化`、`模型中心` 页面改版
- 新增后端接口或新的持久化字段
- 修改全局快捷助手窗口行为
- 调整左侧历史会话面板结构

## Current Context

当前 `AiWorkspaceSimple` 已具备以下基础能力：

- 会话级 `assistant_id`
- 会话级 `model_override_id`
- 启动发送时显式传入 `assistant_id / model_override_id`
- 从 bootstrap 数据中读取 `settings.providers` 与 `settings.model_catalog`

因此本次改版优先复用现有数据结构与命令，不额外扩展后端协议；`Provider` 选择仅作为前端派生状态存在，真实写回的仍是 `model_override_id`。

## Layout Changes

### Top Area

- 删除 `ChatTopBar` 组件引用与渲染。
- 保留现有会话信息栏：
  - 主题标题
  - 消息数
  - 更新时间
- 顶部信息栏不再承载任何全局操作按钮。

### Message Area

- 消息列表直接承接在顶部信息栏下方。
- 保持现有滚动、自动滚底、消息卡片、工具调用面板与来源面板逻辑不变。

### Composer Area

底部输入区继续保留以下总体结构：

1. 文本输入框
2. 输入框下方工具行

工具行从左到右固定为：

1. 助手选择器
2. Provider 选择器
3. 模型选择器
4. 能力/会话控制图标组
5. 发送按钮

在窄宽度下允许换行，但顺序保持不变，优先保证三个选择器始终位于最左侧。

## Component Responsibilities

### AiWorkspaceSimple

- 移除对 `ChatTopBar` 的依赖。
- 负责派生当前会话有效的：
  - 助手
  - Provider
  - 模型
- 负责底部三连选的状态展示、切换保存和失败回滚。
- 负责在创建新会话时把底部当前选择写入新会话。

### CapabilityBadges

- 从“带短文本的徽章按钮”改成“纯图标按钮组”。
- 保留现有 popover 能力：
  - 知识库明细
  - MCP 明细
- 为所有图标补齐 `title`、`aria-label`、`aria-pressed`。
- 对无弹层但可切换的能力继续复用按钮入口。

### ChatTopBar

- 不再在 `AiWorkspaceSimple` 中使用。
- 若该组件仍被其他页面引用，可保留文件；若仅此处使用，可一并删除。

## Conversation-Level Data Model

### Persisted Fields

会话级持久化继续只使用已有字段：

- `assistant_id`
- `model_override_id`
- `web_search_enabled`

### Derived UI State

前端新增以下派生状态，不新增持久化字段：

- `selectedProviderId`
- `availableProviders`
- `availableModelsForProvider`
- `draftConversationAssistantId`
- `draftConversationModelId`

其中 `Provider` 由当前生效模型所属 `provider_id` 反推得出。

`availableProviders` 必须基于“存在至少一个启用模型”的 `Provider` 计算，而不是简单遍历全部 `settings.providers`。

## Selection Resolution Rules

### Effective Model Resolution

用于渲染当前底部选择器的模型值按以下优先级解析：

1. `selectedConversation.model_override_id`
2. 当前助手的 `primary_model_id`
3. `chat` 角色绑定模型
4. 第一个启用模型

### Effective Provider Resolution

当前显示的 `Provider` 通过当前生效模型对应的 `provider_id` 解析。

若模型为空或无效，则根据回退后的模型继续解析 `Provider`，不允许界面显示“Provider 为空但模型有默认值”的不一致状态。

## Interaction Rules

### Switch Assistant

- 只影响当前会话。
- 更新当前会话的 `assistant_id`。
- 模型同时切换到该助手预配置的模型，优先取 `primary_model_id`，若为空则回退到 `light_model_id`。
- 若该模型不可用：
  - 优先回退到该模型所属 `Provider` 的第一个可用模型
  - 若该 `Provider` 没有可用模型，再回退到系统首个可用模型
- 助手切换后同步刷新该助手对应的能力快照与默认联网策略。

### Switch Provider

- 只影响当前会话。
- `Provider` 本身不直接写库。
- 切换后立即把当前会话 `model_override_id` 改为该 `Provider` 的第一个可用模型。
- 若该 `Provider` 没有可用模型，则该选项不应出现在下拉中。

### Switch Model

- 只影响当前会话。
- 直接更新当前会话 `model_override_id`。
- 不改动当前会话的 `assistant_id` 与其他能力开关。

### Create Conversation With No Active Topic

- 当用户还未选中任何会话但已在底部选择了 `助手 / Provider / 模型` 时，这组选择视为“当前待创建会话配置”。
- 首次发送消息时，用当前底部选择创建新会话。
- 新建会话后，底部选择器应与新会话实际保存值保持一致。
- 当用户切换到一个已有会话时，底部选择器必须立即切回该会话自己的保存值，不得继续显示或复用之前的草稿选择。

## Icon-Only Controls

底部原有能力/会话控制保留，但统一调整为纯图标入口。

覆盖范围：

- 知识库
- MCP
- 工作区读取
- 笔记检索
- 记忆
- 联网
- 重置上下文
- 置顶
- 归档
- 删除

约束：

- 不保留任何常驻文字标签
- 保留 tooltip / popover / disabled 状态
- 不出现重复图标语义

建议图标语义：

- 知识库：`BookOpen`
- MCP：`Blocks` 或 `Database`
- 工作区读取：`FolderSearch`
- 笔记检索：`NotebookPen`
- 记忆：`Brain` 或 `MemoryStick`
- 联网：`Globe`
- 重置上下文：`RotateCcw`
- 置顶：`Pin`
- 归档：`Archive`
- 删除：`Trash2`

最终实现可按项目图标集可用性调整，但必须满足“不同功能不复用同一图标”的要求。

## Error Handling

### Save Failure

- 切换助手 / Provider / 模型时若保存失败：
  - 恢复到上一个有效选择
  - 继续使用现有错误展示区域反馈失败原因

### Invalid Assistant Model

- 助手预配置模型不存在或已禁用时，自动回退到该 `Provider` 第一个可用模型。
- 若无法从预配置模型推导 `Provider`，则回退到系统首个可用模型。

### No Available Models

- 若系统没有任何可用模型：
  - `Provider` 与 `模型` 选择器禁用
  - 输入框与发送按钮禁用
  - 在底部错误区域提示用户前往模型中心配置可用模型

## Implementation Touchpoints

预期主要涉及：

- `src/components/AiWorkspace/AiWorkspaceSimple.tsx`
- `src/components/AiWorkspace/CapabilityBadges.tsx`
- `src/components/AiWorkspace/ChatTopBar.tsx`
- `src/i18n.ts`

可能需要补充少量复用函数，但不应引入新的后端命令。

## Testing And Verification

至少需要覆盖以下验证：

1. `ChatTopBar` 已从对话页移除，页面无多余顶部操作区。
2. 顶部信息栏仍正确显示主题标题、消息数、更新时间。
3. 底部可切换助手，且会跟随切到助手预配置的 `Provider + 模型`。
4. 切换 `Provider` 会自动选择该 `Provider` 的第一个可用模型。
5. 切换模型后发送消息时使用新的 `model_override_id`。
6. 纯图标能力/会话控制仍可点击、悬浮提示正常、弹层不丢失。
7. 不同功能的图标无重复复用。
8. 空态、无模型态、保存失败回滚逻辑正常。
9. 跑至少一轮前端校验命令，确认无新增类型或构建错误。

## Success Criteria

- 对话页顶部只保留会话信息，不再出现 `ChatTopBar`。
- 底部输入区成为唯一的会话级切换入口。
- 用户可以在底部直接切换助手、Provider、模型，且联动结果稳定可预期。
- `Provider` 与模型选择逻辑和当前会话保持一致，不污染全局默认值。
- 能力/会话控制完成纯图标化，同时不降低可理解性和可访问性。
