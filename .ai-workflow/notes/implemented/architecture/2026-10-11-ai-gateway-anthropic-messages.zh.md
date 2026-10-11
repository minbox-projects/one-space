# Agent Note: Native Anthropic Messages Entry, Protocol Recognition and OpenAI Entry Conversion

Status: implemented

[English](2026-10-11-ai-gateway-anthropic-messages.md) | 中文

## Problem

AI 网关此前只中继 OpenAI 兼容的 `/v1/chat/completions` 与 `/v1/responses` 两个入口。服务商持久化的 `protocol` 只认识 `chat_completions` 与 `responses`，模型清单来源解析会丢弃 `supported_endpoints` 中既未声明 Chat Completions 也未声明 Responses 的条目，且回环服务器把 `/v1/messages` 当作未知路径。因此仅支持 Messages 的上游既无法被配置、也无法被原生调用，而使用原生 Messages 协议的 Claude Code 在网关中没有任何入口。当这类上游可配置之后，既有 OpenAI 入口仍然无法触达它：候选解析把 Anthropic 映射排除在 Chat 与 Responses 请求之外，且不存在任何请求、响应或流转换，因此使用 Chat Completions 或 Responses 的调用方无法使用网关仅能经 Messages 服务的模型。

## Decision

- 持久化协议：在 `src-tauri/src/ai_gateway/types_config.rs` 新增 `UpstreamProtocol::AnthropicMessages` 并序列化为 `anthropic_messages`；它被服务商默认、每条映射行、模板默认与每个模板模型接受，`endpoint_path()` 返回 `/messages`。`src-tauri/src/ai_gateway/migration.rs` 中穷尽的协议匹配把新变体当作兼容管道处理；配置 schema 保持 3，不引入迁移。
- 来源识别：`src-tauri/src/ai_gateway/templates.rs` 在声明的 `supported_endpoints` 中按 Chat Completions > Responses > Messages 的偏好识别 `/messages` 与 `/v1/messages`；三者都不含的条目被丢弃，没有端点信息的条目不带协议并继承模板。通用继承与 Go 声明感知路径保持不变，源声明的 Messages 端点现在进入兼容集合，禁用/忽略状态与原子同步不变。
- 路由模块：`src-tauri/src/ai_gateway/anthropic.rs` 提供原生辅助 `is_messages_path`、`DEFAULT_ANTHROPIC_VERSION`（`2023-06-01`）、Anthropic 的 `error_payload` 与 `stream_error_event` 分片，以及转换入口 `is_conversion_path`、`convert_request`、`convert_response` 与 `validate_conversion_request`；`src-tauri/src/ai_gateway/anthropic_sse.rs` 提供只读的 `AnthropicSseObserver` 与 `AnthropicSseConverter`。
- 原生入口：`src-tauri/src/ai_gateway/runtime_http.rs` 提供 `POST /v1/messages`，包括 `?beta=true` 这样的查询串（同时容忍 `/messages`），以本地 Key 经 `Authorization: Bearer` 或 `x-api-key` 本地鉴权，网关自身生成的错误使用 Anthropic 错误信封/事件，只替换映射后的模型，把选中的上游密钥池 key 作为 `x-api-key` 发送，透传 `anthropic-beta` 与 `anthropic-version`，并且仅当调用方省略时才补充 `anthropic-version: 2023-06-01`。
- 请求转换：启用的 `anthropic_messages` 映射（或 Anthropic 默认）现在也服务 Chat 与 Responses 调用方。`convert_request` 把 `system`/`developer` 消息与 Responses 的 `instructions` 转换为 Messages 的 `system` 字段，把文本与 URL/base64 的 `image_url`/`input_image` 内容转换为文本/图片块，把 `user`/`assistant` 轮次、客户端函数工具 schema 转换为带 `input_schema` 的 `tools`，把助手 `tool_calls` 与 Responses 的 `function_call` 转换为 `tool_use`，并把 `tool`/`function_call_output` 结果转换为 `tool_result`，保留工具 id 与多轮连续性。`tool_choice` 把 `auto`/`required`/`none`/`function` 映射为 Messages 对应值，`parallel_tool_calls: false` 追加 `disable_parallel_tool_use`，`temperature`/`top_p` 透传，Chat 的 `stop` 变为 `stop_sequences`。对 Chat，`max_completion_tokens` 优先于 `max_tokens`；Responses 使用 `max_output_tokens`；省略的上限默认 4096；显式零或正整数保留，负数、小数或字符串上限在任何 Anthropic 尝试之前以字段级 400 拒绝。原生请求绝不接受该转换默认值。
- 不支持的转换：`previous_response_id`、未解析的条目引用、`background: true`、`store: true`、`n > 1`、非空 `response_format`（JSON/schema 输出）与不可翻译的非空推理选项，都会在 Anthropic HTTP 或 key 健康尝试之前以字段级 400 拒绝，且当任一候选解析为 `anthropic_messages` 时 `runtime_http.rs` 会在尝试之前执行该校验。携带这些字段的同协议 OpenAI 请求保持既有透传行为，绝不被转换器拒绝。
- 响应转换：`convert_response` 把 Anthropic JSON 响应体转换为调用方格式并标识本地模型：文本块变为 Chat 消息内容与 Responses 的 `output_text`，`tool_use` 块变为 Chat 的 `tool_calls` 与 Responses 的 `function_call` 条目（参数的字符串形式），完成原因与用量一并转换；无法转换的上游 2xx 响应体变为调用方格式的 502 `conversion_error`，而不是原始 Anthropic 字节。
- 流转换：`AnthropicSseConverter` 把分片的 Anthropic SSE 增量转换为调用方事件，具有稳定的文本与工具标识和有序参数增量：`message_start` 捕获消息 id 并合并用量且不发增量，`content_block_start`/`input_json_delta` 发出 Chat 工具调用分片或 Responses 的 `response.output_item.added`/`response.function_call_arguments.delta`，文本增量发出 Chat 内容或 Responses 的 `response.output_text.delta`，`message_stop` 发出恰好一次 Chat `[DONE]` 或 Responses 的 `response.completed`。ping 等原生专属事件不编造任何内容。`AnthropicSseObserver` 只读转发字节，按字段合并 `message_start.message.usage` 与累积 `message_delta.usage`（出现的值替换、缺失的值保留）；原生与转换流都只有在 `message_stop` 之后才算成功，带内 `error` 事件绝不视为成功。
- 失败与生命周期：首字节之后出错、EOF 或缺少 `message_stop` 会以调用方格式结束转换流且绝不写成功完成标记或重放，而原生流保持其原生错误格式；失败前捕获的用量保留在该失败尝试中。既有首客户端字节重试边界、下游取消（丢弃缓冲行且不结算上游健康）、空闲读取超时、key 轮换、探测与健康结算均不变。
- 转换感知解析与计量：`src-tauri/src/ai_gateway/selection.rs` 在候选的有效协议等于入站协议时，或在候选为 `anthropic_messages` 且入站协议为 Chat Completions 或 Responses 时解析该候选。因此原生 Messages 绝不选择 Chat 或 Responses 上游，Chat-only 与 Responses-only 上游在没有 Anthropic 适配器时保持互斥，禁用行、自动禁用行、默认、加权调度、会话亲和与 key/重试策略保持既有行为。用量在转换之前取自实际上游：按字段的 Anthropic 合并归一化为既有四档，因此转换后的 Chat 响应报告包含式输入（普通输入加缓存读加缓存写，符合 OpenAI 形态），而持久化的普通上游输入保持独立。

