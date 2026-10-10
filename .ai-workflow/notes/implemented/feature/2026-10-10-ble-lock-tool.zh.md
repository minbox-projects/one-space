# Agent Note: BLE Lock Tool Automates Mac Session Actions for One Bluetooth Device

Status: implemented

[English](2026-10-10-ble-lock-tool.md) | 中文

## Problem

OneSpace 此前没有原生方式在选定的低功耗蓝牙（BLE）设备远离时锁定 Mac、并在设备返回时唤醒。该工具必须依托项目现有的 macOS 工具链构建，不复制、不依赖、也不引用任何外部 BLE 解锁项目或其配置。它还必须遵守应用既有边界：前端拥有托盘菜单模型与共享运行时状态 store，配置不得进入同步存储，登录密码绝不能持久化到 macOS Keychain 之外。

所需行为横跨单设备 RSSI 存在状态机、按 profile 落位的配置、Keychain 密码存储、受辅助功能（Accessibility）门控的自动解锁、显示唤醒与系统休眠限制、尽力而为的媒体暂停/恢复、屏保/锁定/息屏结果、托盘控制以及本地化失败反馈。因此公开的命令与事件契约以及确切的数据边界必须与代码在同一变更中记录。

## Decision

该工具是独立原生实现，其前端表面、后端运行时与数据边界遵循冻结契约。

- 工具箱与 IPC 契约：`src/toolbox/plugins/ble-lock.ts` 在 `hub` 与 `launcher-quick` surface 注册 `ble-lock` 描述符，别名为 `ble-lock`，`loadComponent` 懒加载；`src/components/BleLockTool.tsx` 是工具 UI，`src/lib/bleLock.ts` 承载 DTO、`ble_lock_get_status`、`ble_lock_save_config`、`ble_lock_scan_start`、`ble_lock_scan_stop`、`ble_lock_select_device`、`ble_lock_clear_device`、`ble_lock_set_password`、`ble_lock_delete_password`、`ble_lock_request_accessibility` 与 `ble_lock_lock_now` 封装、`BLE_LOCK_STATUS_EVENT` / `BLE_LOCK_DEVICES_EVENT` 常量以及平台守卫。后端发出 `ble-lock-status-updated` 与 `ble-lock-devices-updated`，所有命令都返回 `BleLockStatus`，失败只以稳定的 `BleLockError` code 加安全 detail 上报，code 取自固定集合（`platform_unavailable`、`bluetooth_powered_off`、`bluetooth_unauthorized`、`bluetooth_unsupported`、`device_not_selected`、`device_unavailable`、`invalid_config`、`keychain_unavailable`、`password_not_configured`、`accessibility_required`、`system_sleep`、`native_action_unavailable`、`media_control_unavailable`）。
- 监测：`src-tauri/src/ble_lock/central.rs` 恰好拥有一个 CoreBluetooth UUID，在默认的主动模式下连接并周期性读取 RSSI，在被动模式下只消费广播、绝不建立连接；更换选择时取消或断开，对被替换设备丢弃过时回调，在非 macOS 目标上返回 `platform_unavailable`。
- 存在状态：`src-tauri/src/ble_lock/state.rs` 用最近至多五条有效读数的均值做平滑，当 RSSI 连续低于锁定阈值达到锁定延时后进入 away，当无有效读数达到无信号超时后进入 lost，每个 away/lost 片段至多发出一次锁定，有效恢复会取消待定锁定计时器，且只有在观察到 away/lost 之后又出现 near 跃迁时才武装解锁。
- 配置：`src-tauri/src/ble_lock/config.rs` 在当前构建 profile 的应用目录（Release 为 `~/.config/onespace`，Debug 为 `~/.config/onespace-dev`）持久化独立、不同步的 `ble_lock.json`，它不属于 `StorageConfig` 或同步、不含密码字段；写入前先校验，因此被拒绝的配置绝不会替换上一次有效字节，RSSI 阈值只接受 `-95` 到 `-30` 的 `5` 步进，两个 RSSI 动作与全部可选开关默认关闭，仅当两者都启用时要求 unlock RSSI 大于或等于 lock RSSI，锁定延时接受 `1..=3600` 秒、无信号超时接受 `5..=3600` 秒。
- 密码与 Keychain：`src-tauri/src/ble_lock/native.rs` 仅以当前用户的 generic-password Keychain 条目存储登录密码，Release 使用 service `com.onespace.desktop.ble-lock`、Debug 使用 `com.onespace.desktop.ble-lock.dev`，均为不可同步存储，对外只暴露已配置/未配置状态。
- 自动解锁门控：解锁只作用于当前已锁定的普通用户会话，且仅在已配置密码、辅助功能已信任、Keychain 条目可读、解锁阈值已启用并发生 near 跃迁时执行；每个合格跃迁至多尝试一次注入，错误密码不会形成重试循环，且「唤醒但不解锁」会抑制注入。
- 原生接缝与能力门控：`BleLockPlatform` 是唯一 OS 接缝，控制器把每种 macOS 能力隔离在接缝之后并标注其真实可用性——辅助功能信任查询、一次性 CGEvent 解锁与 IOPM user-activity 断言是公开 API，而普通登录会话锁定状态观察、尽力而为的 MediaRemote 暂停/播放以及私有的 locked-key/显示接缝是私有或未公开的；媒体控制只对确认正在播放的媒体生效、并且只恢复当前锁定周期内被暂停的内容，能力不可用时上报其稳定 code 并继续锁定，未被观察到的结果绝不报告为成功。
- 生命周期：`src-tauri/src/app_runtime/run_app.rs` 通过 `#[path = "../ble_lock/native.rs"] pub(crate) mod ble_lock_native;` 注册原生模块并只安装一次进程级运行时，系统唤醒接缝在休眠后重建监视器、连接与扫描，`shutdown_runtime` 经既有幂等的 `shutdown_runtime_services` 路径执行 BLE 清理。
- 前端运行时与托盘：`src/lib/runtimeStatus.ts` 的 `ble-lock` 服务消费带类型的 status 快照，`src/App.tsx` 事件桥订阅 `BLE_LOCK_STATUS_EVENT`；前端所有的 `src/lib/trayMenu.ts` 新增 `ble-lock-controls` 子菜单，含 status、Lock Now、Set Device、Unlock RSSI、Lock RSSI、Wake on Proximity、Wake without Unlocking、Pause while Locked、Use Screensaver 与 Turn Off Display 项；在 macOS Tauri 运行时之外，工具与托盘控制都保持可见，但解析为不可用快照且绝不调用原生命令。
- 手动优先：Lock Now 独立于 proximity 直接锁定，仍在近处的设备无法解锁，直到 OneSpace 观察到 away 或 lost、随后又出现新的 near 跃迁才恢复解锁。

