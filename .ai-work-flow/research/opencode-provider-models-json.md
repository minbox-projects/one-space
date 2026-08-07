# OpenCode `provider.models` JSON 结构与语义核对

**问题**：核对 OpenCode `provider.models` 模型条目的官方 JSON 结构与语义，重点涵盖显示名称与 key/model ID、成本、限制、variants、options 和未知字段。

**范围与时点**：仅使用 OpenCode 官方文档、其公开发布的 `config.json` schema，以及官方 GitHub `anomalyco/opencode` 默认 `dev` 分支源码；查阅日期为 2026-08-07。该 schema 未在响应中标注独立版本或发布日期，因此结论对应查阅时的在线 schema 和默认分支，不可外推至历史版本或未来版本。

## 结论摘要

`provider.<provider_key>.models` 是以**模型 key**为键的对象。常规选择引用为 `<provider_key>/<model_key>`；`name` 是模型显示名称，不能替代该选择 key。条目可选的 `id` 可用于给提供商的实际模型/部署标识，官方 Bedrock 示例以模型 key 映射到 ARN 说明这一用途。

`cost` 的四个费率均按每 1,000,000 token 参与计费计算；官方代码没有为这个货币数值声明货币代码或符号，故不能仅据 OpenCode 官方资料断言一定是 USD。`input` 与 `output` 在存在 `cost` 时必填；两种 cache 字段可省略但不可为 `null`。`limit` 整体可省略，存在时 `context` 与 `output` 必填、`input` 可省略，三者为非空数值。

模型条目、`cost` 和 `limit` 禁止未列字段；模型 `options` 是开放对象，`variants` 的每个变体值也是开放对象。因此“未知字段会保留”的官方依据只适用于这两个开放对象；在其它受限对象中，在线 schema 将其判为不合法。官方文档说明配置通过 schema 进行验证，源码也将解析结果送入 `ConfigV1.Info` schema；本次没有找到官方承诺“校验失败后仍原样保存/转发未知字段”的说明。

## 结构和身份关系

