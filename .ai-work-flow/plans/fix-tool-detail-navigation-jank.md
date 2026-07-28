# 修复工具详情导航卡顿实施计划

## 目标

- 缩短从 Launcher 或“更多工具”进入 Bookmarks、SSH Servers、SSH Tunnels、Protocol Router、Cloud Drive 的阻塞时间，并消除返回列表后再次进入造成的重复挂载和重复加载。
- 将 Protocol Router 的全量调用记录聚合移到 Rust 端，限制 IPC 响应和前端计算规模。
- 限制大列表首批 DOM 数量，消除 SSH 排序中的重复线性查找和多项 secret 顺序读取。
- 建立可复验的冷进入、热进入、数据加载、计算、渲染和长任务指标，并在真实 Tauri 环境完成验收。

## 非目标

- 不重做“更多工具”视觉设计、导航信息架构或工具业务功能。
- 不接入真实阿里云盘 API；仅移除 mock 路径中的人为等待。
- 不改变书签、SSH 主机、SSH 隧道和协议调用记录的持久化格式、保留周期或用户数据。
- 不以一次性关闭 React StrictMode 作为性能修复；StrictMode 仅作为开发测量变量单独标注。
- 不在本计划内处理协议调用日志文件本身的增量存储或数据库迁移；若 Rust 端解析 12,000 条记录仍不达标，再另立计划。

## 已知基线与成功标准

- 当前链路：`Launcher/MoreToolsHub -> App.navigateToTab -> MoreToolsHub` 条件挂载详情；返回列表会卸载详情。
- 当前合成数据长任务：Protocol Router 12,000 calls 为 104ms/75ms；Bookmarks 1,500 条为 473ms/75ms；SSH 1,000 hosts 为 407ms；Tunnels 300 条为 251ms。保留原始脚本、数据生成方式、机器和构建模式，实施前先补记两个数值各自含义。
- 同机同数据每项运行 3 次，以中位数验收：
  - 冷进入“点击到详情壳首帧”不超过 100ms，热进入不超过 50ms。
  - 上述合成场景不出现 `>= 50ms` 的前端长任务；若 WebView 不支持 `longtask` entry，则以事件循环延迟探针和 React 提交时长交叉验证。
  - Bookmarks/SSH 首批最多 60 项，Tunnels 首批最多 30 项；筛选仍覆盖全量内存数据，用户可继续加载其余结果。
  - Protocol Router 12,000 calls 场景的紧凑响应只含当前页最多 20 条请求，不返回全量 `calls`；前端不再按路由反复 `filter/sort/reduce`。
  - SSH 冷加载从 4 次顺序 invoke 改为并行的“主机列表 + 一次 SSH 元数据读取”，secret 文件只解密一次。
  - 返回工具列表再进入同一详情时，组件实例、筛选/分页/编辑状态和已加载数据保持不变，不重复发起初始化请求；主动刷新和数据变更事件除外。
  - CloudDrive mock 连接和目录读取路径不存在固定 800ms/1000ms sleep。
- 功能回归标准：直接入口、Launcher 入口、更多工具入口、返回目的地、搜索/筛选、分页/继续加载、编辑与连接操作均保持现有语义。

## 分阶段实施

### 阶段 0：固定基线与性能埋点

涉及文件：

- 新增 `src/lib/toolPerformance.ts` 及 `src/lib/toolPerformance.test.ts`。
- 修改 `src/App.tsx`、`src/components/MoreToolsHub.tsx`，并在五个目标详情组件接入阶段标记。

具体改动：

1. 提供开发态、可显式开启的统一埋点：`navigation-start`、`shell-painted`、`data-ready`、`compute-done`、`list-committed`，键包含 tool id、cold/warm 和一次导航 transaction id。
2. 使用 `performance.mark/measure`；支持时注册 `PerformanceObserver("longtask")`，不支持时记录事件循环延迟。默认不上传、不写用户数据，生产构建无日志噪声。
3. 在 Launcher 卡片、更多工具卡片和 `navigateToTab` 直接入口统一开始计时；详情在壳首帧和首批数据提交后结束计时。Tauri invoke 单独记录协议统计和 SSH 元数据耗时。
4. 先按当前实现复跑并保存三次原始结果，再进行优化；jsdom 测试只验证 mark 顺序和次数，不把墙钟时间作为 CI 门禁。

阶段门槛：埋点能区分数据等待与主线程阻塞，重复导航不会串用 transaction；基线包含开发 StrictMode、生产前端构建和真实 Tauri 三种模式中的适用项。

### 阶段 1：保活已访问的工具详情

涉及文件：

- `src/components/MoreToolsHub.tsx`
- `src/App.tsx`
- `src/components/MoreToolsHub.test.tsx`
- `src/App.moreToolsNavigation.test.tsx`

具体改动：

