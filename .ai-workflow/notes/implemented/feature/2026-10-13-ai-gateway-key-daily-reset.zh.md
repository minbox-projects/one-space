# Agent Note: Gateway Daily-Reset Keys Recover at the Next UTC+8 Day Boundary and the Editor Replaces Reordering with a Compact Daily-Reset Control

Status: implemented

[English](2026-10-13-ai-gateway-key-daily-reset.md) | 中文

## Problem

额度耗尽的上海上游服务商 key 此前只经既有的基于时间的路径恢复：额度标记的标记龄至少达到 `KEY_QUOTA_MARK_TTL_SECS = 1800` 秒后才会重新可被选中，而全部标记时的半开探测会在 `KEY_PROBE_COOLDOWN_SECS = 60` 后重试最旧的标记 key。这两者都不匹配按自然日边界重置额度的服务商：14:00 被标记的 key 可能在午夜前就被再次尝试（并重新标记、白费一次请求），而 23:59 被标记的 key 会一直保留到 TTL 过期，而不是在次日被释放。操作者也没有任何按 key 表达「该凭据每天恢复」的方式，而密钥编辑器的上移/下移控件占用了该行，其重排动作会静默改变持久化的优先级顺序。

## Decision

`UpstreamKey` 新增 `#[serde(default)] daily_reset: bool`，始终序列化且 `Default` 为 false；缺少该字段的存储记录读取为 false，配置 schema 保持版本 3、无新迁移。该设置是用户意图，独立于运行时标记，且绝不改变 key 的 `enabled` 标志。

`selection::daily_quota_reset_due(key, now)` 是共享的到期判定。仅当 key 用户启用、`daily_reset` 开启、运行时标记为 `failure_kind == Quota` 且带有已知 `marked_at`，并且 `now` 落在严格晚于标记日的固定 UTC+8 自然日时才为真。`utc8_natural_day` 经商/余数分解折叠 `+08:00` 偏移，因此没有任何时间戳（含 `u64::MAX`）会溢出，也绝不查询设备时区。同一天的标记无论多旧都保持排除，未来日期的标记绝不提前到期。`selection::quota_mark_expired` 对 `daily_reset` key 返回该判定，对 legacy（`daily_reset = false`）key 保持含边界的 1800 秒 TTL；因此 `select_usable_key` 经正常列表顺序选择接纳已到期的每日 key，而 `find_key_probe_candidate` 排除每个 `daily_reset` key，所以同一天的每日标记绝不被探测，已到期者经选择与扫描器而非探测恢复。

`daily_reset::scan_due_daily_reset_marks<R: Runtime>(app: &AppHandle<R>, now: u64) -> Result<usize, String>` 是生产恢复过程。它在最新序列化配置上运行一次 `storage::modify_config` 闭包，只经 `clear_key_runtime_state` 清除每个到期 key 的运行时标记字段（`auto_marked`、`failure_kind`、`marked_at`、`reason`），保留用户标志与无关配置；没有到期项的遍历报告无变化且绝不重写文件。成功持久化变更后，它经所传入的 `AppHandle`（而非监听器捕获的句柄）发出既有 `AI_GATEWAY_CONFIG_UPDATED_EVENT` 及其 `()` 载荷，并在既有的测试可观测配置事件接缝上记录同一通知。写入失败向上传播错误，保留此前完整的字节并绝不发事件。

`daily_reset::start_daily_reset_scanner(app: AppHandle) -> bool`（在 `ai_gateway` 根以 `pub(crate)` re-export）恰好安装一次进程级扫描器，并在生成工作线程之前于调用者线程上同步执行即时启动扫描；第二次调用为空操作。工作线程每一趟都重新读取新的真实时钟，并在应用运行期间每 `DAILY_RESET_SCAN_PERIOD = 60` 秒扫描一次，独立于 WebView、网关监听器与模板刷新偏好。`daily_reset::stop_daily_reset_scanner()` 丢弃工作线程的停止发送端并 join 其所属线程，幂等且在没有运行时安全。应用 setup 在既有模板调度器旁启动它（`run_app.rs`），`shutdown_runtime_services` 停止它（`shortcuts_tray.rs`），因此退出后不残留重叠的写入者。

