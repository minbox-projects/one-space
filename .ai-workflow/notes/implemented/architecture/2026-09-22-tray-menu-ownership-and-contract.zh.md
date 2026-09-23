# Agent Note: Tray menu ownership and contract

Status: implemented

[English](2026-09-22-tray-menu-ownership-and-contract.md) | 中文

## Problem

macOS 托盘菜单原本由 Rust 拥有：`create_tray_menu` 构建全部条目，`get_tray_label` 保存导航标签，前端通过 `update_tray_menu` 命令回推状态变化，点击则以 `tray-action` 事件到达。这种划分让导航 id 与双语文案在 IPC 边界两侧重复，因此每个目标或文案改动都需要 Rust 与前端各改一处，菜单无法在不重启的情况下跟随 i18n 语言切换，菜单中展示的实时服务状态（网关、路由、隧道、共享）也没有单一所有者。

## Decision

前端拥有菜单模型与全部文案；Rust 保留托盘创建与 OS 集成。

- `src/lib/trayMenu.ts` 拥有纯模型（`buildTrayMenuModel`、`isAcceleratorHint`）与原生应用器 `applyTrayMenu`：后者构建 `@tauri-apps/api/menu` 条目并对 `main` 托盘图标调用 `TrayIcon.setMenu`，API 不可用时软失败；全部标签来自 `src/i18n.ts`。
- `src/App.tsx` 承载控制器：从窗口可见性、网关与路由状态、隧道计数、共享状态与已配置快捷键初始化状态，在 `main-window-visibility-changed`、`api-gateway-status-update`、`protocol-router-status-update`、`ssh-tunnels-updated`、`file-sharing-updated` 与 i18n `languageChanged` 时重建菜单，并通过既有命令封装实现全部托盘动作。
- Rust 保留 OS 契约：托盘设置 `show_menu_on_left_click(false)`，只响应左键 Up 事件切换主窗口，右键打开原生菜单；webview 就绪前由两项双语启动兜底菜单（`get_fallback_tray_label`）继续显示或隐藏窗口与退出；`windows_data::emit_main_window_visibility` 发出 `main-window-visibility-changed`；`toggle_quick_ai_window` 命令打开快速 AI 窗口；`shutdown_runtime_services` 为托盘退出项、`quit_app` 与 `RunEvent::Exit` 提供同一条幂等清理路径。
- 移除的内部接口为 `create_tray_menu`、`get_tray_label`、`update_tray_menu`、`emit_tray_action`、`TrayActionPayload` 与 `tray-action` 事件；没有外部消费者依赖它们。
- `ssh_tunnels_connect_all` 与 `ssh_tunnels_disconnect_all` 支撑 Services 的批量动作，覆盖全部已保存隧道，复用分组批量内部实现与 `SshTunnelBatchOperationResult`，批次标识固定为 `ALL_TUNNELS_BATCH_ID = "all"` 与 `ALL_TUNNELS_BATCH_NAME = "All Tunnels"`。

## Alternatives considered

- 保留并扩展 Rust 标签表：未采纳，因为导航 id 与双语文案会继续在 Rust 与 TypeScript 之间重复，语言切换也仍需重启或额外的状态推送。
- 菜单全部在 Rust 构建、只转发动作：未采纳，因为每个目标或文案改动仍将是 Rust 改动，`src/i18n.ts` 也不再是唯一文案来源。
- 前端完全替换菜单且不保留启动兜底：未采纳，因为 webview 可能尚未就绪，托盘图标将没有可用菜单，前端加载前窗口也无法重新打开。
- 左键点击改为打开菜单而不是切换窗口：未采纳，因为已批准交互是左键切换、右键菜单，且 button-Up 过滤保证一次点击至多切换一次。
- 在托盘中提供逐隧道与逐共享控制：未采纳，因为托盘保持摘要级并用批量与停止动作，逐项控制仍留在应用内页面。

## Consequences

- 菜单结构、标签、启用与勾选只在一个 TypeScript 文件加 `src/i18n.ts` 中变化，新增或改名目标无需 Rust 改动；配置的快捷键字符串无法解析时，Show/Hide 与 Quick AI Session 的提示会被省略。
- Rust 公共接口收缩为托盘创建、启动兜底、可见性事件、`toggle_quick_ai_window` 与统一退出清理；被移除的命令与事件已无剩余调用方，过时调用会显式失败而不是静默无效。
- 应用内任何位置的语言切换与服务状态变化都会在不重启的情况下重建菜单；菜单读取与应用内页面相同的状态命令，因此展示状态会在下次重建时与页面一致。
- 失败动作经既有 toast 通道上报，且控制器在每次动作后重新查询状态，因此失败或部分失败的动作绝不会渲染为完全成功。
- 两次硬编码的双语启动兜底标签是 webview 接管前被接受的临时豁免；`MEMORY.md` 与导航索引在同一变更中记录该契约。
