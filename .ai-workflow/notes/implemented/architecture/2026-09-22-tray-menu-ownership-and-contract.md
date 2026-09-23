# Agent Note: Tray menu ownership and contract

Status: implemented

English | [中文](2026-09-22-tray-menu-ownership-and-contract.zh.md)

## Problem

The macOS tray menu was Rust-owned: `create_tray_menu` built every item, `get_tray_label` held the navigation labels, the frontend pushed state changes back through the `update_tray_menu` command, and clicks arrived as `tray-action` events. That split duplicated navigation ids and bilingual labels on both sides of the IPC boundary, so every destination or wording change needed a Rust edit plus a frontend edit, the menu could not follow an i18n language change without a restart, and the live service state shown in the menu (gateway, router, tunnels, sharing) had no single owner.

## Decision

The frontend owns the menu model and every text; Rust keeps tray creation and the OS integration.

- `src/lib/trayMenu.ts` owns the pure model (`buildTrayMenuModel`, `isAcceleratorHint`) and the native applicator `applyTrayMenu`, which builds `@tauri-apps/api/menu` items and calls `TrayIcon.setMenu` on the `main` tray icon, failing soft when the API is unavailable; all labels come from `src/i18n.ts`.
- `src/App.tsx` hosts the controller: it seeds state from window visibility, gateway and router status, tunnel counts, sharing status and the configured shortcuts, rebuilds the menu on `main-window-visibility-changed`, `api-gateway-status-update`, `protocol-router-status-update`, `ssh-tunnels-updated`, `file-sharing-updated` and i18n `languageChanged`, and implements every tray action through the existing command wrappers.
- Rust keeps the OS contract: the tray sets `show_menu_on_left_click(false)`, reacts only to the left-button Up event by toggling the main window and opens the native menu on right click; before the webview is ready a two-item bilingual bootstrap fallback menu (`get_fallback_tray_label`) still shows or hides the window and quits; `windows_data::emit_main_window_visibility` emits `main-window-visibility-changed`; the `toggle_quick_ai_window` command opens the quick AI window; and `shutdown_runtime_services` gives the tray Quit item, `quit_app` and `RunEvent::Exit` one idempotent cleanup path.
- The removed internal surface is `create_tray_menu`, `get_tray_label`, `update_tray_menu`, `emit_tray_action`, `TrayActionPayload` and the `tray-action` event; no external consumer relied on them.
- `ssh_tunnels_connect_all` and `ssh_tunnels_disconnect_all` back the Services batch actions for every saved tunnel, reusing the group batch internals and `SshTunnelBatchOperationResult` with the stable batch identity `ALL_TUNNELS_BATCH_ID = "all"` and `ALL_TUNNELS_BATCH_NAME = "All Tunnels"`.

## Alternatives considered

- Keeping the Rust label table and extending it: declined because navigation ids and bilingual text would stay duplicated across Rust and TypeScript, and language changes would still need a restart or an extra state push.
- Building the menu in Rust and forwarding only the resulting actions: declined because every destination or wording change would remain a Rust change and `src/i18n.ts` would stop being the single text source.
- Replacing the menu from the frontend with no bootstrap fallback: declined because the webview may not be ready yet, so the tray icon would have no usable menu and the window could not be reopened before the frontend loads.
- Making a tray left click open the menu instead of toggling the window: declined because the approved interaction is left click to toggle with a right-click menu, and the button-Up filter keeps one click to at most one toggle.
- Putting per-tunnel and per-share controls in the tray: declined because the tray stays summary-level with batch and stop actions while the in-app pages keep the per-item controls.

## Consequences

- Menu structure, labels, enablement and check marks change in one TypeScript file plus `src/i18n.ts`, so adding or renaming a destination needs no Rust change; the Show/Hide and Quick AI Session accelerator hints are omitted when the configured shortcut string does not parse.
- The Rust public surface shrinks to tray creation, the bootstrap fallback, the visibility event, `toggle_quick_ai_window` and the unified shutdown; the removed command and event have no remaining callers, and a stale caller fails explicitly instead of silently doing nothing.
- Language changes and service state changes made anywhere in the app rebuild the menu without a restart; the menu reads the same status commands as the in-app pages, so the displayed state follows the pages on the next rebuild.
- Failed actions surface through the existing toast channel and the controller re-queries state after every action, so a failed or partially failed action never renders as fully successful.
- The two hardcoded bilingual bootstrap labels remain an accepted transient startup exemption until the webview replaces the fallback menu; `MEMORY.md` and the navigation index record this contract in the same change.