## Alternatives considered

- 复用或复制外部 BLE 解锁项目、其配置或其依赖：未采纳，因为该功能必须是独立的 CoreBluetooth 实现，不含任何外部地址、依赖、更新检查或参考配置。
- 把 BLE 设置存入同步的 `StorageConfig` 或共享 profile schema：未采纳，因为设备选择与阈值是本机偏好，不应跨设备同步。
- 把登录密码存入 OneSpace 配置、Secrets 文件、日志、前端存储或事件负载：未采纳，因为密码是 macOS 凭据，只应存在于不可同步的 Keychain 条目中，且工具绝不能返回它。
- 支持多个受监测设备或从 RSSI 推断物理距离：未采纳，因为契约最多监测一个设备，绝不解码蓝牙 MAC 地址，也不报告以米为单位的距离。
- 把控制项做进 Rust 托盘菜单：未采纳，因为托盘模型归前端所有；用户要求的 `ble-lock-controls` 子菜单改为作为既有契约的有界扩展交付。
- 直接调用 macOS 锁定、媒体与显示 API 并把一次调用视为成功：未采纳，因为这些能力可能缺失或属于私有；每一项都隔离在 OS 接缝之后，上报稳定的能力错误，并通过观察到的状态确认。
- 承诺 macOS 系统休眠期间的检测或整机唤醒：未采纳，因为系统休眠会暂停 proximity 判定、只唤醒显示；监视在唤醒后重建。
- 让近处设备在手动锁定后立即重新解锁：未采纳，因为手动锁定必须优先，直到观察到 away 或 lost、随后又出现 near 跃迁才重新武装解锁。

