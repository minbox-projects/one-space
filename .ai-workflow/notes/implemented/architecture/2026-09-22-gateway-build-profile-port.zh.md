# Agent Note: API Gateway Resolves the Listening Port per Build Profile

Status: implemented

[English](2026-09-22-gateway-build-profile-port.md) | 中文

## Problem

API 网关把配置保存在 `get_app_dir()` 下单一加密文件 `api_gateway.json` 中，而 `tauri dev`（debug 构建）与已安装的 release 构建共用同一个 `~/.config/onespace` 目录。存储的 `port` 字段由更早的 release 运行写为 release 默认值 `17688`，且 `normalize_config` 只在该值为 `0` 时才替换它。`default_port()` 在 debug 构建中已返回 `DEV_DEFAULT_PORT`（`17689`），但该值只在配置文件不存在时生效，因此 `npm run tauri dev` 仍然读取存储的 `17688`，与正在运行的已安装应用争抢同一个回环端口。侧边栏由前端 `import.meta.env.DEV` 驱动的 DEV 徽标与实际使用的端口因此毫无关联。

## Decision

`types_config::resolve_port(stored: u16, is_dev: bool) -> u16` 按当前构建 profile 解析有效监听端口：存储的 `0` 回退到当前 profile 默认值；两个规范默认值按 profile 双向翻译，即 debug 构建把 `17688` 解析为 `DEV_DEFAULT_PORT`（`17689`），release 构建把 `17689` 解析回 `DEFAULT_PORT`（`17688`）；其他任何存储值都是真实自定义端口，原样返回。`storage::normalize_config` 在每次读取与写入时应用 `resolve_port(config.port, cfg!(debug_assertions))`，因此 `read_config`（进而 `api_gateway_get_config`、`api_gateway_status`、`start_server`、autostart 与终端同步 Base URL）始终看到当前 profile 的有效端口，某个 profile 写入的配置绝不会把另一个 profile 移出自己的端口。在写入时同样归一化另一个 profile 的规范默认值，正是让共享文件在两个方向上都安全的原因。

## Alternatives considered

- 独立的 dev 配置文件（如 `api_gateway.dev.json`）：未采纳，因为它会隔离服务商、本地 Key 与启用状态，导致开发会话以空网关启动，而不是复用用户的真实配置。
- 通过 npm 脚本接入环境变量覆盖：未采纳，因为构建 profile 已经能区分两种运行时，额外包装脚本只会增加脆弱的脚本接线，且解决不了 release 读取 dev 写入值的问题。
- 在 `#[cfg(test)]` 下关闭该映射以保持既有夹具的 `17688`：未采纳，因为这是不诚实的测试行为；测试构建就是开发者运行的 debug profile，改为把唯一依赖端口的终端同步夹具换成自定义端口。
- 始终强制 profile 默认值并无视存储值：未采纳，因为这会丢弃真实的自定义端口，并破坏既有的“配置端口在 bind 失败时不得被改写”保证。

## Consequences

- `npm run tauri dev` 自动把共享配置中的 `17688` 解析为 `17689`，release 构建把 dev 写入的 `17689` 解析回 `17688`，两者无需手工修改即可同时监听。
- `0`、`17688`、`17689` 之外的自定义端口绝不会被改写，bind 失败仍报告配置端口且绝不回退到其他端口。
- 由于测试运行在 debug profile，API 网关测试现在会覆盖 dev 映射；终端同步接缝夹具使用端口 `19000` 以与规范映射保持独立。
- 服务商、本地 Key、启用标志与终端同步台账仍在两个 profile 间共享；两个应用同时运行时 `synced_base_url` 可能在两个端口间交替，并在另一个应用中显示为待同步，这是已接受且本次不解决的问题。
- 同一变更内 `MEMORY.md` 与 `docs/USAGE.md` 已描述按 profile 解析端口，导航索引因路径、归属与公共符号均未变化而保持不变。
