# 04 - 更新索引并完成整体验收

- task_id: `verify-and-index`
- order: `04`
- blocked_by: `register-md5-navigation`
- source_plan: `../plan.md`
- source_plan_digest: `b3abab4c25876cd7f28dc2ccbf97da0bc079bc75b9885acfe3ff0226e35a8b29`
- write_scope: `.ai-work-flow/index/feature-navigation.md, .ai-work-flow/index/frontend-navigation.md, src/lib/md5.ts, src/lib/md5.test.ts, src/components/SettingsView.tsx, src/components/SettingsView.test.tsx, src/components/Md5EncryptionTool.tsx, src/components/Md5EncryptionTool.test.tsx, src/i18n.ts, src/lib/navigation.ts, src/lib/navigation.test.ts, src/components/MoreToolsHub.tsx, src/components/MoreToolsHub.test.tsx, src/components/Launcher.tsx, src/components/Launcher.test.tsx, src/lib/launcherToolVisibility.ts, src/lib/launcherToolVisibility.test.ts, src/lib/moreToolPresentation.ts, src/App.tsx, src/App.moreToolsNavigation.test.tsx`

## Outcome

项目索引准确记录 MD5 能力与导航链路，全部定向及全量质量命令通过，并完成有记录的桌面/窄屏、中英文视觉验收。

## Implementation Checklist

- [x] 更新 `feature-navigation.md`，记录 MD5 文本工具能力、`Md5EncryptionTool`、共享 `md5Hex` 模块、设置页复用关系及用户可达入口。
- [x] 更新 `frontend-navigation.md`，记录 `md5-encryption` 别名解析、More Tools 卡片/详情分发、Launcher 入口、`md5Encryption` 可见性字段及详情返回流程。
- [x] 依次运行计划列出的全部定向 Vitest、`npm run test`、`npm run lint` 和 `npm run build`；仅在本计划 `write_scope` 内修复发现的问题并重跑受影响验证。
- [x] 启动仓库现有前端开发环境；仅在当前运行获得浏览器自动化授权后使用优先无头方式，否则按计划使用开发环境人工截图并记录验收结果。
- [x] 分别以桌面宽度和窄屏宽度检查英文、中文、初始空状态、已计算空字符串、四项结果与复制按钮、长输入、空白输入、重算、复制失败保态及清空后焦点。
- [x] 对发现的重叠、裁切、页面级横向溢出、焦点或文案问题在既定功能文件内做最小修复，并重新运行相关测试、lint、build 与对应截图检查。
- [x] 检查依赖、网络、Tauri/Rust 和托盘变更边界，完成本任务 checklist 并记录命令及视觉验收结论。

## Acceptance Criteria

- [x] 两份索引均准确列出共享算法、组件、唯一工具 ID、三个用户入口、可见性兼容规则及返回链路，且不声称存在托盘专属入口。
- [x] 计划涉及的算法、SettingsView、Md5EncryptionTool、MoreToolsHub、Launcher、App、导航与可见性定向测试全部通过。
- [x] `npm run test`、`npm run lint` 和 `npm run build` 均以退出码 0 完成。
- [x] 桌面与窄屏、中英文验收均覆盖规定状态；四个结果行和图标按钮无重叠、裁切或页面级横向溢出，长摘要可读且按钮可操作，清空后输入获得焦点。
- [x] 视觉验收记录明确说明采用经授权的无头自动化还是人工截图，不在未授权情况下调用浏览器自动化。
- [x] 最终变更不包含新增 npm 依赖、后端能力、网络调用、Tauri command、系统权限或 `src-tauri/src/app_runtime/shortcuts_tray.rs` 修改。

## Verification Steps

- [x] 使用仓库现有 Vitest 调用方式一次性运行本计划全部定向测试文件，预期退出码 0。
- [x] 依次运行 `npm run test`、`npm run lint`、`npm run build`，预期每条命令退出码均为 0。
- [x] 运行现有开发脚本并完成桌面与窄屏截图检查，预期所有规定状态在中英文下可读、可操作且布局稳定；保存或记录截图位置与检查结论。
- [x] 检查最终变更文件及依赖清单，预期仅包含计划内前端、测试和索引文件，且无托盘、Rust/Tauri、网络或依赖变更。

