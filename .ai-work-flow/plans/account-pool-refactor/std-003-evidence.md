# STD-003 账号池真实渲染验收证据

- 执行日期：2026-08-07
- 基线：`b0368736f502248c99757391084bec4a0d912b97`
- 浏览器：Playwright Chromium，实际启动本机 Google Chrome，headless 渲染
- 页面：`http://127.0.0.1:4173`，通过页面级 Tauri IPC fixture 返回账号池数据；验收脚本未向产品代码增加专用逻辑分支
- 开发服务器命令：`npm run dev -- --host 127.0.0.1 --port 4173`
- 验收命令：`node .ai-work-flow/plans/account-pool-refactor/std-003-playwright-evidence.mjs http://127.0.0.1:4173`

完整原始报告见 [`std-003-playwright-report.json`](./std-003-playwright-report.json)，可复核脚本见 [`std-003-playwright-evidence.mjs`](./std-003-playwright-evidence.mjs)。

## 尺寸与溢出

每个视口均在首页、账号列表、创建表单和创建成功返回列表状态读取 `document.documentElement.scrollWidth/clientWidth`；`taskWidth/taskClientWidth` 是账号池根容器的同一指标。

| 实际视口 | 状态 | documentWidth | clientWidth | horizontalOverflow | taskWidth/clientWidth | taskHorizontalOverflow |
| --- | --- | ---: | ---: | --- | --- | --- |
| 1440x1000 | 首页、账号列表、创建表单、创建后列表 | 1440 | 1440 | `false` | 1136/1136 | `false` |
| 390x844 | 首页、账号列表、创建表单、创建后列表 | 390 | 390 | `false` | 342/342 | `false` |

两组视口的 `browserErrors` 均为 `[]`。

## 交互结果

- 五个网关页签：首页、账号池、网关密钥、请求日志、设置均真实点击并展示对应面板。
- 长内容：默认分组中的长账号名、长 API 地址、长标签、长模型映射均可读，未造成文档级水平溢出。
- 批量工具栏：全选当前默认分组的 2 个可见账号后，批量禁用和批量删除按钮均可见可操作；取消批量删除确认后选择仍为 2 个。
- 确认弹窗：真实触发原生确认文案 `永久删除选中的 2 个账号及其凭据、额度和映射？请求历史快照会保留。`，自动化选择取消；分组管理弹层也真实打开并检查关闭控件、默认组保护和团队组操作。
- 完整创建表单：填写连接、认证方式、上游协议、分组、标签、阈值、备注、两个模型映射和每个模型四类价格；保存只调用 1 次 `ai_routing_gateway_account_create_api_key_with_configuration`，后续 bootstrap 返回新账号，切换团队分组后列表显示 `Playwright 创建后的新账号`。

## 持久证据

本轮不声称仓库内保留截图文件。`std-003-playwright-report.json` 持久记录最终执行的命令、视口、尺寸、溢出、browserErrors 和交互结果；`std-003-playwright-evidence.mjs` 是可重放脚本，执行后会重新生成同一组页面截图。原生 `window.confirm` 不属于 DOM，确认文案和取消结果由 JSON 报告持久记录。

## 检查结论

- 账号池定向 Vitest：`npm test -- --run src/lib/aiRoutingGateway.test.ts src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`，38/38 通过。
- 前端全量 Vitest：`npm test -- --run`，33 个测试文件、313 项通过。
- TypeScript/Vite 构建：`npm run build`，通过；仅保留既有 Browserslist 和大 chunk warning。
- ESLint：`npm run lint`，通过，0 errors、386 条仓库既有 warning。
- diff 检查：`git diff --check`，通过。
- React 创建成功测试现在以状态化 bootstrap fixture 返回新账号，断言后续加载并在团队分组列表显示，同时保留单次原子创建请求的完整 payload 断言。
- Rust 本轮未修改 Rust 文件；因此未重复全量 Cargo 测试。任务 04 已有基线记录的 Rust 定向、共享 SQLite 和全量测试结果保持不变。
- 本报告结论：两组指定视口均无浏览器错误和文档级水平溢出，指定 tabs、长内容、批量工具栏、完整创建表单、确认弹层及创建后刷新路径全部通过。
