# Agent Note: Native Anthropic Messages Entry and Protocol Recognition

Status: implemented

[English](2026-10-11-ai-gateway-anthropic-messages.md) | 中文

## Problem

AI 网关此前只中继 OpenAI 兼容的 `/v1/chat/completions` 与 `/v1/responses` 两个入口。服务商持久化的 `protocol` 只认识 `chat_completions` 与 `responses`，模型清单来源解析会丢弃 `supported_endpoints` 中既未声明 Chat Completions 也未声明 Responses 的条目，且回环服务器把 `/v1/messages` 当作未知路径。因此仅支持 Messages 的上游既无法被配置、也无法被原生调用，而使用原生 Messages 协议的 Claude Code 在网关中没有任何入口。

## Decision

- 持久化协议：在 `src-tauri/src/ai_gateway/types_config.rs` 新增 `UpstreamProtocol::AnthropicMessages` 并序列化为 `anthropic_messages`；它被服务商默认、每条映射行、模板默认与每个模板模型接受，`endpoint_path()` 返回 `/messages`。`src-tauri/src/ai_gateway/migration.rs` 中穷尽的协议匹配把新变体当作兼容管道处理；配置 schema 保持 3，不引入迁移。
- 来源识别：`src-tauri/src/ai_gateway/templates.rs` 在声明的 `supported_endpoints` 中按 Chat Completions > Responses > Messages 的偏好识别 `/messages` 与 `/v1/messages`；三者都不含的条目被丢弃，没有端点信息的条目不带协议并继承模板。通用继承与 Go 声明感知路径保持不变，源声明的 Messages 端点现在进入兼容集合，禁用/忽略状态与原子同步不变。
- 原生路由模块：`src-tauri/src/ai_gateway/anthropic.rs` 提供 `is_messages_path`、`DEFAULT_ANTHROPIC_VERSION`（`2023-06-01`）、Anthropic 的 `error_payload` 与 `stream_error_event` 分片；`src-tauri/src/ai_gateway/anthropic_sse.rs` 提供只读的 `AnthropicSseObserver`。
- 原生入口：`src-tauri/src/ai_gateway/runtime_http.rs` 提供 `POST /v1/messages`，包括 `?beta=true` 这样的查询串（同时容忍 `/messages`），以本地 Key 经 `Authorization: Bearer` 或 `x-api-key` 本地鉴权，网关自身生成的错误使用 Anthropic 错误信封/事件，只替换映射后的模型，把选中的上游密钥池 key 作为 `x-api-key` 发送，透传 `anthropic-beta` 与 `anthropic-version`，并且仅当调用方省略时才补充 `anthropic-version: 2023-06-01`。
- 原生流与计量：成功的原生字节原样中继，不注入 OpenAI `[DONE]`；原生流只有在 `message_stop` 之后才算成功，带内 `error` 事件绝不视为成功。`AnthropicSseObserver` 只读转发字节，在现有归一化之前按字段合并 `message_start.message.usage` 与累积 `message_delta.usage`（出现的值替换、缺失的值保留）；既有首客户端字节重试边界、取消、空闲读取超时与健康结算均不变。

## Alternatives considered

- 保留仅 OpenAI 的协议枚举并为 Messages 入口单独配置：未采纳，因为服务商默认与映射行已经表达有效协议，平行的配置面会重复模板绑定、密钥池与选择逻辑。
- 在本步就把 Chat Completions 与 Responses 输入转换为 Messages：未采纳，属 Step 1 范围之外；先交付原生入口、转换留待后续，因此 Step 1 只保留原生透传。
- 为未声明或元数据缺失的模型 id 猜测协议，或复用 OpenAI 入口服务 Messages：未采纳，因为猜测会错误路由一个从未声明 Messages 端点的模型，且目前还不存在调用方转换。
- 改写或规范化原生流字节，或注入 OpenAI 终止符：未采纳，因为原生调用方与本网关都是字节忠实的中继，注入 `[DONE]` 或改写事件会破坏 Messages 协议。

## Consequences

- 服务商默认或映射行可以持久化 `anthropic_messages` 并经原生 `POST /v1/messages` 入口服务；在转换交付之前该映射不被 OpenAI 入口选中，而原生 Messages 请求绝不选择 Chat Completions 或 Responses 上游。
- 来源同步在 Chat Completions 与 Responses 之后识别 `/messages` 与 `/v1/messages`，因此已声明 Messages 的模型能在同步中保留，而三者都不含的条目仍被丢弃、未声明的无元数据 id 仍被排除。
- 原生 SSE 保持字节精确，拆分用量按字段合并：已注册的夹具把 `message_start` 的 input/cache-read/cache-write 与最后一次累积 output 快照合并为 `10/20/3/7`，缺少 `message_stop` 的 EOF 与带内错误都记为失败，既有重试、取消、超时与健康边界全部保留。
- 兼容与永久差异：配置 schema 保持 3 且无新迁移，既有 OpenAI 载荷除既有模型改写外保持行为；本步不交付 Chat/Responses 到 Messages 的转换、Claude 终端同步、UI 协议选择器/对话框或终端适配器，也不添加任何整理的模型或价格清单。
- `MEMORY.md`、权威的 `ai-gateway-backend` 导航条目与重新生成的 `navigation.md` 在同一变更中描述新模块、符号、测试与行为。
- 部分取代：[API Gateway Provider Templates and Incremental Model Sync](2026-09-18-api-gateway-provider-templates.md) 保留其增量合并、忽略集合生命周期与称谓决策，而其端点过滤与识别新增 Messages；[Provider Templates Own Closed Strategies and Bind Providers by Template Id](2026-10-10-provider-template-bound-strategies.md) 保留其封闭策略字段、绑定校验与有效策略解析，而其 Go 声明基线现在接纳源声明的 Messages id；[API Gateway Cache Hit Rate Normalizes Provider Usage Semantics](../bug-fix/2026-09-21-api-gateway-cache-hit-accounting.md) 保留其四档归一化、存储迁移、合格谓词与覆盖契约，而原生 Messages 用量由独立的按字段合并观察。
