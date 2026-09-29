# Agent Note: Dev and Release Builds Isolate Application State and AI Gateway Terminal Identity

Status: implemented

[English](2026-09-29-dev-release-environment-isolation.md) | 中文

## Problem

debug（`tauri dev`）与已安装的 release 构建都把 `get_app_dir()` 解析为 `~/.config/onespace`，因此共享同一份可变应用数据：加密的 `ai_gateway.json` 及其上游服务商、本地 Key 与终端同步台账，存储根，skills/subagents 本地缓存，以及 AI Environments legacy providers 回退。此前的[端口决策](2026-09-22-gateway-build-profile-port.md)只隔离了监听端口，并明确接受服务商、本地 Key、启用标志与台账仍然共享，因此 dev 的终端同步可能把 release 的 OpenCode `gateway` 服务商记录改写成 `http://127.0.0.1:17689/v1`，dev 实验也会作用在真实 release 状态上。

## Decision

- **构建 profile**：`config::is_dev_build()` 仅对 `cfg(all(debug_assertions, not(test)))` 为真；`config::app_dir_for(home, is_dev)` 对 dev 返回 `~/.config/onespace-dev`、对 release 返回 `~/.config/onespace`，绑定的 `get_app_dir()` 让所有应用托管路径都由它派生。单测编译保持 release 解析，既有测试夹具不发生位移。
- **启动播种**：`config::seed_dev_gateway_files_on_start()` 在 `run()` 顶部、CLI 处理之前运行；每次 debug 启动时 `config::seed_gateway_files(release_dir, dev_dir)` 先复制 `.local_key`（仅当 release 源存在且 dev 目标缺失），随后仅当 `ai_gateway.json` 目标缺失且两端 `.local_key` 字节完全一致时才复制它。它绝不覆盖已存在的 dev 文件、绝不复制用量数据库、失败仅记录日志以便下次启动重试，并且绝不写 release 目录。
- **按 profile 派生存储与缓存**：`config::default_storage_type_for(is_dev)` 让未配置 `config.json` 的 dev 构建默认使用 `local` 存储，绝不使用 release profile 的 macOS iCloud 默认；local/shared 存储根、local_data 镜像（`ensure_local_data_mirror_initialized_at`）、skills/subagents 本地缓存以及 AI Environments legacy providers 回退都解析在 profile 应用目录下。
- **终端身份**：`TerminalSyncProfile`（`RELEASE`、`DEV`、`current()`）决定写出的身份与标记认领：release 拥有 OpenCode `provider_key = gateway`、名称 `AI Gateway` 与布尔 `ai_gateway_gateway = true` 标记，dev 拥有 `gateway-dev`、`AI Gateway (Dev)` 与字符串 `"dev"` 标记。每个 profile 只认领自己的标记，把另一 profile 的记录与 legacy 改名标记视为外来，legacy 归 release 所有。既有 OpenCode 投影按 provider key 作用域写入，因此 dev 同步绝不会创建、更新或删除 `provider.gateway`；Codex 注册使用不同 provider id，Codex 激活保持手动。
- **端口不变**：release 仍解析 `17688`、dev 仍解析 `17689`；`resolve_port` 继续按 profile 翻译两个规范默认值，因为首次播种或 legacy 共享历史可能带入另一 profile 的默认值，自定义端口保持原样。
- **范围外**：AI Environments 服务商投影、MCP 投影、`~/.agents/skills` 内容、显式选择的 iCloud 或自定义存储路径、OS 级快捷键与托盘冲突，以及 Windows-only 消息存储回退仍然共享且保持不变。

## Alternatives considered

- 完全不做播种、让两个 profile 彻底分离：未采纳，因为开发会话会以空网关启动，开发者需要重新创建服务商与 Key，而不是验证真实配置。
- 完整复制 release 目录，或复制包括用量数据库在内的更多文件：未采纳，因为 dev 不需要 release 的用量历史，无条件复制还会覆盖 dev 的应用内编辑；只播种两个网关核心文件，且仅在缺失时复制。
- 用硬链接或符号链接共享 `ai_gateway.json` 而不是复制一次：未采纳，因为任一 profile 的写入都会改到另一份文件，重新制造本次要消除的共享可变状态。
- 让 dev 复用 release 的 iCloud 存储路径：未采纳，因为 dev 会把测试数据写进云同步的 release 目录并可能传播到其他设备；改为范围外保留显式选择的 iCloud/自定义路径。
- 继续共享 `gateway` 终端服务商身份：未采纳，因为两个 profile 会争抢并覆盖同一条 OpenCode 服务商记录，现在每个 profile 只读写自己的记录。

## Consequences

- `npm run tauri dev` 创建并使用 `~/.config/onespace-dev`；首次 debug 启动从 `~/.config/onespace` 播种 `.local_key` 与 `ai_gateway.json`，删除 dev 目录或某个已播种的网关文件后，下次启动只恢复缺失的那一个文件。
- release 目录绝不会被播种写入；release 保持自己的目录、端口、存储默认与 `gateway` 记录，既有布尔标记与 legacy 标记的终端记录仍归 release 所有。
- 两个构建可以同时运行：各自网关监听各自端口，各自终端投影只以原子写入触碰自己的 provider key，因此一份 OpenCode 配置可以同时持有 `provider.gateway` 与 `provider.gateway-dev`；release 的下一次同步会把仍指向 `http://127.0.0.1:17689/v1` 的过期 `provider.gateway` 修复回自己的地址。
- Codex 保持手动激活；之后激活 dev provider 只会投影它自己的 `model_providers.onespace_<id>` 条目，release 记录在手动变更前保持不变。
- [端口决策](2026-09-22-gateway-build-profile-port.md)的共享状态后果被取代：服务商、本地 Key、启用标志与终端同步台账不再在两个 profile 间共享，而其按 profile 的 `resolve_port` 翻译继续有效。
- `MEMORY.md`、`README.md` 与 `docs/USAGE.md` 在同一变更中描述隔离后的 dev 目录、播种规则与两种服务商身份，导航索引登记 profile 应用目录 helper 与终端 profile 值。
