# Agent Note: SSH Tunnel Reconnect Resilience Uses a Persistent Supervisor, a Round-Trip Probe, a Desired-Running Watchdog and a Wake/Window Retry Poke

Status: implemented

[English](2026-09-30-ssh-tunnel-reconnect-resilience.md) | 中文

## Problem

此前的 SSH 隧道运行时是一次性的：已连接的转发线程在链路断开时结束，启动路径只暴露错误而不再继续监管，因此一次暂时性故障、半开链路或系统休眠都会让隧道停摆，直到用户手动重连。仅 keepalive 的健康检查在黑洞链路上仍返回成功，唤醒与窗口显示不会打断进行中的等待，意外结束的运行时线程也永远不会被重启。因此自动恢复必须做到：对可重试失败持续重试、区分死掉的传输与不可达的转发目标、只在用户仍希望运行且不存在监管实例时重新启动隧道、并对只有用户能修复的失败停止重试——且不改变持久化格式、任何命令签名或任何事件契约。

## Decision

- `spawn_runtime_thread` 让 local、remote、dynamic 三种运行时全部跑在 `forwarding_runtime/supervisor.rs` 的持久监管循环下。每个运行时返回 `RuntimeOutcome`（`Stopped`、`FailedAtStartup { kind, message }`、`DroppedAfterConnected { kind, message }`）与 `FailureKind`（`Transport`、`Auth`、`HostKey`、`Config`、`Port`、`Target`）；`is_retryable` 把传输、端口与目标视为可重试，把鉴权、主机密钥与配置视为终态。除 `Stopped` 之外的每个结果都会持久化 `last_error`、以去重键 `ssh-tunnels:{category}:{id}` 记录恰好一条去重的 `auto-connect`（`FailedAtStartup`）或 `auto-reconnect`（`DroppedAfterConnected`）消息并发出 `ssh-tunnels-updated`；终态类别或被禁用的 `auto_reconnect` 会清除 desired 标记并以 `error` 停止；在继续重试时，`DroppedAfterConnected` 会把尝试计数归零，使下一次延迟重新从初始值开始。
- `next_retry_delay(attempt, jitter_fraction)` 计算 `base = min(RECONNECT_INITIAL_BACKOFF * 2^attempt, SUPERVISOR_RETRY_MAX_BASE_DELAY)`，其中 `RECONNECT_INITIAL_BACKOFF` 为 2 秒、`SUPERVISOR_RETRY_MAX_BASE_DELAY` 为 48 秒，并返回 `base * (1 + jitter_fraction)`；生产代码传入 `rand::random::<f64>() * SUPERVISOR_RETRY_JITTER_RATIO`，比例固定为 0.25，因此每次重试从 2 秒起步、带 0–25% 抖动、绝不超过 60 秒。监管器在每次尝试的退避睡眠之前把状态置为 `reconnecting`。
- `session_probe.rs` 实现由 `PROBE_TIMEOUT`（5 秒）限界的双步往返探测：第一步打开并关闭一个 SSH 会话通道且必须收到服务端应答；第二步只在第一步成功后运行，因此其失败被归类为 `Target`。`apply_probe_outcome` 在连续两次传输失败后让运行时退出，把目标失败只记为运行时 `last_error` 且不触碰计数，并在下一次成功时清除已记录错误。local 与 dynamic 运行时每 `PROBE_INTERVAL`（10 秒）经池化会话探测——local 对转发目标打开 `direct-tcpip` 通道，dynamic 在配置了探测目标时执行一次临时 SOCKS 往返——而 remote 运行时从自己的 accept tick 探测转发会话，并按 `io::ErrorKind`（`TimedOut` 与 `WouldBlock` 即周期 tick）而不是匹配 "timed out" 消息文本归类 accept 错误。
- 进程内存的 desired-running 集合（`DESIRED_TUNNEL_IDS` 加 mark、clear 与快照 helper）记录用户希望运行的内容：每个连接路径都会把隧道标记为 desired，每个停止路径——手动、分组与全量断开、删除与 upsert——都会清除它。按 id 的启动认领先持运行时管理器锁、再持认领锁地检查并插入；进行中的实例在首个启动结果被等待之前就插入管理器，因此以 `connecting` 可见；`start_tunnel_watchdog` 每 `TUNNEL_WATCHDOG_INTERVAL`（30 秒）扫描一次，只重启没有实例也没有认领的 desired id，并在真正 spawn 前于锁内再次检查两者；其 busy 集合（`watchdog_busy_ids`）把每个已持有的认领与每个运行时线程仍存活的实例计为忙，因此线程已结束的运行时绝不再阻塞重启。看门狗的 `if_missing` 启动调用 `begin_tunnel_start(_, false)`，要求隧道仍为 desired，否则拒绝且绝不标记；`reconcile_started_tunnel_with_desired` 在管理器插入之后、启动认领仍持有时运行，因此启动期间落地的断开获胜：刚启动的实例被停止并移除，连接返回断开视图。每个不继续重试的退出都会清除 desired 标记，因此看门狗与 poke 绝不会复活已被停止或终态的隧道。
- 重试 poke 纪元由 sleep-gap 心跳、macOS 唤醒观察者以及 `show_main_window` 调用的新廉价 `ssh_tunnels_poke` 窗口显示 poke 递增。监管退避睡眠（`sleep_respecting_stop_and_poke`）在收到 poke 时提前返回，每个探测循环——local 与 dynamic 的池化会话循环以及远端 accept 路径——都以 `probe_tick_due` 对照记录的 `retry_poke_epoch` 判定 tick，因此唤醒或窗口显示 poke 会在所有模式下立即触发探测，而不是让等待自己耗尽。`mark_system_resume` 仍不变地为网关恢复宽限供给信号；`ssh-tunnel-window-reconnect-start` / `ssh-tunnel-window-reconnect-done` 事件、同步的窗口显示重连及其常量已删除，既有前端监听器保持惰性。
- 线程已结束但状态为 `Error` 的实例由 `ssh_tunnels_refresh_status` 保留（其余任何已结束状态的实例都会被回收），因此终态 `error` 与其 `last_error` 保留到用户操作。`connect_blocking_running_ids` 只把 `Connecting`、`Connected` 与 `Reconnecting` 实例视为阻塞，供分组与全量显式连接路径判定，因此已结束的 `Error` 实例可被这些路径重连，而进行中的启动、连接或重试仍然阻塞。

