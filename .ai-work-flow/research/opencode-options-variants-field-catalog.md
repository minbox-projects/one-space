# OpenCode `options` 与 `variants` 字段目录研究

## 问题与范围

- 问题：动态行表单维护 OpenCode 模型的 `options` 与 `variants` 时，是否有可选择的官方字段清单；并区分配置 schema、SDK/provider 动态参数与内置 variants 自动生成。
- 允许来源：OpenCode 官方文档、`opencode.ai/config.json`、官方 GitHub `anomalyco/opencode` 源码。
- 截止与访问日期：2026-08-07。源码基线：官方仓库 `dev` 分支，当次查询的树 SHA 为 `be25c905fa1ccd040e122dec67227237dad89961`。
- 结论边界：下文仅说明 OpenCode 当前公开 schema、文档和运行时代码所证明的结构与内置行为；实际请求是否接受某个透传字段仍取决于该 model/provider 所使用的 AI SDK 与上游 API。

## 结论

**不存在一份由 OpenCode 定义、跨 provider/model 完整且统一的 `options` 字段与值域清单。** 适合产品的模型是：将静态连接/配置字段与模型请求参数分开；请求参数采用 provider/model 条件化字段目录加任意 JSON 兜底，而不能用一份固定枚举阻止未知字段。

`variants` 的每个 value 本质上是**一组模型选项覆盖值**，而不是对 `options` 的引用、也不是另一套参数语言。官方源码把模型 `options` 定义为 `Record<string, any>`，把 `variants` 定义为 `Record<string, Record<string, any>>`；内置 variants 生成器也直接返回这些选项对象。选中 variant 后应将其 value 视为对基础 options 的叠加/覆盖；应保留独立存储，不能把 value 序列化为 `options` 的别名。

