# Agent Note: Port Tunnel Tool for Public Port Forwarding

Status: implemented

[English](2026-10-21-port-tunnel-tool.md) | 中文

## Problem

使用 OneSpace 的开发者经常需要把本机开发端口（如 `http://localhost:3000` 或 API 服务）暴露到公网，用于手机真机预览、客户演示或外部 Webhook 集成（微信、飞书、GitHub、Stripe）。此前 OneSpace 没有原生内网穿透能力，开发者只能手动切换到第三方 CLI 或外部隧道。

方案需要零配置、免费，无需强制注册账号或自建服务器即可立即分享 Web 开发内容，同时支持需要低延迟和自定义域名的开发者使用自己 VPS 上的私有 FRP 服务器。界面还必须遵循 OneSpace 桌面端设计规范（卡片、微光状态指示、深浅色主题、手机相机扫码弹窗和实时进程日志），后端进程生命周期必须保证应用退出后不残留孤儿后台进程。

## Decision

我们在 OneSpace 中交付了双模式的内网穿透（Port Tunnel）工具（`port-tunnel`），并完成前后端完整集成。

1. **工具箱注册表与导航**：
   - 在 `src/toolbox/plugins/port-tunnel.ts` 中注册 `port-tunnel`，使用 `Globe` 图标（`bg-emerald-500/10 text-emerald-600`），在 `hub` 与 `launcher-quick` 两个入口均可用（`defaultOrder: 14`）。
   - 将 `port-tunnel` 加入 `src/lib/navigation.ts` 的 `MoreToolsSection`，并在 `src/toolbox/registry.ts` 中注册。
   - 在 `src/i18n.ts`、`en_keys.txt` 与 `zh_keys.txt` 中补充完整的中英双语词条。

2. **前端界面与用户体验**：
   - 交付 `src/components/PortTunnelTool/`，使用 `ToolShell`、`ToolStatusBadge` 与微光状态指示。
   - 构建公网地址主卡片，支持一键复制（通过 `useCopyToClipboard`，带 1600ms 勾选反馈）、浏览器打开按钮，以及手机相机扫码弹窗（`QrCodeModal`，基于 `qrcode.react`）。
   - 实现依赖缺失提示 `EnvironmentBanner`，支持一键复制包管理器命令（`brew install cloudflared`）并提供下载指引。
   - 实现可折叠的 `LogViewer`，显示带时间戳的实时 stdout/stderr 行并提供清空操作。
   - 构建 `TunnelConfigDialog`，支持 Cloudflare Quick Tunnel 与 Custom FRP 两种模式，并严格校验端口（1–65535）。

3. **后端 Tokio 进程监管器与 IPC**：
   - 模块 `src-tauri/src/tunnel/`，包含 `types.rs`、`config.rs`、`environment.rs`、`supervisor.rs` 与 `commands.rs`。
   - 将隧道定义持久化到 `get_app_dir()`（`~/.config/onespace` 或 `~/.config/onespace-dev`）下按 profile 隔离的 `tunnels.json`。
   - 异步进程监管器启动 `cloudflared tunnel --url http://127.0.0.1:<port>` 或 `frpc -c <ini_path>`，流式读取 stdout/stderr，并通过正则（`https://[a-zA-Z0-9-]+\.trycloudflare\.com`）在数秒内提取分配的公网 HTTPS URL。
   - 在 URL 发现与状态变化时发出 `tunnel-status-changed` 事件。
   - 在 `src-tauri/src/app_runtime/shortcuts_tray.rs`（`shutdown_runtime_services`）注册关闭钩子，调用 `tunnel::shutdown_all()`，确保应用退出时终止所有子进程。

## Alternatives considered

- **继续手工切换第三方 CLI 或外部隧道**：这是工具出现前的既有方式。它被否决，因为每次分享都需要离开 OneSpace 并手工配置外部工具，而交付的双模式工具在同一处同时覆盖零配置 Quick Tunnel 分享与私有 FRP 配置。

## Consequences

- 工具依赖已安装对应 CLI（Quick Tunnel 需要 `cloudflared`，Private FRP 模式需要 `frpc` 及私有 FRP 配置），缺失依赖由 `EnvironmentBanner` 提示。
- Quick Tunnel 无需自建服务器，而 Private FRP 模式需要开发者自己的 FRP 服务器配置。
- 隧道定义保存在按 profile 隔离的 `tunnels.json` 中，不同 profile 不共享隧道定义。
- 监管器持有子进程，应用退出时关闭钩子调用 `tunnel::shutdown_all()`，因此隧道进程不会在应用退出后继续运行。

## Verification

- **后端**：`cargo test --manifest-path src-tauri/Cargo.toml tunnel` 退出码 0，76 项测试通过（包含 8 项新增的 `tunnel::tests`，覆盖配置校验、FRP 要求、URL 正则提取、反例、序列化以及监管器初始状态）。
- **前端注册表与组件**：`npm test src/components/PortTunnelTool/index.test.tsx src/toolbox/registry.test.ts` 退出码 0，2 个测试文件共 33 项测试通过。
- **前端代码规范与 Lint**：`npx eslint src/components/PortTunnelTool/ src/lib/portTunnel.ts src/toolbox/plugins/port-tunnel.ts` 退出码 0，0 错误 0 警告。
- **生产构建**：`npm run build` 退出码 0，成功编译 TypeScript 并生成 `PortTunnelTool-*.js` 产物（22.19 kB，gzip 5.81 kB）。
- **架构与导航索引**：`ai-workflow context validate --all` 退出码 0，`"valid": true, "errors": []`。
