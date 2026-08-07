# STD-003 账号池真实渲染验收证据

- 执行日期：2026-08-08
- 基线：`cc9e03d29da928d4a16f17ca5c82438992af8639`
- 浏览器：Playwright Chromium，实际启动本机 Google Chrome，headless 渲染
- 页面：`http://127.0.0.1:4174`，通过页面级 Tauri IPC fixture 返回账号池数据；验收脚本未向产品代码增加专用逻辑分支
- 实际开发服务器命令：`npm run dev -- --host 127.0.0.1 --port 4174`（4173 已由其他 worktree 占用）
- 实际验收命令：`node .ai-work-flow/plans/account-pool-refactor/std-003-playwright-evidence.mjs http://127.0.0.1:4174`

完整原始报告见 [`std-003-playwright-report.json`](./std-003-playwright-report.json)，可复核脚本见 [`std-003-playwright-evidence.mjs`](./std-003-playwright-evidence.mjs)。

## 尺寸与溢出

脚本在首页、账号列表、创建表单和创建成功返回列表状态读取 `document.documentElement.scrollWidth/clientWidth`，并记录账号池根容器的 `taskWidth/taskClientWidth/taskHorizontalOverflow`。分组 tabs 使用真实 `role="tablist"` locator 设置滚动位置并逐个滚入、点击；最终数值和断言只以 JSON 为准：视口见 `results[0].viewport`、`results[1].viewport`，尺寸见 `results[*].measurements[*].viewport`、`documentWidth`、`clientWidth`、`horizontalOverflow`、`taskWidth`、`taskClientWidth`、`taskHorizontalOverflow`，tabs 滚动见 `results[*].assertions[0].tablist` 与 `results[*].assertions[0].reachableTabs`。

两个结果的 `browserErrors` 均以 JSON `results[*].browserErrors` 为准；报告中的 `horizontalOverflow` 和断言通过条件由脚本末尾的 `conclusion` 计算。

## 结构化断言

以下内容只引用 [`std-003-playwright-report.json`](./std-003-playwright-report.json) 中每个视口的 `assertions` 字段；断言 ID、字段路径和值均以该 JSON 为唯一来源。

| 断言 ID | JSON 字段路径 | 结论 |
| --- | --- | --- |
| `group-tabs-horizontal-reachability` | `results[*].assertions[0].passed`、`tabCount`、`tablist`、`lastTab`、`reachableTabs`；每个 tab 的字段为 `results[*].assertions[0].reachableTabs[*].visible`、`enabled`、`tablistContainment`、`viewportContainment`、`selectedAfterClick` | 两个结果中 `passed` 为 `true`，8 个 tabs 均完成滚入和点击。 |
| `group-dialog-viewport-controls` | `results[*].assertions[1].passed`、`boundingBox`、`viewport`、`viewportContainment`、`createCall`、`deleteConfirmation`；控件字段为 `results[*].assertions[1].controls.close.visible`、`controls.close.enabled`、`controls.close.viewportContainment`、`controls.close.elementFromPoint.unobscured`、`controls.createInput.visible`、`controls.createInput.enabled`、`controls.createInput.viewportContainment`、`controls.createInput.elementFromPoint.unobscured`、`controls.createButton.visible`、`controls.createButton.enabled`、`controls.createButton.viewportContainment`、`controls.createButton.elementFromPoint.unobscured`。 | 两个结果中 `passed` 为 `true`，弹层和关键控件均通过视口及未遮挡断言。 |
| `default-group-protection` | `results[*].assertions[2].passed`、`groupId`、`name`、`renameEntryCount`、`deleteEntryCount` | 两个结果中 `passed` 为 `true`，默认组重命名和删除入口计数均为零。 |
| `custom-group-actions` | `results[*].assertions[3].passed`、`groups[*].groupId`、`groups[*].name`；入口字段为 `results[*].assertions[3].groups[*].rename.visible`、`groups[*].rename.enabled`、`groups[*].rename.viewportContainment`、`groups[*].rename.elementFromPoint.unobscured`、`groups[*].delete.visible`、`groups[*].delete.enabled`、`groups[*].delete.viewportContainment`、`groups[*].delete.elementFromPoint.unobscured`。 | 两个结果中 `passed` 为 `true`，所有自定义组入口均通过。 |

弹层控件通过真实填写、点击新建、进入重命名后取消，以及点击删除后取消原生确认的重放；调用和确认文案分别见 `results[*].assertions[1].createCall`、`results[*].assertions[1].deleteConfirmation` 及 `results[*].interactions`。

## 交互结果

- 五个网关页签、长内容、批量工具栏、确认弹窗和完整创建表单的交互原文见 JSON `results[*].interactions`。
- 批量删除确认文案和取消后的选择状态见 `results[*].confirmationMessage` 及对应 `interactions` 项。
- 创建与刷新调用的最终命令计数见 `results[*].commandCounts.ai_routing_gateway_account_create_api_key_with_configuration` 和 `results[*].commandCounts.ai_routing_gateway_bootstrap`；分组新建调用参数见 `results[*].assertions[1].createCall.args.input`。
- 创建后的账号显示结论见 `results[*].interactions`，脚本使用 `results[*].commandCounts` 和 DOM 等待共同验收。

## 持久证据

本轮不声称仓库内保留截图文件。`std-003-playwright-report.json` 持久记录最终执行的命令、desktop/mobile 视口、尺寸、溢出、browserErrors、四项结构化断言和交互结果；`std-003-playwright-evidence.mjs` 是可重放脚本，执行后会重新生成页面截图。原生 `window.confirm` 不属于 DOM，确认文案和取消结果由 JSON `results[*].confirmationMessage` 及 `results[*].assertions[1].deleteConfirmation` 持久记录。

## 检查结论

- 账号池定向 Vitest：`npm test -- --run src/lib/aiRoutingGateway.test.ts src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，2 个测试文件、38 项通过。
- 前端全量 Vitest：`npm test -- --run`，33 个测试文件、313 项通过。
- TypeScript/Vite 构建：`npm run build`，通过；保留 Browserslist 过期和大 chunk warning。
- ESLint：`npm run lint`，通过，0 errors、386 条 warning。
- diff 检查：`git diff --check`，通过。
- React 创建成功测试以状态化 bootstrap fixture 返回新账号，断言后续加载并在团队分组列表显示，同时保留单次原子创建请求的完整 payload 断言。
- Rust 本轮未修改 Rust 文件；因此未重复全量 Cargo 测试。任务 04 已有基线记录的 Rust 定向、共享 SQLite 和全量测试结果保持不变。
- 本报告最终结论只引用 JSON 根字段 `conclusion`；其结论必须与脚本末尾基于 `results[*].browserErrors`、`results[*].measurements[*].horizontalOverflow` 和 `results[*].assertions[*].passed` 的计算保持一致。