来源：[配置 schema](https://opencode.ai/config.json)、[模型文档](https://opencode.ai/docs/models)、[模型定义与加载源码](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/provider/provider.ts)、[variants/选项转换源码](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/provider/transform.ts)。

## 三层约束

| 层 | OpenCode 已定义的内容 | 对动态表单的含义 |
| --- | --- | --- |
| Config schema 的静态约束 | `provider.<id>.options` 静态列出 `apiKey`、`baseURL`、`enterpriseUrl`、`setCacheKey`、`timeout`、`headerTimeout`、`chunkTimeout`；模型 `options` 是无属性清单的 object。模型 `variants.<name>` 只显式说明 `disabled:boolean`，但没有 `additionalProperties:false`，因此仍可包含任意附加键。 | 将连接字段做强类型控件；模型请求 options/variant value 不做全局字段白名单。`timeout`/`headerTimeout` 为正整数或 `false`，`chunkTimeout` 为正整数；其余列出的连接字段见 schema 类型。 |
| SDK/provider 动态参数 | 模型文档明确给出模型 `options` 的 OpenAI 与 Anthropic 示例；agents 文档明确说其他 agent 字段直接传给 provider，且取决于 model/provider。源码会按 SDK npm 包将参数包入对应 `providerOptions` 命名空间。 | 字段目录必须以 provider、模型 ID、SDK transport、模型能力为条件；未知字段须允许 JSON 值并提示“上游兼容性未知”。 |
| OpenCode 内置 variants | `ProviderTransform.variants()` 根据 `model.capabilities.reasoning`、模型 ID、SDK npm 包和 release date 生成 variant 名与覆盖对象；不同 transport 对同一“推理强度”使用不同键和嵌套结构。 | 内置推荐 variants 应显示为运行时数据，不应从通用 options 表静态推导，也不应把所有 provider 的字段混合展示。 |

静态证据：[schema](https://opencode.ai/config.json)；动态透传证据：[Agents - Additional](https://opencode.ai/docs/agents/#additional)；生成逻辑：[transform.ts](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/provider/transform.ts)。

## 可验证的常见模型字段

下表是官方材料明确出现、适合做条件化候选项的字段，不是完整清单。

| 字段 | JSON 值类型与官方可验证示例 | 适用条件 / 来源 |
| --- | --- | --- |
| `reasoningEffort` | string；`"none"`、`"minimal"`、`"low"`、`"medium"`、`"high"`、`"xhigh"`，但具体子集依模型而变；GLM 5.2 兼容路径还会用 `"max"`。 | OpenAI、Azure、Copilot、OpenAI-compatible 等路径生成；模型文档给出 `"high"`。不能将完整集合施加给所有 OpenAI 型模型。 |
| `reasoningSummary` | string；官方内置值为 `"auto"`。 | OpenAI/Azure/Copilot/Bedrock Mantle 的部分 reasoning variants 与 GPT-5 默认选项。它不是 schema 的全局枚举。 |
| `include` | string array；`["reasoning.encrypted_content"]`。 | OpenAI Responses 相关路径为无状态多轮 reasoning 取回加密推理状态；文档在 OpenAI `gpt-5` 选项中示例该值。 |
| `textVerbosity` | string；文档示例 `"low"`，源码对部分非 Codex、非 chat 的 GPT-5.x 默认注入 `"low"`。 | OpenAI GPT-5 相关，源码明确排除 Azure 与 `-chat` 的默认写入；并非所有模型共享枚举。 |
| `thinking` | object；例如 `{ "type": "enabled", "budgetTokens": 16000 }`；自适应路径为 `{ "type": "adaptive", "display": "summarized" }`。 | Anthropic 或其兼容 transport；不同 Claude 世代及 Kimi/Minimax 条件不同。文档示例为 Anthropic。 |
| `effort` | string；`low`、`medium`、`high`、`xhigh`、`max` 的子集。 | Anthropic adaptive thinking 与部分 Kimi 路径。它与 `reasoningEffort` 不是可无条件互换的同义字段。 |
| `reasoning` | object；例如 `{ "effort": "low" }`。 | OpenRouter 的内置 variants；是该 SDK 路径的嵌套参数格式。 |
| `thinkingConfig` | object；例如 `{ "includeThoughts": true, "thinkingBudget": 16000 }` 或 `{ "includeThoughts": true, "thinkingLevel": "low" }`。 | Google/Vertex；Gemini 2.5 用 budget，其他分支用 level。 |
| `reasoningConfig` | object；例如 `{ "type": "enabled", "budgetTokens": 16000 }` 或 `{ "type": "enabled", "maxReasoningEffort": "high" }`。 | Amazon Bedrock；Anthropic 与 Nova 分支结构不同。 |
| `disabled` | boolean；`true`。 | variant 层的唯一 schema 明确说明字段，用于关闭一个内置或自定义 variant。 |

字段来源：[Models - Configure models/Variants](https://opencode.ai/docs/models)、[transform.ts 的 `variants` 与 `options`](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/provider/transform.ts)。

## 指定字段核对

| 字段 | 是否由 OpenCode 定义 | 判定 |
| --- | --- | --- |
| `reasoningEffort` | 是，作为动态模型选项与内置 variant 覆盖键；不是全局 schema 枚举。 | 有官方文档例子，且源码生成多个 transport 的该键。 |
| `reasoningSummary` | 是，作为动态模型选项与部分内置 OpenAI 型 variant 覆盖键；不是全局 schema 枚举。 | 官方文档例子为 `"auto"`，源码会自动写入 `"auto"`。 |
| `thinking` | 是，作为 Anthropic 等条件化动态模型选项与内置 variant 覆盖键；不是全局 schema 枚举。 | 官方文档给出 object 结构；源码按模型世代生成 enabled/adaptive 结构。 |
| `include` | 是，作为动态模型选项与部分内置 variant 覆盖键；不是全局 schema 枚举。 | 官方文档与源码均给出加密 reasoning content 的 string array。 |
| `textVerbosity` | 是，作为 OpenAI GPT-5 条件化动态模型选项；不是全局 schema 枚举，也不是所有内置 variants 的必备字段。 | 官方文档例子和源码 GPT-5 默认处理均可验证。 |
| `serviceTier` | **未发现 OpenCode config schema、官方文档或上述内置 variants 生成器把它定义为静态字段、统一值域或自动 variant 字段。** | 它可落入无约束的模型 options 透传通道，但是否有效、允许什么值，应由目标 provider/AI SDK 的官方资料决定；不可标注为“OpenCode 已定义字段”。 |

## 建议的表单数据策略

1. 固定字段区只纳入 schema 的 provider 连接项，以及 `variants.<name>.disabled`。
2. “模型参数”行保存 `key: string` 和任意 JSON `value`，按 provider/model 呈现上表的建议项；选择建议项不代表全局合法性。
3. variant 保存 `name: string`、`disabled?: boolean` 和同样的参数行数组。其 value 使用与 options 相同的 JSON 值编辑器与校验器，但存储为独立覆盖对象。
4. 内置 variants 从 OpenCode 实际返回的模型信息读取；当无法取得运行时模型信息时，只展示官方文档中“非完整”的 provider 级提示，不伪造完整清单。
5. 对 `serviceTier` 保留手工 JSON 参数入口，不放入 OpenCode 预定义下拉选项；需要支持时另以目标 SDK/provider 的官方契约增加条件化枚举。

## 未知项与时效风险

- OpenCode 文档明确称内置 variants 列表“not comprehensive”；源码的规则也会随 `dev` 变更。因此字段建议和 variant 名必须版本化或运行时读取。
- schema 对模型 `options` 的 object 未定义字段和值域，不能据其推导请求成功率。
- `models.dev` 模型目录参与 OpenCode 的模型加载；provider/model 的可用字段不能仅根据 provider 名判断。
- 本报告未使用第三方文章、社区讨论或非官方 API 文档来填补 `serviceTier` 的值域空缺。

## 官方引用清单

1. https://opencode.ai/config.json
2. https://opencode.ai/docs/config
3. https://opencode.ai/docs/models
4. https://opencode.ai/docs/agents/#additional
5. https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/provider/provider.ts
6. https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/provider/transform.ts
