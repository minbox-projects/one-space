# Agent Note: BLE Lock 重试失败连接并仅在显式用户发现时打开设备选择器

Status: implemented

[English](2026-10-12-ble-lock-reconnect-and-device-discovery.md) | 中文

## Problem

在 [BLE Lock Tool Automates Mac Session Actions for One Bluetooth Device](../feature/2026-10-10-ble-lock-tool.md) 交付后，仍存在两个「设备返回」缺陷。后端方面，运行时只在 `Disconnected` 回调中重试连接，因此落在 5000 毫秒重试窗口内的快速连接失败被丢弃、没有周期 tick 重试，即便设备已返回连接也可能保持断开；手动重新扫描（不重选设备）是当时唯一的恢复方式。前端方面，设备选择器显示的扫描状态读取自原生监视器的 `status.scanning`，而该标志在被动广播监测时同样为真，并且后台设备事件会自行打开选择器；因此用页脚按钮、对话框关闭控件或 Escape 关闭选择器时，用户发现可能仍在运行或被错误呈现。

## Decision

- 后端重试：`state.rs` 新增 `RECONNECT_RETRY_INTERVAL_MS = 5_000` 与 `last_connect_retry_ms`。当选择为主动、无线电状态为 `PoweredOn` 且最近错误为 `device_unavailable` 时，`BleLockMonitor::tick` 发出 `BleLockEffect::Connect`，并节流为每 5000 毫秒至多一次（尚未记录重试时立即进行首次重试）。重试绝不重置存在状态历史、计时器或手动锁定资格，`reset_history` 会清除重试时间戳，因此快速失败不会忙循环，窗口之后的下一次 tick 无需重新扫描或重选即可再次重试。
- 后端派发与桥接：`native.rs` 把发出的 `Connect` 收集到 `retry_connect`，释放运行时锁后复用 background-aware 的 `hardware_plan` 经 `dispatch_sync_central` 派发，因此主线程派发发生在运行时锁之外；`Disconnected` 回调只记录 `device_unavailable`，重试归周期 tick 负责。`central.rs` 的 `select` 会比较请求 id 与当前选择，因此重复应用同一 UUID 不会取消进行中的请求（CoreBluetooth 待定连接没有超时），而设备替换仍会取消；`connect_selected` 只在设备状态为 `Disconnected` 时发起 `connectPeripheral:`，保留等待中的 `Connecting` 与已建立状态，`request_rssi` 只对 `Connected` 设备调用 `readRSSI`。
- 监视器重建后的无线电状态同步：`dispatch_sync_central` 读取保留管理器的真实 `central.state()` 一次，并在应用模式与连接之前调用 `guard.monitor.set_bluetooth_state(state)`，因此同设备同模式保存导致监视器重建为 `Unknown` 时，PoweredOn 重试守卫不会被永久压制。
- 恢复：一个有效的、已选主动 `ConnectionRead` 只清除 `device_unavailable` 这一最近错误；无关的稳定错误在恢复读数后仍然保留。
- 前端用户发现：`BleLockTool.tsx` 新增本地 `userScanning` 标志，与原生 `status.scanning` 分离。选择器只在用户显式扫描/查找/更换设备时打开，该动作设置 `userScanning`、打开对话框并调用 `bleLockScanStart`；`ble-lock-devices-updated` 事件只刷新设备列表，绝不打开选择器。页脚关闭按钮、对话框关闭控件、Escape 与选中成功都经既有 `bleLockScanStop` 结束用户发现，选择器关闭后延迟的 `bleLockScanStart` 结算不会重新打开选择器或恢复 `userScanning` 指示，不过其状态快照仍会被消费。被动后台监测按配置保留自己的扫描，不受 `userScanning` 支配。
- 无新增公开契约：命令、事件、`BleLockStatus`、`BleLockConfig` 与固定错误码集合均不变。

## Alternatives considered

- 依赖用户手动重新扫描（不重选）来恢复连接。未采纳，因为持久化的选择必须在设备返回时自行恢复；手动重新扫描是缺陷本身，而不是预期工作流。
- 在每次监测 tick 取消并重启待定连接。未采纳，因为 CoreBluetooth 待定连接没有超时，取消会放弃进行中的请求，周期性的取消/重启可能打断一个即将完成的连接；合法的清除、切到被动模式、设备替换或关闭仍会取消。
- 从整体硬件扫描标志或设备事件派生选择器可见性与用户发现生命周期。未采纳，因为原生监视器在被动广播监测时已经上报 `status.scanning` 并持续推送设备事件，选择器会在没有用户意图时打开；必须使用显式的 `userScanning` 意图。

## Consequences

- 被传输层上报为不可用的已选主动设备现在会自动恢复：无线电开启期间监视器至多每五秒重试一次连接，快速失败无法停止重试，设备返回后有效的连接读数会清除瞬时的 `device_unavailable` 错误且不干扰无关错误。
- 用户的选择与已监测存在状态在中断期间无需重选即可保留，手动锁定资格不受重试影响。
- 因同设备同模式保存而重建的监视器会立即获得保留管理器的真实无线电状态，因此只要无线电开启，PoweredOn 重试守卫就会恢复，而不会在 `Unknown` 重建后一直停用。
- 选择器只在显式用户发现时打开；后台设备更新会刷新列表但不打开它，页脚按钮、对话框关闭控件、Escape 与选中都经 `bleLockScanStop` 恰好一次停止用户发现，同时被动监测保持不变。
- 公开契约不变：未新增、重命名或移除任何命令、事件、状态字段、配置字段或错误码。
- 本次交付记录的验证：有界 Rust 运行 `cargo test --manifest-path src-tauri/Cargo.toml ble_lock --lib` 退出 0，49 通过、0 失败、0 ignored；`cargo check --manifest-path src-tauri/Cargo.toml --lib` 退出 0 且无 warning；一条回归覆盖无需重新扫描或重选的自动重试，并在修复前先因缺失的自动 `Connect` 失败。前端运行 `npm test -- src/components/BleLockTool.test.tsx src/lib/bleLock.test.ts` 退出 0，共 33 个测试（组件 19、封装 14），覆盖 `status.scanning` 为真时仍能开始被动发现、选中后在被动扫描仍在的情况下停止并关闭发现，以及关闭后延迟的 `bleLockScanStart` 结算不会重新打开选择器；`tsc` 与有界 lint 退出 0。完整构建与真实 BLE 离开—返回未被执行，也未声称任何真实硬件验证。
- Supersession：无全部或部分取代。本记录是一处有界修正，补充 [BLE Lock Tool Automates Mac Session Actions for One Bluetooth Device](../feature/2026-10-10-ble-lock-tool.md)；该笔记的决策与保证继续有效且适用。