## Alternatives considered

- 保留仅 OpenAI 的协议枚举并为 Messages 入口单独配置：未采纳，因为服务商默认与映射行已经表达有效协议，平行的配置面会重复模板绑定、密钥池与选择逻辑。
- 把原生入口与 OpenAI 入口转换作为不可分割的一步交付：未采纳，因为原生入口与协议识别可独立验证，而转换依赖它们的选择与运行时管道已经就位。
- 直接把 Chat Completions 转换为 Responses 或反向转换、合成多个生成候选、增加服务端 Responses 会话/检索/后台存储，或猜测结构化输出与推理参数等价物：未采纳，因为这些都不是调用方请求的协议，且会凭空发明上游可能不支持的行为；不受支持的关键字段改为按字段拒绝。
- 修复或静默改写不支持的转换输入而不是拒绝它：未采纳，因为发送畸变或降级的请求会歪曲调用方意图；在任何尝试之前给出字段级 400 是明确且安全的。
- 让转换输出上限无界或继承任意上游默认值：未采纳，因为省略的上限需要一个确定且有文档的边界；固定的 4096 默认值只应用于转换请求。
- 改写或规范化原生流字节，或向原生流注入 OpenAI 终止符：未采纳，因为原生调用方与本网关都是字节忠实的中继，注入 `[DONE]` 或改写事件会破坏 Messages 协议。