1. `MoreToolsHub` 维护仅在本次应用会话内有效的 `visitedTools`；目录与已访问详情保持挂载，通过 `hidden`/可见包装层切换，首次访问前不预挂载低频工具。
2. 初始 `activeTool` 非空时同步加入首批 visited 集合，避免 Launcher 直达出现空帧。
3. 给有轮询或刷新监听的详情传递真实 `isVisible`：Protocol Router、SSH Tunnels 仅在当前可见时刷新/轮询；隐藏详情保留本地状态但不持续做昂贵工作。Bookmarks、SSH Servers、CloudDrive 保持实例，不因返回列表重新初始化。
4. 保留 `moreToolsReturnTab` 语义：Launcher 直达返回 Launcher，目录进入返回工具列表；再次直达同一工具复用已挂载实例。

阶段门槛：测试以组件 mount 计数、初始化 invoke 计数和内部状态探针证明“首次懒挂载、返回不卸载、热进入不重载”；切换到其他主 tab 后隐藏工具无轮询。

### 阶段 2：Protocol Router 后端聚合与分页

涉及文件：

- `src-tauri/src/protocol_router/types_config.rs`
- `src-tauri/src/protocol_router/stats_public.rs`
- `src-tauri/src/protocol_router/commands.rs`
- `src-tauri/src/protocol_router/tests.rs`
- `src-tauri/src/app_runtime/run_app.rs`
- `src/lib/protocolRouter.ts`
- `src/components/ProtocolRouterTool.tsx`
- 新增 `src/components/ProtocolRouterTool.test.tsx`

具体改动：

1. 新增 `protocol_router_dashboard` 命令；Rust 端读取并按 retention/days 裁剪后只排序一次、单次遍历生成总计、错误数、每路由摘要、趋势桶和当前筛选的请求分页。
2. 查询参数：`days` 限制 1..365，`route_id` 可空，`page >= 1`，`page_size` 限制 1..100，前端固定 20；携带本地 UTC offset，使 1 天的连续 3 小时桶和 7/30 天的本地日桶边界可复验。
3. 路由摘要提供 `call_count`、token 总数、最后调用/延迟/错误，以及最近 10 次的失败数和样本数，前端据此保持 connected/flaky/failed/inactive 规则；route enabled 仍由 config 决定。
4. 前端按 `statsDays + selectedRouteId + requestsPage` 请求紧凑数据；删除全量 calls 上的多轮 `filter/sort/reduce` 和趋势构建。配置、状态、dashboard 可并行加载，快速切换筛选时用 request id/Abort 等价机制丢弃过期响应。

阶段门槛：Rust 单元测试覆盖聚合正确性、稳定倒序分页、空数据、未知 route、边界 clamp、时区桶和 12,000 条响应上限；组件测试覆盖筛选/翻页只渲染返回页且过期响应不覆盖新状态。

### 阶段 3：大列表首批限量与 SSH 热点

涉及文件：

- `src/components/Bookmarks.tsx`
- `src/components/SshServers.tsx`
- `src/components/SshTunnels.tsx`
- `src-tauri/src/secrets.rs`
- `src-tauri/src/ssh_oauth.rs`
- `src-tauri/src/app_runtime/run_app.rs`
- 新增对应组件测试；补充 Rust secret/SSH 元数据单元测试。

具体改动：

1. 三个列表分别维护可见数量：Bookmarks/SSH 每批 60，Tunnels 每批 30；搜索、标签、视图或分组变化时重置首批，显示“已显示/总数”和明确的继续加载操作。数据层仍保留全量，编辑、连接、状态事件和排序作用于全量数据。
2. Bookmarks 将标签集合、规范化搜索词和筛选结果放入 `useMemo`，保存时复制后排序，避免原地修改 state 数组。
3. SSH 将 favorites/ignored 转为 `Set`，将 config history 转为 `Map<name, connect_count>`；筛选、排序和卡片 frequent 判断复用索引，移除比较器及 map 回调内的 `history.find`。
4. 在 `secrets.rs` 增加一次加载后读取指定键的 crate 内 helper；在 `ssh_oauth.rs` 新增范围受限的 `get_ssh_server_metadata`，仅返回 history/favorites/ignored。前端并行请求 hosts 与 metadata，解析失败时对单项使用空数组并报告可定位错误。
5. Tunnels 对当前分组结果先 memo，再切片渲染；批量连接/断开判断复用 memo 结果，实时状态更新不重置用户已展开数量。

阶段门槛：合成列表测试断言初始 DOM 上限、继续加载、筛选全量命中、状态更新和操作目标正确；SSH 排序纯函数/组件测试覆盖 favorites、frequent、字母序优先级，Rust 测试证明批量读取只加载一次且不返回其他 secret。

### 阶段 4：移除 CloudDrive mock 人为延迟

涉及文件：

- `src/components/CloudDrive.tsx`
- 新增 `src/components/CloudDrive.test.tsx`

具体改动：

1. 移除连接和目录读取中的固定 1000ms/800ms sleep；保留真实 secret invoke 对应的 loading/error 状态。
2. 将 mock 文件解析保持为同步、确定性结果；用 request id 防止快速切目录时旧结果覆盖新目录。

