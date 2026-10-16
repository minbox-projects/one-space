# Agent Note: Model Pool Candidate Editor Removes the Advanced Declaration Parameters

Status: implemented

[English](2026-10-16-model-pool-bindings-only-editor.md) | 中文

## Problem

候选编辑器改为绑定优先后，操作者仍会看到一个「高级声明」披露区，用于配置原生 profile、声明取值集合、预算范围与 `supports_disable` 开关，另有一个逐档位的数字预算输入。这些控件重复了编辑器已经派生的原生推理元数据与存储声明本身已携带的内容，且 manual-budget 候选仍可被重绑到来源并未暴露的手输 token 数。用户请求了一个直接、有界的后续改动：从池候选编辑器中移除高级声明的全部参数及其界面配置项。

## Decision

- 候选编辑器主面板只保留五个规范绑定下拉（`low`、`medium`、`high`、`xhigh` 与显式 `none`），作用于该候选的原生档位；默认收起的「高级声明」`<details>` 及其原生 profile、声明取值集合、预算范围与 `supports_disable` 控件被移除，逐档位数字预算输入同样被移除。本地推理草稿只保存操作者选择的绑定。
- 派生仍以不可变的存储 `candidate.reasoning` 为只读基底：空 profile 上的首次绑定选择协议默认适配器（Chat Completions 与 Responses 为 `openai_effort`，Anthropic Messages 为 `anthropic_effort_inherit`），被绑定的原生枚举值仅在尚不存在时追加到 `values`，把 `none` 绑定到显式禁用时置 `supports_disable`。未触碰的新候选仍不发出 `reasoning` 键，清空全部新绑定不遗留孤儿声明。
- manual-budget 候选只把其原有的互异本机预算动作作为重绑目标，以 `Budget · <tokens>` 标签与 `budget:<tokens>` 选项值展示；没有数字或范围编辑。仅开关档位只提供 `enabled` 并保留其只读的无强度梯度说明。
- 精确来源未声明任何原生推理档位的候选显示一条指向服务商模型推理档位配置的提示，而不是高级说明。
- 不变更任何 DTO、schema、API、后端、模块、公开导出符号或路径。

## Alternatives considered

- 把这些高级参数保留在既有的收起披露区、而不是移除：拒绝，因为直接请求是移除全部高级参数及其界面项，且派生声明已经覆盖那些控件所配置的内容。
- 替换持久化的池自有声明格式或改后端以接受编辑器派生的负载形状：拒绝，因为既有的池自有 `reasoning` 负载及其校验已支持派生声明，格式或后端改动徒增范围而无用户可见收益。

## Consequences

- 范围：只改变池候选编辑器；来源服务商配置、普通服务商编辑器以及池专属路由、解析、隔离与默认守卫均不受影响。
- 保留行为：绑定优先的主面板、静态覆盖、用于保存与合法既有 manual-budget 或 switch 选择的不可变存储元数据、被拒绝保存时保留草稿、运行时刷新不覆盖未保存编辑，以及全部后端校验保证继续有效。
- 来源元数据：来源处新近变空的原生档位集合经服务商模型推理档位设置配置，而不经池编辑器；编辑器绝不把来源元数据取值提升为映射。
- 不宣称实时服务商：派生只从当前配置确定性得出，不证明真实服务商会遵守所绑定的原生档位；未运行任何实时服务商检查。
- 部分取代：本记录只在默认收起的高级声明披露与其逐档位预算编辑上部分取代 [Model Pools Bind Supported Native Reasoning Levels Directly in the Candidate Editor](2026-10-16-model-pool-reasoning-bindings.md)——两者在此被移除；该记录的绑定优先主面板以及全部路由、解析、隔离与默认守卫继续有效。
