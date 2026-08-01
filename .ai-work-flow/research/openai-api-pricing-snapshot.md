# OpenAI API 价格快照研究

- 研究主题：随 OneSpace 内置、版本化且可幂等初始化的 OpenAI API 价格快照
- 访问日期：2026-08-01（UTC 日期；未从页面推断价格的实际起始日期）
- 范围：OpenAI 官方定价页的 **Flagship models**，以及官方 Models / Prompt Caching 文档。下表不是 OpenAI 全部模型目录，也不包含音频、图像、工具或 Batch 价格。

## 可作为文本路由价格快照的官方模型

| 官方模型 ID | input（USD / 1M tokens） | output（USD / 1M tokens） | cache read / cached input（USD / 1M tokens） | cache write（USD / 1M tokens） |
| --- | ---: | ---: | ---: | ---: |
| `gpt-5.6-sol` | 5.00 | 30.00 | 0.50 | 6.25 |
| `gpt-5.6-terra` | 2.50 | 15.00 | 0.25 | 3.125 |
| `gpt-5.6-luna` | 1.00 | 6.00 | 0.10 | 1.25 |

模型 ID 由官方 Models 页面逐项列出；前三列和 `cached input` 由官方 API Pricing 页面列出。官方 Prompt Caching 文档说明 GPT-5.6 及后续家族的 cache write 为未缓存 input 的 **1.25 倍**，因此最后一列是以该规则计算，而不是从定价页逐模型抄录。

对 GPT-5.6 之前的模型，官方说 cache write **没有额外费用**。这不应解释为所有模型都没有该字段，或其费率未知：对于 GPT-5.6 家族，官方明确计费并以 `cache_write_tokens` 报告。

## 快照与幂等初始化建议

- 使用不可变版本，例如 `openai-api-pricing-2026-08-01-r1`；若同一观察日需要修订，递增 `rN`，不要覆盖既有版本。
- `effective_at` 保守地记录为 `2026-08-01`（或实际抓取时刻的 UTC ISO-8601 值），其语义必须是“本快照被官方页面观察到的时间”，**不是**“价格从该时间开始生效”。官方来源未公布这些价格的生效时间。
- 同时保存 `source_url`、`observed_at` 和内容哈希。以 `(provider, version)` 为唯一键执行 upsert；写入内容哈希相同则 no-op，冲突则拒绝或要求新 revision。这样应用重复启动不会产生重复记录，也保留可审计历史。
- 金额以十进制字符串或定点小数保存，单位显式为 `USD_PER_MILLION_TOKENS`；不要用二进制浮点数。`cache_write` 应是可空字段，以便容纳官方未公布或不适用的未来模型。

## 本地测试 seed

建议最小集合为三个上述模型：`gpt-5.6-sol`、`gpt-5.6-terra`、`gpt-5.6-luna`。它们是官方当前首推的同一文本模型家族，分别覆盖高能力、均衡和成本敏感的路由选项，同时以最小数量验证排序、输入/输出计费、cache read 及有偿 cache write 的计算。不要把未在此研究范围内逐项验证价格的别名、旧模型或多模态模型加入默认 seed。

## 官方来源

1. OpenAI, [API Pricing](https://openai.com/api/pricing/)，访问于 2026-08-01。列出 GPT-5.6 Sol / Terra / Luna 的 input、cached input、output 标准处理价格；页面说明此处价格适用于小于 270K 的上下文长度。
2. OpenAI, [Models](https://platform.openai.com/docs/models)，访问于 2026-08-01。明确列出模型 ID `gpt-5.6-sol`、`gpt-5.6-terra`、`gpt-5.6-luna`，并列出其 input/output 价格。
3. OpenAI, [Prompt caching](https://platform.openai.com/docs/guides/prompt-caching)，访问于 2026-08-01。说明 GPT-5.6 及后续模型 cache write 为未缓存 input 的 1.25 倍，读取使用 cached-input rate；旧于 GPT-5.6 的模型没有额外 cache-write 费用；使用量字段为 `cached_tokens` 和 `cache_write_tokens`。