## Verification Evidence

本轮当前请求已明确授权无头浏览器自动化；仅启动 Vite Web 前端，未启动可见浏览器或 Tauri 壳。

### Commands

| 命令 | 退出码 | 结果 |
|---|---:|---|
| `git branch --show-current && git rev-parse HEAD` | 0 | 分支为 `ai-work-flow/add-md5-encryption-tool-7f82a047-task-04-verify-and-index`，HEAD 为 `16ae29c9a8db22114f87ece7fc4b8eab32610f98` |
| `git status --porcelain=v2 -z --untracked-files=all`（开始门禁） | 0 | 空输出 |
| `npm run test -- src/lib/md5.test.ts src/components/SettingsView.test.tsx src/components/Md5EncryptionTool.test.tsx src/lib/navigation.test.ts src/components/MoreToolsHub.test.tsx src/components/Launcher.test.tsx src/lib/launcherToolVisibility.test.ts src/App.moreToolsNavigation.test.tsx` | 0 | 8 个测试文件通过，91 个测试通过 |
| `npm run test` | 0 | 29 个测试文件通过，203 个测试通过 |
| `npm run lint` | 0 | 0 errors，386 个仓库既有 warnings |
| `npm run build` | 0 | `tsc -b && vite build` 成功，2589 个模块转换完成；仅有既有 chunk size 提示 |
| `git diff --check` | 0 | 空输出 |
| `npm run dev -- --host 127.0.0.1 --port 4178` + `curl -fsS http://127.0.0.1:4178/` | 0 | Vite Web 前端可访问；监听 PID 78062，验收后已停止，`lsof -nP -iTCP:4178 -sTCP:LISTEN` 空输出 |

### Visual Records

截图均位于浏览器会话输出目录 `/Users/yuqiyu/AiHistorys/one-space/onespace-app/.playwright-mcp/`，不在本任务 worktree 中。

- `page-2026-07-31T04-42-18-800Z.png`：1800x987，中文 Launcher；MD5 固定入口卡片可见、文案完整。
- `page-2026-07-31T04-42-33-113Z.png`：1800x987，中文，从 Launcher 进入后的初始空状态；安全说明、输入与操作区、空结果文案完整。
- `page-2026-07-31T04-43-13-785Z.png`：1800x987，中文，显式计算空字符串；四项结果及四个复制图标按钮完整。
- `page-2026-07-31T04-43-55-494Z.png`：1800x987，中文，常规中文/英文/换行输入重算；四项结果同步替换。
- `page-2026-07-31T04-44-30-060Z.png`：1800x987，中文，318 字符长输入（含空格、制表符、换行）重算；输入与结果可读，无页面横向溢出。
- `task04-desktop-zh-copy-failure.png`：1800x987，中文，仅空白/制表符/换行结果；强制复制失败显示“无法复制32 位小写。”，原输入及四项结果保留。已重新读取图片并实际检查。
- `page-2026-07-31T04-48-27-501Z.png`：1800x987，英文初始空状态；标题、安全说明、按钮与空状态文案完整。
- `page-2026-07-31T04-49-29-741Z.png`：1800x987，英文常规换行输入结果；四项英文标签、摘要与图标按钮完整。
- `page-2026-07-31T04-50-42-177Z.png`：1800x987，英文 More Tools Hub；MD5 卡片入口可见、文案完整。
- `page-2026-07-31T04-52-46-188Z.png`：390x844，英文直接别名进入后的初始空状态；响应式标题、安全说明、输入与按钮无裁切。
- `page-2026-07-31T04-53-16-689Z.png`：390x844，英文显式计算空字符串；窄屏结果行与按钮可见，无页面横向溢出。
- `element-2026-07-31T04-53-29-562Z.png`：390px 窄屏英文四项结果区域；四行和全部复制按钮实际可见、无重叠。
- `page-2026-07-31T04-54-38-705Z.png`：390x844，英文 276 字符长输入重算；文本框内部纵向滚动，结果完整，无页面横向溢出。
- `element-2026-07-31T04-54-50-706Z.png`：390px 窄屏英文长输入与四项结果区域；长文本可读，四行稳定。
- `page-2026-07-31T04-56-55-894Z.png`：390x844，中文清空后的初始状态；输入为空、结果移除、输入框焦点环可见。
- `element-2026-07-31T04-57-25-220Z.png`：390px 窄屏中文空字符串四项结果；四行和全部复制按钮无重叠或裁切。
- `element-2026-07-31T04-58-37-119Z.png`：390px 窄屏中文 173 字符长输入重算；输入框内部纵向滚动，四项结果完整。
- `task04-mobile-zh-four-copy-success.png`：390x844，中文，四个复制按钮依次操作；四条成功反馈与对应结果保留。已重新读取图片并实际检查。