## Consequences

- 服务商默认或映射行可以持久化 `anthropic_messages`，既经原生 `POST /v1/messages` 入口服务，也经转换通过既有 `/v1/chat/completions` 与 `/v1/responses` 入口以调用方请求的模型标识服务；原生 Messages 请求仍只选择 Anthropic 上游。
- 请求、JSON 响应与 SSE 转换覆盖 system/developer/instructions、文本与 URL/base64 图片、函数工具 schema、调用与结果、tool choice、并行控制、采样/停止选项与输出上限，并给出有序的增量调用方事件；上限遵循既定优先级（Chat `max_completion_tokens` 优先于 `max_tokens`，Responses `max_output_tokens`，默认 4096），非法上限与不受支持的关键语义在任何 Anthropic 尝试之前按字段失败。
- 流保持增量与稳定的标识和顺序：Chat 以 `[DONE]` 恰好结束一次，Responses 携带自己的工具与完成事件，仅 ping 输入不编造内容，首字节后出错、EOF 或缺少 `message_stop` 产生调用方格式的失败且无成功标记或重放，同时保留已捕获的用量。
- 转换感知解析保留每一条运行时边界：尝试与探测都携带实际生效的上游协议，加权调度保持在入站范围内，排除、默认阻塞与 key/重试策略成立，原生 Messages 绝不选择 OpenAI 上游，Chat-only 与 Responses-only 上游保持互斥。
- 计量保持实际：按字段的 `message_start`/`message_delta` 合并归一化为既有四档，转换后的 Chat 响应包含式输入可与持久化的普通上游输入不同，而尝试、价格与服务商标识仍取上游实际值。
- 兼容与边界：配置 schema 保持 3 且无新迁移，同协议 OpenAI 载荷除既有模型改写外保持既有行为，且不添加 Chat 到 Responses 的转换、多候选、服务端 Responses 存储、结构化输出/推理推断或整理的模型/价格清单。本变更不交付 Claude 终端同步、其 UI 选择器/对话框或任何终端同步行为。
- `MEMORY.md`、权威的 `ai-gateway-backend` 导航条目与重新生成的 `navigation.md` 在同一变更中描述新模块、符号、测试与行为。
- 部分取代：[API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md) 保留其增量合并、忽略集合生命周期与称谓决策，而其端点过滤与识别新增 Messages，且其识别出的 Messages 映射现在可经既有 OpenAI 入口通过转换服务；[Provider Templates Own Closed Strategies and Bind Providers by Template Id](2026-10-10-provider-template-bound-strategies.md) 保留其封闭策略字段、绑定校验与有效策略解析，而其 Go 声明基线接纳源声明的 Messages id，且该 id 现在可由转换经 OpenAI 入口服务；[API Gateway Cache Hit Rate Normalizes Provider Usage Semantics](../bug-fix/2026-09-21-api-gateway-cache-hit-accounting.md) 保留其四档归一化、存储迁移、合格谓词与覆盖契约，而原生与转换的 Messages 用量由独立的按字段合并观察，且转换调用方的包含式输入仍与持久化的普通档位不同；[OpenCode Responses-Only Models Carry the @ai-sdk/openai npm Override](../bug-fix/2026-09-28-opencode-responses-npm.md) 保留其对真正仅 Responses 模型的按模型 npm 覆盖，而其「网关不做请求体转换」边界与逐模型可服务性现在计入 Chat/Responses 到 Messages 的转换使 Anthropic 映射经 Chat 可服务。
