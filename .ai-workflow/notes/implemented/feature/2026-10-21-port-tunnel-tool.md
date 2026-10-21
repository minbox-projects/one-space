# Agent Note: Port Tunnel Tool for Public Port Forwarding

Status: implemented

English | [中文](2026-10-21-port-tunnel-tool.zh.md)

## Problem

Developers using OneSpace frequently need to expose local development ports (e.g., `http://localhost:3000` or API services) to the public internet for remote mobile preview, customer demonstrations, or external webhook integrations (WeChat, Feishu, GitHub, Stripe). Previously, OneSpace had no native port tunneling capability, requiring developers to switch to third-party CLIs or external tunnels manually.

The solution needed to be zero-config and free for immediate web development sharing without mandatory server setup, while also supporting advanced developers who require private FRP servers with low latency and custom domains. Furthermore, the UI had to adhere strictly to OneSpace's desktop aesthetic guidelines (cards, subtle glow indicators, dark/light theme, QR code modals for mobile cameras, and real-time process logs), and the backend process lifecycle had to guarantee that no orphaned background processes remain on application exit.

## Decision

We delivered a dual-mode Port Tunnel tool (`port-tunnel`) in OneSpace with complete frontend-backend integration.

1. **Toolbox Registry & Navigation**:
   - Registered `port-tunnel` in `src/toolbox/plugins/port-tunnel.ts` with the `Globe` icon (`bg-emerald-500/10 text-emerald-600`), available on both `hub` and `launcher-quick` surfaces (`defaultOrder: 14`).
   - Added `port-tunnel` to `MoreToolsSection` in `src/lib/navigation.ts` and registered in `src/toolbox/registry.ts`.
   - Complete bilingual translation keys added in `src/i18n.ts`, `en_keys.txt`, and `zh_keys.txt`.

2. **Frontend UI/UX**:
   - Delivered `src/components/PortTunnelTool/` using `ToolShell`, `ToolStatusBadge`, and glowing status indicators.
   - Built the active public URL hero card with one-click copy feedback (via `useCopyToClipboard`), browser open button, and mobile camera QR code modal (`QrCodeModal` using `qrcode.react`).
   - Implemented `EnvironmentBanner` for missing dependencies with one-click package manager copy (`brew install cloudflared`).
   - Implemented collapsible `LogViewer` displaying real-time timestamped stdout/stderr lines with clear action.
   - Built `TunnelConfigDialog` supporting both Cloudflare Quick Tunnel and Custom FRP modes with strict port validation (1–65535).

3. **Backend Tokio Supervisor & IPC**:
   - Module `src-tauri/src/tunnel/` with `types.rs`, `config.rs`, `environment.rs`, `supervisor.rs`, and `commands.rs`.
   - Persists tunnel definitions in profile-isolated `tunnels.json` under `get_app_dir()` (`~/.config/onespace` or `~/.config/onespace-dev`).
   - Async process supervisor launches `cloudflared tunnel --url http://127.0.0.1:<port>` or `frpc -c <ini_path>`, streams stdout/stderr, and uses regex matching (`https://[a-zA-Z0-9-]+\.trycloudflare\.com`) to extract the assigned public HTTPS URL within seconds.
   - Emits `tunnel-status-changed` events upon URL discovery and state changes.
   - Registered shutdown hook in `src-tauri/src/app_runtime/shortcuts_tray.rs` (`shutdown_runtime_services`) calling `tunnel::shutdown_all()`, guaranteeing all child processes are killed on app exit.

## Verification

- **Backend**: `cargo test --manifest-path src-tauri/Cargo.toml tunnel` exited 0 with 76 passed tests (including 8 new `tunnel::tests` covering config validation, FRP requirements, URL regex extraction, negative cases, serialization, and supervisor initial state).
- **Frontend Registry & Components**: `npm test src/components/PortTunnelTool/index.test.tsx src/toolbox/registry.test.ts` exited 0 with 33 passed tests in 2 test files.
- **Frontend Code Standards & Lint**: `npx eslint src/components/PortTunnelTool/ src/lib/portTunnel.ts src/toolbox/plugins/port-tunnel.ts` exited 0 with 0 errors and 0 warnings.
- **Production Build**: `npm run build` exited 0, successfully compiling TypeScript and generating `PortTunnelTool-*.js` chunk (22.19 kB, gzip 5.81 kB).
- **Architecture & Navigation Index**: `ai-workflow context validate --all` exited 0 with `"valid": true, "errors": []`.