### DOM And Behavior

- 1800x987 桌面：`scrollWidth=clientWidth=1800`；结果行 `1496x66`，摘要区宽 `1426`，复制按钮 `32x32`；四行 `overlap=false`、`clipped=false`。
- 390x844 窄屏：`scrollWidth=clientWidth=390`；结果行 `342x66`，摘要区宽 `272`，复制按钮 `32x32`；中英文四行均 `overlap=false`、`clipped=false`。
- 长输入：桌面文本框 `scrollWidth=clientWidth=1460`；窄屏中英文文本框 `scrollWidth=clientWidth=306`，仅出现预期的内部纵向滚动，页面横向溢出均为 0。
- 输入从空字符串改为常规文本、长文本或仅空白/制表符/换行时，旧的四项结果保持不变；再次点击计算后四项一次性同步替换。
- 空字符串结果为 `d41d8cd98f00b204e9800998ecf8427e`；四项依次为 32 位小写、32 位大写、16 位小写、16 位大写，16 位值为 `[8, 24)`。
- 四个复制按钮均有对应的 `aria-label`/`title`，未禁用；成功桩依次收到四项准确值并显示四条成功反馈。
- 失败桩收到准确的 32 位小写值并拒绝 Promise；50ms 内捕获失败 Toast，`inputPreserved=true`、`resultsPreserved=true`。
- 点击清空后 `input=""`、结果行数为 0、`document.activeElement.id="md5-encryption-input"`。
- Launcher 卡片进入后返回按钮为 “Back to Launcher”；More Tools 卡片进入后返回按钮为 “Back to tools” 并返回 Hub；`window.setActiveTab('md5-encryption')` 直接别名进入同一详情。
- 无头浏览器控制台最终为 0 errors。

### Acceptance Evidence

1. 两份索引新增 MD5 行，记录 `md5Hex`/`Md5EncryptionTool`、唯一 ID、More Tools/Launcher/直接别名、`md5Encryption: true` 与旧配置逐字段 boolean 合并、App 返回语义，并明确纯前端、无后端、无托盘入口。
2. 定向 Vitest 8 文件、91 测试全部通过，覆盖算法、SettingsView、Md5EncryptionTool、导航、MoreToolsHub、Launcher、可见性与 App。
3. 全量测试 29 文件/203 测试、lint（0 errors）和 build 均退出码 0。
4. 上述桌面/窄屏、中英文截图及 DOM 数据覆盖空状态、空字符串、常规/长/空白输入、重算、四项结果、复制成功/失败、保态与清空聚焦；未发现重叠、裁切或页面级横向溢出。
5. 本轮使用用户明确授权的无头 Playwright 自动化；未启动可见浏览器或 Tauri 壳。
6. 未做源码修复；仅更新两份索引和本 task04 证据。无新增依赖、网络调用、后端/Tauri command、系统权限、Rust 或托盘变更。

## Out of Scope

不新增自动化截图基础设施，不在未授权时操作浏览器，不扩展 MD5 以外功能，不修改计划文件、系统托盘、Rust/Tauri、依赖或外部接口。
