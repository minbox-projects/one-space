# Agent Note: Reflection/Beam Pool Chat Candidates Repair Missing Historical Reasoning

Status: implemented

[English](2026-10-31-reflection-beam-historical-reasoning-repair.md) | 中文

## Problem

Reflection/Beam 的 Chat Completions API 要求每条较早的助手消息都带有非空 `reasoning_content`，并在任一历史助手轮次的该值缺失、为 `null`、空串或纯空白时返回 HTTP 400。重放一次未记录思考的降级轮次的客户端因此无法继续对话，而所选 Reflection/Beam 候选全部过不了这项检查的模型池会在回退到无关来源之前，为每个候选浪费一次上游尝试。此前的网关行为只把该拒绝归类为失败后跳过；它保住了池的连续性，却从未让受影响的候选变得可服务。

## Decision

- 新模块 `src-tauri/src/ai_gateway/reasoning_history.rs` 承载这一有界修复。`model_requires_historical_reasoning` 在 trim 与 ASCII 小写后识别 `reflection/` 命名空间中的上游模型，`repair_historical_reasoning` 遍历 Chat Completions JSON 请求体的助手消息。
- `forwarding.rs::build_request` 在生效上游协议为 Chat Completions 且解析后的上游 `model` 参数位于该命名空间时应用该修复，因此同时覆盖普通路由（例如把 `reflection/beam:free` 映射到 `reflection/beam:free` 的普通路由）与已准备模型池候选。修复在协议转换与模型改写之后、池原生推理注入之前运行。
- 对每条 `reasoning_content` 缺失、为 `null`、为空串或纯空白的助手消息，修复写入非空占位符 `[reasoning unavailable]`。既有非空值与其余每个消息字段均被保留。非对象请求体、没有 `messages` 数组的请求体以及无需修复的请求体按原始字节返回。
- 任何路由上位于 `reflection/` 命名空间之外的模型、OpenCode Zen Free 候选 `beam-free`、`glm-5.3-flash-free` 与 `deepseek-v4.1-flash-free`、每种非 Chat Completions 协议以及其他所有普通路由均不受影响，仍原样转发调用方历史。
- 既有的缺失历史推理拒绝分类保留为后备，用于修复不适用或上游仍拒绝请求的情形：当前请求跳过该已准备池候选，不重试、不冷却、不标记 key，也不变更映射健康。

## Alternatives considered

- 保留仅跳过行为并让池移向下一个候选：拒绝，因为每个 Reflection/Beam 候选仍会收到上游已知会拒绝的请求，而只由这类候选组成的池总会回退到无关来源。
- 为缺失轮次合成真实推理文本：拒绝，因为这会臆造 provider 输出并错误呈现模型实际产生的内容。
- 对每个模型应用该修复而不管命名空间：拒绝，因为只有 Reflection/Beam 声明了这一历史要求，改写每个请求体会改变字节并冒险触碰无关的上游契约。
- 删除受影响的助手消息或把它们改成其他角色：拒绝，因为这会改变对话历史与调用方可观察的上下文。
- 使用空串或纯空白占位符：拒绝，因为 Reflection/Beam 把空白值视为缺失，只有非空标记才能满足检查。

## Consequences

- 当较早助手轮次缺少已记录推理时，普通或已准备池的 Reflection/Beam Chat 路由现在首次尝试即成功，同时调用方的非空推理被精确保留，且没有缺失值的请求体与其输入保持字节一致。
- 该修复刻意保持狭窄：它不臆造推理内容、在任何路由上都不触碰 `reflection/` 命名空间之外的模型，也不改变任何其他协议。provider 自身的降级缺失思考模式会为占位符渲染空分析块。
- 占位符对上游与后续历史重放可见；它是有界标记而非推理连续性保证，网关仍不承诺跨模型的通用推理连续性。
- 验证：`src-tauri/src/ai_gateway/tests/model_pools.rs` 中三个 mock 行为测试通过，其中包括覆盖非池路由的 `ordinary_route_preserves_and_fills_reasoning_content`；`ai_gateway::tests::model_pools::` 套件报告 95 通过，`ai_gateway::tests::openai_conversion::` 报告 44 通过。一次使用生产配置副本的临时带外验证（副本已删除，未修改任何生产密钥）对全部四个 `reflection/beam:free` 服务商以及对后备 `deepseek-v4.1-flash` 与 `glm-5.3-flash` 返回 HTTP 200；完整免费模型池序列返回 5/5 HTTP 200 且未尝试 Zen。同一时间窗内三个 Zen 免费候选分别处于其上游免费额度上限（HTTP 429 / 额度耗尽的 key），这是上游状态而非网关代码失败；移除 reflection 失败负载也移除了到达它们的回退压力。
- 部分取代：[Model Pool Portable Contract Recognizes the Assistant reasoning_content Trace](2026-10-14-model-pool-reasoning-content.md) 予以保留并交叉引用。本记录收窄该记录此前关于缺失历史推理拒绝绝不改写、绝不臆造的绝对表述：上述占位符修复是唯一有界例外，而该记录对普通同协议 Chat 流量的精确 JSON 保留，以及其在 Responses 与原生 Messages 转换上的省略且不臆造，仍然成立。
