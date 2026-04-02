# AI Smart Assistant Composer Controls Design

## Goal

调整 `AI智能助手` 页面右侧对话区结构，彻底移除右侧顶部重复信息栏，把会话级切换与控制收敛到底部输入区，并把底部控制改造成更精致的三段式聊天工具条。

## Approved Direction

- 右侧主内容顶部的重复信息栏整块移除，不保留标题、消息数、更新时间，也不留占位空白。
- 底部输入区工具行采用三段式布局：
  - 左侧：`助手`、`Provider`、`模型` 三个胶囊选择器
  - 中间：`联网 / MCP / 知识库 / 其他能力状态` 的紧凑状态组
  - 状态组后：一个 `更多` 图标按钮
- `发送` 按钮始终固定在工具条最右侧，不能被其他图标挤到中间。
- `模型` 选择器宽度需要跟随当前选中模型文字动态收缩，不预留多余空白。
- `助手` 与 `Provider` 选择器也统一改成更精致的紧凑胶囊视觉，而不是默认后台表单风格。
- `更多` 按钮点击后带动画弹出第三组动作：`重置 / 置顶 / 归档 / 删除`。
- `Provider` 切换后自动跳到该 `Provider` 的第一个可用模型，并允许用户再手动修改模型。
- 切换 `助手` 时，当前会话切换到该助手预配置的 `Provider + 模型` 组合；若预配置模型不可用，则回退到该 `Provider` 的第一个可用模型。
- 所有新增切换与控制只影响当前会话，不改动全局默认值。

## Scope

本次设计只覆盖 `智能工作台 -> 对话` 子页右侧会话面板的布局与交互。

包含：

- 右侧顶部重复信息栏移除
- 底部三连选新增与视觉重绘
- 会话级 `assistant_id / model_override_id` 联动
- 中间能力状态组与右侧动作组的重新编排
- 模型选择器动态宽度
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

- 删除右侧主内容顶部现有会话信息栏。
- 消息列表直接贴合主面板顶部开始渲染。
- 顶部不再承载任何重复信息与操作按钮。

### Message Area

- 消息列表直接承接主面板顶部。
- 保持现有滚动、自动滚底、消息卡片、工具调用面板与来源面板逻辑不变。

### Composer Area

底部输入区继续保留以下总体结构：

1. 文本输入框
2. 输入框下方工具行

工具行从左到右固定为：

1. 左侧配置组：助手选择器、Provider 选择器、模型选择器
2. 中间状态组：联网、知识库、MCP、工作区读取、笔记检索、记忆等会话状态
3. 状态组后的更多入口：点击后动画弹出动作组
4. 最右侧发送按钮

在窄宽度下允许换行，但顺序保持不变，且发送按钮必须继续保持在最后一列或最后一行最右侧。

### Visual Styling

- 三个选择器统一使用圆角胶囊样式，提升与聊天输入区的一致性。
- 选择器采用更轻的边框、更柔和的背景层次和更清晰的 hover / focus 态，避免原生表单的生硬观感。
- `模型` 选择器宽度按当前选中项文字动态测量，宽度 = 文本宽度 + 内边距 + 下拉箭头预留，不再固定为大宽度。
- 能力状态组使用更轻量的 badge / pill 风格，动作按钮默认收在 `更多` 弹层中，避免底部工具条过长。

## Component Responsibilities

### AiWorkspaceSimple

- 负责移除顶部信息栏后的右侧主布局收缩。
- 负责派生当前会话有效的：
  - 助手
  - Provider
  - 模型
- 负责底部三连选的状态展示、切换保存和失败回滚。
- 负责测量当前模型文本并驱动模型选择器宽度。
- 负责在创建新会话时把底部当前选择写入新会话。

### CapabilityBadges

- 调整为适配底部中间状态组的紧凑工具条样式。
- 保留现有 popover 能力：
  - 知识库明细
  - MCP 明细
- 对联网等可切换能力继续保留按钮交互。
- 为所有状态项补齐 `title`、`aria-label`、`aria-pressed`。

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

## Toolbar Grouping

底部工具条按以下语义分组：

- 左侧配置组：`助手 / Provider / 模型`
- 中间状态组：`知识库 / MCP / 工作区读取 / 笔记检索 / 记忆 / 联网`
- 右侧动作组：`重置上下文 / 置顶 / 归档 / 删除`
- 单独主动作：`发送`

约束：

- 发送按钮始终固定在最右侧
- 删除按钮要与发送按钮保持明确分隔，降低误触风险
- 状态组与动作组之间要有稳定视觉分割，不与配置组混成一排
- 图标语义不得重复复用

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
- `src/i18n.ts`

可能需要补充少量复用函数，但不应引入新的后端命令。

## Testing And Verification

至少需要覆盖以下验证：

1. 右侧顶部重复信息栏已完全移除，消息区直接从主面板顶部开始。
2. 底部三连选已变为胶囊样式，且顺序固定为 `助手 / Provider / 模型`。
3. 模型选择器宽度会跟随当前模型文字动态收缩，无明显多余留白。
4. 底部可切换助手，且会跟随切到助手预配置的 `Provider + 模型`。
5. 切换 `Provider` 会自动选择该 `Provider` 的第一个可用模型。
6. 发送按钮始终保持在工具条最右侧。
7. 状态组与动作组分组清晰，tooltip / popover / disabled 状态正常。
8. 空态、无模型态、保存失败回滚逻辑正常。
9. 跑至少一轮前端校验命令，确认无新增类型或构建错误。

## Success Criteria

- 对话页顶部只保留会话信息，不再出现 `ChatTopBar`。
- 底部输入区成为唯一的会话级切换入口。
- 用户可以在底部直接切换助手、Provider、模型，且联动结果稳定可预期。
- `Provider` 与模型选择逻辑和当前会话保持一致，不污染全局默认值。
- 能力/会话控制完成纯图标化，同时不降低可理解性和可访问性。