最小自定义模型示意（字段取值仅为结构示例）：

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "custom-provider": {
      "models": {
        "transport-model-id": {
          "name": "UI display name"
        }
      }
    }
  },
  "model": "custom-provider/transport-model-id"
}
```

| 项目 | 官方结论 | 证据 |
| --- | --- | --- |
| provider key | `provider` 对象中的键；自定义 provider 的 `provider_id` 就是该键。 | [模型文档](https://opencode.ai/docs/models/) |
| model key | `provider.models` 对象中每个条目的键；`model_id` 就是该键，且默认模型以 `provider_id/model_id` 引用。 | [模型文档](https://opencode.ai/docs/models/) |
| `name` | 可选字符串，作为模型显示名称；官方 Atomic Chat 示例明确把 models 描述为“model IDs 到 display names”的映射。它不是选择 ID。 | [Providers 文档](https://opencode.ai/docs/providers/)；[在线 schema](https://opencode.ai/config.json) |
| `id` | 可选字符串。官方 Bedrock 示例在键 `anthropic-claude-sonnet-4.5` 下填入 ARN；这是将配置中选择的模型 key 映射到提供商请求所用部署/模型标识的官方示例。 | [Providers 文档](https://opencode.ai/docs/providers/)；[在线 schema](https://opencode.ai/config.json) |

限制：官方文档没有明确规定 `name` 缺省时 UI 的回退显示规则，也没有提供独立文字定义说明 `id` 对每一种 provider 的传输优先级。不要把该 Bedrock 特例推广为所有 provider 的相同传输协议。

## `cost`

在线 schema 中 `cost` 本身可省略；若提供，则如下：

| 字段 | JSON 类型与可空性 | 必填性 | 单位与运行时语义 |
| --- | --- | --- | --- |
| `input` | `number`，不接受 `null` | 必填 | 非缓存输入 token 费率。源码以 `inputTokens * input / 1_000_000` 加入会话成本。 |
| `output` | `number`，不接受 `null` | 必填 | 输出 token 费率。源码以 `outputTokens * output / 1_000_000` 计费；reasoning token 也暂按此费率计费。 |
| `cache_read` | `number`，不接受 `null` | 可省略 | 缓存读取 token 费率；缺省时运行时代码以 `0` 计费。公式同样除以 1,000,000。 |
| `cache_write` | `number`，不接受 `null` | 可省略 | 缓存写入 token 费率；缺省时运行时代码以 `0` 计费。公式同样除以 1,000,000。 |

源码对模型目录数据使用 `Schema.Finite`，故运行时模型费率必须是有限数值；JSON 本身也没有 `NaN` / `Infinity` 字面量。发布的 JSON Schema 只约束为 `number`，未声明最小值、整数性、币种或四舍五入规则。

证据：[在线 config schema](https://opencode.ai/config.json) 的 `ProviderConfig.models.*.cost`；[会话成本实现](https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/session/session.ts) 的 `getUsage`；[模型目录 schema](https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/provider/models.ts)。

## `limit`

`limit` 在配置模型覆盖条目中可省略；若提供，发布 schema 的精确约束为：

| 字段 | JSON 类型与可空性 | 必填性 | 官方可确认的语义 |
| --- | --- | --- | --- |
| `context` | `number`，不接受 `null` | 必填 | 模型上下文窗口限制元数据。 |
| `output` | `number`，不接受 `null` | 必填 | 最大输出 token 限制元数据。 |
| `input` | `number`，不接受 `null` | 可省略 | 输入 token 限制元数据。 |

发布 schema 未给这三项设置 `minimum`、`integer` 或单位描述；官方模型目录源码将它们读为有限数值。不能据此断言配置加载器会把负数、分数自动归一化为整数 token 数。

证据：[在线 config schema](https://opencode.ai/config.json)；[模型目录 schema](https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/provider/models.ts)。

## `variants` 与 `options`

| 字段 | 结构 | 语义 |
| --- | --- | --- |
| `options` | 模型条目下的开放 JSON object；其成员未被在线 schema 列举或限制。 | 全局模型 provider 参数。官方示例直接放入 `reasoningEffort`、`textVerbosity`、`reasoningSummary`、`include`，并说明 agent 配置中的选项会覆盖这些全局选项。 |
| `variants` | 以变体名称为键的 object；每个值是开放 object。在线 schema 明确识别的成员为可选布尔值 `disabled`。 | 让同一模型有不同配置，避免复制模型条目。文档说明可新增或覆盖内建变体；`disabled: true` 禁用该变体。变体示例将 provider 参数直接置于变体对象内，例如 `reasoningEffort`。 |

官方文档列出许多 provider 的内建变体（例如 OpenAI 的 `none` 至 `xhigh`），但也明确说实际支持集合因模型而异。不要把文档示例的参数值集合当成全部 provider 的通用、强校验枚举。

证据：[模型文档](https://opencode.ai/docs/models/)；[在线 config schema](https://opencode.ai/config.json)。

## 未知字段与保留边界

| 位置 | 在线 schema 的 `additionalProperties` | 可得结论 |
| --- | --- | --- |
| 模型条目 | `false` | 除列出的字段外，不应写入未知键。 |
| `cost`、`cost.context_over_200k`、`limit` | `false` | 不应写入未知键。 |
| `options` | 未设置 `additionalProperties: false` | 开放对象；schema 不会因其未知成员拒绝该对象。 |
| `variants` | 变体名称由 `additionalProperties` 映射到 object；该 object 未设置 `additionalProperties: false` | 开放对象；可放 provider/变体特定参数，`disabled` 是唯一被显式说明的通用字段。 |

官方配置文档说 `https://opencode.ai/config.json` 是运行时配置 schema，编辑器可据此验证和补全；官方加载源码调用 `ConfigParse.schema(ConfigV1.Info, ...)`。这支持将上述封闭/开放结论用于配置校验。仍不明确的是：官方没有承诺对“在封闭对象中出现未知字段”的失败方式（报错文本、是否跳过文件）以及是否会在任何非标准加载路径中保留原始 JSON。因此本报告不把“未知字段保留”表述为全局运行时保证。

证据：[配置文档](https://opencode.ai/docs/config/)；[在线 config schema](https://opencode.ai/config.json)；[配置加载源码](https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/config/config.ts)。

## 官方引用清单

1. https://opencode.ai/docs/models/
2. https://opencode.ai/docs/providers/
3. https://opencode.ai/docs/config/
4. https://opencode.ai/config.json
5. https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/session/session.ts
6. https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/provider/models.ts
7. https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/config/config.ts
