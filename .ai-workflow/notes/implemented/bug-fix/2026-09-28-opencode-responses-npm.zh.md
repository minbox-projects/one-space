# Agent Note: OpenCode Responses-Only Models Carry the @ai-sdk/openai npm Override

Status: implemented

[English](2026-09-28-opencode-responses-npm.md) | 中文

## Problem

同步写出的 opencode 网关服务商使用服务商级 npm `@ai-sdk/openai-compatible`（`tool_config.npm` 加 `tool_config.options`），而该包的 language model 始终 POST `/chat/completions`。因此，当网关只能经 Responses 协议服务某个本地模型时——该模型的全部启用映射行都声明 `responses`，或所属服务商协议为 `responses`——经 opencode 的调用必然失败：opencode POST 到 `/chat/completions`，解析只找到 Responses 行，网关返回 `502` 与 `all providers unavailable: ... serves model '...' via /responses`。网关按既有文档边界不做请求体转换，中继无法桥接两种协议；只有工具侧的 endpoint 选择能解决。

## Decision

`build_gateway_provider`（`src-tauri/src/ai_gateway/commands.rs`）现在按本地模型逐个判定。写出 opencode 模型条目前，它用新的私有 helper `gateway_protocol_servable(gateways, local_model, protocol)` 在所有启用网关上分别求两个布尔值：只有 `selection::resolve_model_for_protocol` 对该网关返回 `Serve(_)` 时才计为可服务——与请求路径同一套权威解析，因此禁用与自动禁用行绝不计入、行级协议覆盖生效、`default_model` 兜底同样有效。当该本地模型不可经 Chat 服务、但可经 Responses 服务时，条目追加 `"provider": { "npm": "@ai-sdk/openai" }`（新私有常量 `OPENCODE_RESPONSES_NPM`）；该包自带的 `languageModel()` 解析为 Responses API，并在同一 `baseURL` 上 POST `/responses`。任一启用网关能经 Chat Completions 服务该模型时保持既有条目形态，因此混合协议的本地模型仍走 chat，服务商级 `@ai-sdk/openai-compatible` 默认不变。网关仍不做任何请求体转换。

## Alternatives considered

- 在网关内把 chat 请求转换为 Responses 请求：未采纳，因为违反既有的「不做请求体转换」边界；中继按原字节转发，endpoint 选择属于工具侧。
- 从同步的 opencode 模型清单中隐藏仅 Responses 的本地模型：未采纳，因为这会隐藏网关本可服务、其他调用方也已能经 `/responses` 使用的模型。
- 把整个 opencode 服务商条目切换为 `@ai-sdk/openai`：未采纳，因为同一映射清单里的纯 chat 模型会因此 POST `/responses` 而失败。

## Consequences

- 只能经 Responses 服务的本地模型现在可经 opencode 使用，opencode 会为该模型条目调用 `/responses`；纯 chat 与混合协议模型保持原形态，opencode 默认的 `/chat/completions` 路径对它们不变。混合协议的本地模型即使有某个网关能经 Responses 服务，也仍走 chat。
- 可服务性按本地模型跨全部启用网关求值，绝不按贡献该行的单个服务商：禁用网关、用户禁用行或自动禁用行都不能把模型变成仅 Responses，而由某网关 `default_model` 服务的模型可以计入。
- 该覆盖只存在于终端同步写出的工具侧记录中；持久化网关 schema、命令签名与模板都不变，回滚只会在下一次同步时丢弃多出的键。
- 验证：`cargo test --manifest-path src-tauri/Cargo.toml --lib build_gateway_provider`（exit 0，20 通过）与 `cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway::`（exit 0，574 通过）覆盖仅 Responses、行级协议覆盖、混合协议 chat 优先、禁用/自动禁用行与 `default_model` 可服务性。
- Supersession（取代评估）：部分取代。[Gateway Reasoning Efforts Sync to OpenCode Model Variants](../architecture/2026-09-20-gateway-reasoning-efforts-in-terminal-sync.md) 记录「没有 `reasoning_efforts` 的映射保持 `{ "name": ... }` 形态」；该断言现在只在任一启用网关可经 Chat Completions 服务该模型时成立，因为仅 Responses 的条目还会携带 `provider.npm`。它的 variants、重复保留首个与仅取启用行的规则继续有效，其余 active 的终端同步记录互不相关。
- `MEMORY.md`、`docs/USAGE.md`、`.ai-workflow/index/navigation.json` 的 `ai-gateway-backend` 条目与重新生成的 `navigation.md` 在同一变更中记录该按模型 npm 规则。
