# Agent 交付记录：内网穿透（Port Tunnel）工具实现

状态：已实现 (implemented)

[English](2026-10-21-port-tunnel-tool.md) | 中文

## 问题背景

开发者在使用 OneSpace 时，经常需要将本机开发端口（如 `http://localhost:3000` 或 API 接口服务）直接暴露到互联网公网，以便在手机端进行真机调试、给远程客户或同事预览、或对接第三方平台的 Webhook 回调（如微信公众号、飞书、GitHub、Stripe 等）。此前 OneSpace 缺乏原生的内网穿透工具，用户需要手动在终端中切换与配置第三方 CLI。

该功能的核心诉求包括：提供零配置免注册的极速通道（即开即用），同时支持国内开发者使用私有 VPS 低延迟的 FRP 模式；在界面上必须严格保持 OneSpace 桌面端的审美设计规范（精致卡片、状态呼吸灯、深浅色自适应、高反差手机扫码弹窗以及实时运行日志）；在后端生命周期上必须提供严格的子进程管理，防止应用退出后残留孤儿进程占用端口。

## 架构决策

我们在 OneSpace 中交付了双轨模式的“内网穿透（Port Tunnel）”工具，实现了前后端完整的闭环集成：

1. **工具箱注册表与导航**：
   - 在 `src/toolbox/plugins/port-tunnel.ts` 中注册 `port-tunnel` 描述符，使用 `Globe` 图标，默认在 Hub 与启动台快速工具中可见（`defaultOrder: 14`）。
   - 将 `port-tunnel` 添加到 `src/lib/navigation.ts` 的 `MoreToolsSection`，并在 `src/toolbox/registry.ts` 静态登记。
   - 在 `src/i18n.ts`、`en_keys.txt` 与 `zh_keys.txt` 中完整补充中英文双语词条。

2. **前端界面与用户体验**：
   - 交付 `src/components/PortTunnelTool/` 领域组件，采用 `ToolShell`、`ToolStatusBadge` 与呼吸微光指示点。
   - 打造醒目的公网地址卡片，集成一键复制（`useCopyToClipboard` 带有 1600ms 勾选反馈）、浏览器一键打开与手机相机扫码二维码弹窗（`QrCodeModal` 基于 `qrcode.react`）。
   - 实现依赖缺失提示横幅 `EnvironmentBanner`，提供一键复制包管理器命令（`brew install cloudflared`）与下载指引。
   - 实现折叠式黑曜石风格实时日志面板 `LogViewer`，展示带时间戳的标准输出与清空功能。
   - 实现 `TunnelConfigDialog`，支持 Cloudflare Quick Tunnel 零配置模式与自定义 FRP 模式，严格校验 1–65535 端口。

3. **后端 Tokio 进程监管器与 IPC 命令**：
   - 模块位于 `src-tauri/src/tunnel/`，划分 `types.rs`、`config.rs`、`environment.rs`、`supervisor.rs` 与 `commands.rs`。
   - 配置持久化于当前 Profile 目录下的 `tunnels.json`（`~/.config/onespace` 或 `~/.config/onespace-dev`），实现严格的 Profile 隔离。
   - 异步进程监管器启动 `cloudflared tunnel --url http://127.0.0.1:<port>` 或 `frpc -c <ini_path>`，通过正则匹配（`https://[a-zA-Z0-9-]+\.trycloudflare\.com`）实时提取公网 HTTPS URL。
   - 在 URL 识别与状态切换时广播 `tunnel-status-changed` 事件。
   - 在应用退出生命周期 `shutdown_runtime_services` 中注册 `tunnel::shutdown_all()` 清理钩子，确保子进程完全销毁。

## 验证事实

- **后端测试**：`cargo test --manifest-path src-tauri/Cargo.toml tunnel` 退出码 0，76 项测试全部通过（包含 8 项针对端口校验、FRP 规则、正则提取、反例与初始状态的单元测试）。
- **前端测试**：`npm test src/components/PortTunnelTool/index.test.tsx src/toolbox/registry.test.ts` 退出码 0，33 项测试全部通过。
- **代码规范**：`npx eslint src/components/PortTunnelTool/ src/lib/portTunnel.ts src/toolbox/plugins/port-tunnel.ts` 退出码 0，0 错误 0 警告。
- **生产构建**：`npm run build` 退出码 0，成功生成 `PortTunnelTool-*.js` 产物（22.19 kB，gzip 5.81 kB）。
- **导航索引验证**：`ai-workflow context validate --all` 退出码 0，`"valid": true, "errors": []`。
