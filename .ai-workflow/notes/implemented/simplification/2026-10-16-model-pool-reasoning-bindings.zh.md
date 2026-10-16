# Agent Note: Model Pools Bind Supported Native Reasoning Levels Directly in the Candidate Editor

Status: implemented

[English](2026-10-16-model-pool-reasoning-bindings.md) | 中文

## Problem

池自有推理编辑器要求操作者先声明候选的原生推理档位与声明取值集合，然后才能把任何公共偏好映射到原生档位，因此同一批原生取值被维护两次，且候选无法通过直接从其来源已经暴露的档位中选择来配置。用户请求了一个直接、有界的简化：为已选候选把每个受支持的原生档位绑定到统一模型池档位，并在模型列表中展示统一档位，且不引入新的持久格式、不改后端、不影响既有池专属路由、解析、隔离与默认守卫。

## Decision

- 候选编辑器的主面板通过一个原生取值下拉，把每个规范池偏好（`low`、`medium`、`high`、`xhigh` 与显式 `none`）绑定到一个原生档位。选项标签就是原生取值本身；`enum:<value>` 拼写仅是内部选项取值、绝不显示。
- 单个候选的原生选项是其精确来源映射声明的 `reasoning_efforts`（按服务商 id + trim 后原始 local/upstream 对 + 生效协议解析）、候选自身声明的取值与其已存枚举动作取值的并集。每个取值都 trim 并去重且不改写大小写，同级模型、其他来源或按名猜测的档位都不参与。
- 正向偏好绝不提供显式禁用动作，`none` 也绝不提供枚举或预算：当候选的原生档位并集已包含 `none`，或高级 `supports_disable` 开关被设置时，`none` 才提供显式禁用。
- 只有显式选择才创建池自有声明。当 profile 仍为空时，首次绑定选择候选协议的默认适配器（Chat Completions 与 Responses 为 `openai_effort`，Anthropic Messages 为 `anthropic_effort_inherit`）；已存在的非空 profile 始终恰好是操作者自己的选择。新绑定的原生取值只在其尚不存在时追加到先前的声明取值。
- 原生 profile、声明取值、预算范围与 `supports_disable` 字段移入默认收起的「高级声明」`<details>`，而外层逐候选面板保持其既有默认收起状态。未触碰的候选仍不发出 `reasoning` 键。
- manual-budget 绑定保留 manual-budget 档位并提供带自己原生 token 数的预算动作，仅开关档位保留其无强度梯度的说明；两者都保留正确的原生动作。
- 模型列表已发布规范池推理目录与普通来源原生徽标，因此那里无需任何产品改动；一条新回归确认把规范 `high` 绑定到原生 `max` 后，池行显示 `high`、普通来源行显示 `max`。
- 不变更任何 DTO、schema、后端、公开导出符号或路径。

## Alternatives considered

- 保留所有原字段为必填、而不是绑定优先的编辑器：拒绝，因为那样会保留直接请求要移除的冗余 profile 与取值维护；这些字段保留在高级披露之下、但不再是必填。
- 替换持久化的池自有用声明格式或增加后端改动：拒绝，因为它增加额外步骤与范围却没有用户可见的收益；既有的池自有 `reasoning` 负载及其后端校验被原样复用。

## Consequences

- 范围：只改变池候选编辑器及其派生的池自有声明。来源服务商配置与普通服务商编辑器不受影响，新回归让池行的规范档位与普通行的原生标识保持隔离。
- 保留既有行为：manual-budget 每映射 token、范围校验、被拒绝保存时保留草稿、运行时刷新不覆盖未保存编辑，以及池自有推理策略的全部路由、解析、隔离与默认守卫继续有效。
- 缺失元数据：精确来源不再声明 `reasoning_efforts` 的候选仍只提供其高级声明与已存枚举取值，空选项集合让正向偏好保持显式未映射，且没有任何来源元数据取值被提升为映射。
- 不宣称实时服务商：编辑器只从当前配置确定性地派生声明；它不证明真实服务商会遵守所绑定的原生档位，也未运行任何实时服务商检查。
- 部分取代：本记录只在池编辑器的 UI 路径及其默认适配器显式选择行为上部分取代 [Model Pools Own a Bounded Pool-Local Reasoning Policy](../feature/2026-10-15-pool-only-reasoning.md)——候选编辑器现在在其主面板中绑定规范偏好，而 profile、取值、范围与禁用字段移入默认收起的高级披露——该记录中的全部路由、解析、隔离与默认守卫继续有效。
- 本记录的部分取代：本记录只在默认收起的高级声明披露与其逐档位预算编辑上被 [Model Pool Candidate Editor Removes the Advanced Declaration Parameters](2026-10-16-model-pool-bindings-only-editor.md) 部分取代，两者在那里作为先前决策被移除；绑定优先的主面板、其默认适配器显式选择行为以及全部路由、解析、隔离与默认守卫继续有效。