前端 `GatewayProviderKey.daily_reset?: boolean` 镜像该设置且可选，以便功能前的 fixture 与记录仍可赋值；编辑器保存 `daily_reset: key.daily_reset === true`，新行从 false 开始，并且其仅运行时合并只复制运行时字段，因此未保存的日重置编辑能经受传入的运行时刷新。密钥行此前的上移/下移控件、`moveKey` 及其双语 `MoveUpAria`/`MoveDownAria` 文案已移除（`ChevronUp`/`ChevronDown` 导入仍用于映射展开），腾出的区域承载一个紧凑的带标签开关：`aiGatewayProviderKeyDailyReset`（`Daily reset` / `日重置`）、按行的 `aiGatewayProviderKeyDailyResetAria` 名称与说明固定 UTC+8 含义的原生 title `aiGatewayProviderKeyDailyResetHint`。存储的 key 顺序与末尾追加行为不变，且没有前端定时器。

## Alternatives considered

- 用滚动 24 小时标记龄而非 UTC+8 自然日重置每日 key：未采纳，因为操作者的意图是「次日再次可用」，而规定的边界是 UTC+8 自然日而非时长；滚动窗口会在日中恢复并随每次重启漂移。
- 用设备本地时区作为日边界：未采纳，因为恢复必须在各主机上确定一致，并与网关已用于用量与计价的 UTC+8 约定匹配；设备时区会让同一份配置在不同机器上行为不同。
- 像额度 TTL key 一样让全部标记的 60 秒探测恢复每日 key：未采纳，因为同一天的每日标记按设计绝不能被尝试，而已到期者经选择与扫描确定性地恢复；探测它会花掉该功能本就是为了省下的请求。
- 只在后台扫描恢复、不加入选择判定：未采纳，因为午夜后到达的请求必须能立即使用该 key；只有扫描器会让第一个合格请求最多等待 60 秒。
- 增加 OS 唤醒钩子或守护进程以跨午夜存续睡眠或退出：未采纳，因为启动与 60 秒进程扫描已经完成追赶，而睡眠/恢复集成会把变更扩展到网关进程之外。
- 保留上移/下移重排控件并把每日控件放到别处：未采纳，因为编辑器绝不能静默改变持久化优先级，而腾出的按钮区域是在不挤占名称/值字段前提下承载新控件的自然紧凑位置。

## Consequences