阶段门槛：假计时器测试证明无固定等待，已有 token 时首批文件在 secret 返回后的下一次提交可见，快速目录切换以最后一次请求为准。

### 阶段 5：集成验收

1. 运行受影响 Vitest、完整 `npm test`、`npm run lint`、`npm run build`，以及 `cargo test protocol_router` 和新增 Rust 测试。
2. 使用与基线相同的合成数据复跑；报告三次原始值、中位数、最长任务、DOM 数和 IPC 响应条数/字节数，不只报告百分比。
3. 在真实 Tauri 开发构建与生产构建各验证一次 Launcher 直达、目录进入、返回、热进入和工具切换；开发结果注明 StrictMode 双 effect，最终门槛以生产构建为准，同时确认 StrictMode 下无重复副作用。
4. 检查隐藏详情无后台轮询、无重复 listener、无控制台异常，用户数据写入格式未变化。

## 数据契约与兼容策略

- `protocol_router_dashboard` 是新增命令；保留现有 `protocol_router_stats`、`ProtocolRouterStatsSummary.calls` 和注册项，直到新前端稳定后另行清理。新响应不复用旧类型，避免“同名字段不同语义”。
- dashboard 分页按 `ts desc`，相同时间戳追加稳定 tie-break；`recent.total` 表示 route filter 后总数，聚合总计和 route summaries 表示 days 窗口内全量。
- 趋势桶返回 `bucket_start`、`bucket_seconds`、`calls`、`total_tokens`，展示标签由前端本地化；不传调用明细用于重建聚合。
- `get_ssh_server_metadata` 为新增、范围受限命令；旧 `get_secret` 继续可用。history/favorites/ignored 的 JSON 字符串和值结构保持原样，缺键等价空数组。
- 工具保活只影响 React 生命周期，不改变 URL、tab id、Launcher target、Tauri 命令名或持久化 schema。
- 渐进展示只限制 DOM，不截断搜索数据、不改变排序结果、不修改保存内容。

## 测试策略

- 单元测试：Rust 聚合/桶/分页、SSH 批量 secret 投影；TypeScript 埋点事务、SSH 索引排序和可见数量重置。
- 组件测试：MoreToolsHub 生命周期、App 返回目的地、各列表 DOM 上限与继续加载、Protocol Router 请求竞态与分页、CloudDrive 无固定 timer。
- 集成检查：Tauri 命令注册、序列化字段和前端类型一致；协议旧命令仍可调用，新增命令响应不含全量 calls。
- 性能测试：固定数据、固定机器、固定构建模式，3 次中位数；CI 只门禁确定性规模和调用次数，墙钟阈值在真实 Tauri 验收，避免共享 runner 抖动导致误报。

## 风险与回滚

- 保活会增加常驻内存和 listener 数：只保活已访问工具，昂贵任务受 `isVisible` 门控；若内存不可接受，可独立回滚阶段 1，后端和列表优化仍有效。
- 后端桶边界和前端本地时间可能偏移：契约显式传 offset，并用跨午夜/DST 附近样例测试；异常时仅回退到旧 stats 调用，不改日志数据。
- 分页可能在新调用写入时漂移：稳定排序并在每次刷新/筛选变化时回到第 1 页；本计划不引入快照游标。
- 渐进展示可能隐藏用户目标：始终展示总数，搜索全量数据并重置首批，继续加载可达全部结果；可按单工具独立回滚。
- 批量 secret 命令扩大读取面：命令只暴露固定三键，不接受任意 key；旧单键接口不移除。
- 当前工作区已有未提交的 `src/App.tsx`、`src/components/MoreToolsHub.tsx` 及测试改动；实施必须基于当时 diff 增量合并，不得覆盖或重置现有变更。

## 建议任务/提交拆分

1. `perf: add tool navigation measurement baseline`：仅埋点与基线记录。
2. `fix: preserve visited more-tools detail instances`：详情保活、可见性门控及导航测试。
3. `perf: add compact protocol router dashboard query`：Rust 契约、命令、测试和注册。
4. `perf: consume compact protocol router dashboard`：前端类型、竞态控制和组件测试；可在此后独立切回旧命令。
5. `perf: bound bookmarks initial rendering`：Bookmarks 首批限制与测试。
6. `perf: index and batch ssh server bootstrap data`：SSH 索引、批量元数据命令与测试。
7. `perf: bound ssh tunnel initial rendering`：Tunnels 首批限制、状态回归测试。
8. `fix: remove cloud drive mock waits`：CloudDrive 延迟与竞态测试。
9. `test: verify tool navigation performance in tauri`：全量检查与真实 Tauri 验收记录，不混入行为改动。

## 假设与实施门禁

- 已给出的合成数据可复用；若原始测量脚本未入库，阶段 0 先将步骤固化为可重复的开发测试入口，但不把不稳定墙钟断言加入 CI。
- 真实 Tauri 环境可在实施验收时访问本地协议统计、SSH config 和加密 secrets；否则只能完成结构门槛，不能宣称性能验收完成。
- 本计划须经用户明确确认后才能实施；确认前不得修改业务源码、测试或配置。