## Alternatives considered

- 按错误消息字符串匹配归类探测失败：未采纳，因为消息文本随服务商与语言环境而变；现在归类使用双步 helper 返回的类型化 `FailureKind` 与远端 accept 路径上的 `io::ErrorKind`。
- 保留 `reconnect.rs` 中的睡眠/唤醒全量对账：未采纳，因为唤醒时对每条隧道做对账与监管器自身的重试循环重复，并可能竞争一次正在进行的重启，而 poke 纪元只打断已经在等待的等待。
- 在每条客户端连接上探测：未采纳，因为它会给每条已桥接连接增加一次往返，并会把客户端自身的目标失败错误归因到隧道；探测保持固定的 10 秒间隔。
- 持久化 desired-running 集合：未采纳，因为它会为 `state.enc.json` 增加持久化字段、迁移与生命周期规则；该集合只在进程内存，重启后仅由 `auto_connect` 决定启动什么。

## Consequences

- 恢复是持续的：首次尝试的可重试失败会进入同一条受监管的重试循环，重试以有界抖动退避继续直到某次成功；只有用户停止、终态归类（鉴权、主机密钥、配置）或被禁用的 `auto_reconnect` 才会以 `error` 结束监管并只留一条去重消息。成功连接会重置尝试计数，因此下一次断开从 2 秒重新重试。
- 半开链路由第二次连续传输探测失败检出并重连；不可达的转发目标只让仍处于 connected 的隧道记录 `last_error`，目标恢复应答后错误被清除，两个方向都不会重连。远端模式不再依赖匹配错误消息。
- 监管保持唯一：进行中的连接在启动结果到达前就以 `connecting` 可见，同一 id 的看门狗重启与手动连接只产生恰好一个实例，被手动断开（desired 已清除）的隧道绝不会被看门狗、唤醒 poke 或窗口 poke 重启。
- 新增内容全部只在进程内存：desired 集合、启动认领、重试次数与 poke 纪元绝不持久化，因此应用重启后只有 `auto_connect` 决定启动什么；`state.enc.json`、`SshTunnelStatus` 取值、命令名与参数形状以及 `ssh-tunnels-updated` 事件均不变，回退会恢复此前行为且不会使已保存隧道失效。
- poke 感知的探测门已接入所有模式：local 与 dynamic 的池化会话循环以及远端 accept 路径都以 `probe_tick_due` 对照记录的 `retry_poke_epoch` 判定，因此 sleep-gap 心跳、macOS 唤醒观察者与窗口显示 poke 会在 local、dynamic 与 remote 模式下同样立即触发探测。
- 行为测试锁定修复后的行为：启动呈现 `Connecting` → `Reconnecting` 序列、`watchdog_busy_ids` 对已结束运行时的重启豁免、`if_missing` 启动要求 desired、启动中途 desired 被清除时的对账停止、`ssh_tunnels_refresh_status` 保留终态 `Error`，以及排除 `Error` 的 `connect_blocking_running_ids`。
- 取代评估：不取代 [Gateway Auto-Disable Recovery Uses a Cooldown Half-Open Probe, a Post-Resume Transport Grace and a Transition Broadcast](../architecture/2026-09-23-gateway-auto-disable-recovery.md)，其恢复信号事实继续成立，因为 `mark_system_resume` 仍在 sleep-gap 心跳与 macOS 唤醒观察点运行并继续不变地为网关宽限供给信号；不取代 [Tray menu ownership and contract](../architecture/2026-09-22-tray-menu-ownership-and-contract.md)，其 `ssh_tunnels_connect_all` / `ssh_tunnels_disconnect_all` 批量行为、批次标识与结果契约均不变。`MEMORY.md` 与 `.ai-workflow/index/navigation.json` 中的 `ssh-tunnels-backend` 条目在同一变更中承载监管标准，`navigation.md` 已按权威 JSON 重新生成。