- 在某个 UTC+8 日被标记为额度耗尽的 `daily_reset` key 在该日余下时间同时被排除出正常选择与探测，并在下一个 UTC+8 零点精确变为可用，即使 1800 秒 TTL 尚未到期；午夜前一秒的标记在下一秒即合格，同一天的标记无论多旧都保持排除，未来日期的标记绝不提前恢复。
- legacy（`daily_reset = false`）key 保持含边界的 1800 秒 TTL、唯一一次 60 秒半开探测与手动重新启用路径不变；auth 标记、没有已知标记时间的 key 与用户禁用的 key 绝不被每日恢复触及，且没有 key 到期时扫描器绝不重写任何内容。
- 恢复只经既有串行化写入者写入合格的运行时字段，保留用户标志与无关配置，并在成功持久化变更后发布既有 `ai-gateway-config-update` 事件；写入失败保留此前文件、不发事件并保持到期，供后续扫描处理。
- 扫描器在启动时及每 60 秒运行，因此跨日边界的退出或睡眠会在下次启动时追赶，且第一个合格请求可立即使用该 key；它独立于网关监听器与模板刷新，且退出时 join 工作线程，不残留重叠的写入者。
- 密钥编辑器不再重排 key：它保留存储顺序、末尾追加新 key，并对每个 key 暴露独立可操作的日重置开关，带有简洁的双语标签、按行的无障碍名称与原生 tooltip 的 UTC+8 提示；既有的密钥编辑、启用/禁用、删除、可见性与手动重新启用操作保持可用。
- 恢复后的每日 key 若在上游仍然耗尽，会按实际失败时间重新标记，网关轮换到下一个可用 key，且该每日 key 再次被排除至其下一个日边界；key 域额度失败的轮换不为映射失败计数器增加任何计数。
- 兼容与回滚：唯一的持久化扩展是 schema 3 下 serde 默认、始终序列化的布尔值，因此旧记录读取为 false，较早的构建仍能读取其余 key 字段（其后续保存可能丢弃新字段）。不新增迁移、依赖、上游额度查询或 WebView 轮询，回退代码即恢复 legacy TTL/探测行为与此前的编辑器控件。
- 验证：`src-tauri/src/ai_gateway/tests/daily_reset.rs` 中的六个新后端行为测试覆盖独立设置的往返、UTC+8 日边界（超过 1800 秒的同日排除、一秒午夜边界、未来日期排除、缺失时间排除）、生产扫描的持久化清除与发出事件、扫描器的启动/周期追赶与幂等停止、一次真实持久化失败后稍后恢复，以及恢复后仍耗尽的 key 轮换且无映射失败。`cargo test --manifest-path src-tauri/Cargo.toml --lib ai_gateway` 以 0 退出、765 通过、2 个既有 ignored、用时 93.26 秒（含 15 个 `attempt_policy` 测试）。`cargo fmt --manifest-path src-tauri/Cargo.toml --check` 以 1 退出，源于本变更不负责的广泛既有基线漂移，而新模块、新测试文件与新增行本身 fmt 干净。前端五个声明的 AI 网关测试文件以 410 个测试通过、退出 0，限定范围的 `eslint` 退出 0（0 错误、2 个既有 `any` 警告），生产 `tsc` 加 Vite 构建退出 0。
- 验证缺口：未进行交互式可视化检查。没有可用的 UI 检查工具或有实施的测试人员，因此英文/中文窄/宽编辑器开关/保存/重开与关闭页面后再打开的状态刷新都未经视觉确认，也不存在截图；紧凑编辑器替换、后台与追赶恢复、保留的用户意图以及持久变更与 UI 一致性的 UI 部分仅由组件测试与生产构建佐证，绝不声称完成了完整的交互式检查。一次原生 `npm run tauri dev` 运行在 291 毫秒内到达 Vite ready、`cargo` dev 编译 45.75 秒并运行起 `target/debug/OneSpace`，随后所启动的进程组被停止且端口被释放；无关的 release 进程（用户 PID 49737）未被触碰。
- Supersession（取代评估）：部分取代。[Gateway Key Pool Rotation Is Ordered, Probe-Limited and Manual-Only for Auth](../architecture/2026-09-24-gateway-key-pool-rotation.md) 予以保留并交叉链接；本记录只取代其密钥编辑器重排事实（上移/下移控件与 `moveKey` 已移除）并新增按 key 的日重置设置，而有序选择、持久 auth 与 quota 标记、空池跳过服务商、鉴权手动恢复与 upsert 保留继续有效。[Gateway Key Health Rotates Bare 429s, Recovers Quota Marks after a TTL, Retries a Single Candidate and Pins the Query Source Key](../architecture/2026-09-25-gateway-key-health-lifecycle.md) 与 [Gateway Credit-Exhaustion Responses Mark the Attempted Key as Quota](../bug-fix/2026-10-06-gateway-credit-exhaustion-key-mark.md) 予以保留并交叉链接；本记录只取代其对 daily-enabled key 的恢复陈述（`daily_reset` 额度标记在下一个 UTC+8 日边界恢复，而非 1800 秒 TTL，且被排除出探测），而裸 429 轮换、legacy TTL 与探测、单候选重试、源 key 梯子、状态加额度消息分类与手动路径全部继续有效。
- 同一变更中更新了 `MEMORY.md` 与 `ai-gateway`、`ai-gateway-backend`、`app-runtime-backend` 导航条目，并按权威 JSON 重新生成了 `navigation.md`。