## Consequences

- 交付的公开路径与符号记录在 `MEMORY.md` 与权威 `.ai-workflow/index/navigation.json` 中，作为前端 `ble-lock` 与后端 `ble-lock-backend` 两个 feature；`docs/USAGE.md` 与 `README.md` 用中文记录面向用户的设置与边界。
- 数据边界成立：`ble_lock.json` 只位于 profile 应用目录，不属于 `StorageConfig` 或同步；Keychain 条目为当前用户、不可同步，并使用按 profile 区分的 service 名；配置、同步数据、前端存储、日志、事件或错误中都不出现密码。
- 平台与失败行为明确：非 macOS 构建、浏览器预览或非 Tauri WebView 保持入口可见并显示 `platform_unavailable`，零原生调用；某一能力失败绝不阻塞仍然可用的独立配置、状态或手动动作。
- 托盘与工具读取同一 status 快照，托盘的阈值与开关勾选跟随该快照，App 动作处理器施加屏保/息屏互斥，因此工具与托盘消费同一共享快照。
- 手动锁定保持优先：Lock Now 之后仍在近处的设备不会解锁，只有 away 或 lost 之后又 near 才恢复资格。
- 本次交付记录的验证：`cargo test --manifest-path src-tauri/Cargo.toml ble_lock --lib` 在初始 RED、主动 RSSI、原生告警、阈值、设备、休眠与异步媒体回归之后退出 0，44 通过、0 失败、0 ignored（过滤 1241）；有界九文件前端运行在四个平台/l10n 回归之后退出 0，331 个测试、9 个文件；`npm run lint` 退出 0，0 error、409 既有 warning，新文件 0 problem；`npm run build` 退出 0，产出与 App chunk 分离的懒加载 `BleLockTool-*.js` chunk（12.02 kB，gzip 3.39 kB）。
- 打包证据：`npm run tauri -- build --debug --bundles app` 先退出 1（app bundle 已构建完成，但 updater 签名需要 `TAURI_SIGNING_PRIVATE_KEY`）；随后有界本地重试 `--config '{"bundle":{"createUpdaterArtifacts":false}}'` 退出 0、完成 1 个 bundle，该选择只作用于命令、未持久化且未改源码；`plutil -lint` 以及对新生成的 `target/debug/bundle/macos/OneSpace.app/Contents/Info.plist` 的提取退出 0，`NSBluetoothAlwaysUsageDescription` 恰为字符串 "OneSpace uses Bluetooth to detect when your selected device moves away or returns, so it can lock or wake your Mac."；debug 构建在 `native.rs` 携带 1 条 dead-code warning，无单测 warning；普通构建在跳过 updater 产物前先尝试了 updater 签名，而 debug app 仅做 ad hoc；未进行签名 release 或 publish。
- macOS 冒烟清单未运行，也不标记为通过：蓝牙 TCC 提示与关闭/拒绝状态、主动与被动 RSSI、Keychain 保存/读取/删除及 Debug/Release 隔离、辅助功能、普通 Lock Now 与受保护解锁、屏保与息屏、MediaRemote 播放/未播放/不可用、以及休眠恢复与重复退出清理，每一项都只是带预期结果的手动流程。辅助功能信任查询、一次性 CGEvent 解锁与 IOPM user-activity 断言是公开 API，而普通登录会话锁定观察、MediaRemote 控制与部分显示接缝是私有或未公开的，因此这些能力缺口保持记录、不声称已验证。
- Supersession：无全部或部分取代。[Tray menu ownership and contract](../architecture/2026-09-22-tray-menu-ownership-and-contract.md) 决策继续有效，仅被有界的 `ble-lock-controls` 子菜单扩展；[Toolbox Plugin Registry Replaces Hand-Maintained Tool Lists](../architecture/2026-09-25-toolbox-plugin-registry.md) 决策继续有效，因为 `ble-lock` 只是又一个从注册表派生 Hub、Launcher 与别名表面的描述符。
