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

- [ ] 更新 `feature-navigation.md`，记录 MD5 文本工具能力、`Md5EncryptionTool`、共享 `md5Hex` 模块、设置页复用关系及用户可达入口。
- [ ] 更新 `frontend-navigation.md`，记录 `md5-encryption` 别名解析、More Tools 卡片/详情分发、Launcher 入口、`md5Encryption` 可见性字段及详情返回流程。
- [ ] 依次运行计划列出的全部定向 Vitest、`npm run test`、`npm run lint` 和 `npm run build`；仅在本计划 `write_scope` 内修复发现的问题并重跑受影响验证。
- [ ] 启动仓库现有前端开发环境；仅在当前运行获得浏览器自动化授权后使用优先无头方式，否则按计划使用开发环境人工截图并记录验收结果。
- [ ] 分别以桌面宽度和窄屏宽度检查英文、中文、初始空状态、已计算空字符串、四项结果与复制按钮、长输入、空白输入、重算、复制失败保态及清空后焦点。
- [ ] 对发现的重叠、裁切、页面级横向溢出、焦点或文案问题在既定功能文件内做最小修复，并重新运行相关测试、lint、build 与对应截图检查。
- [ ] 检查依赖、网络、Tauri/Rust 和托盘变更边界，完成本任务 checklist 并记录命令及视觉验收结论。

## Acceptance Criteria

- [ ] 两份索引均准确列出共享算法、组件、唯一工具 ID、三个用户入口、可见性兼容规则及返回链路，且不声称存在托盘专属入口。
- [ ] 计划涉及的算法、SettingsView、Md5EncryptionTool、MoreToolsHub、Launcher、App、导航与可见性定向测试全部通过。
- [ ] `npm run test`、`npm run lint` 和 `npm run build` 均以退出码 0 完成。
- [ ] 桌面与窄屏、中英文验收均覆盖规定状态；四个结果行和图标按钮无重叠、裁切或页面级横向溢出，长摘要可读且按钮可操作，清空后输入获得焦点。
- [ ] 视觉验收记录明确说明采用经授权的无头自动化还是人工截图，不在未授权情况下调用浏览器自动化。
- [ ] 最终变更不包含新增 npm 依赖、后端能力、网络调用、Tauri command、系统权限或 `src-tauri/src/app_runtime/shortcuts_tray.rs` 修改。

## Verification Steps

- [ ] 使用仓库现有 Vitest 调用方式一次性运行本计划全部定向测试文件，预期退出码 0。
- [ ] 依次运行 `npm run test`、`npm run lint`、`npm run build`，预期每条命令退出码均为 0。
- [ ] 运行现有开发脚本并完成桌面与窄屏截图检查，预期所有规定状态在中英文下可读、可操作且布局稳定；保存或记录截图位置与检查结论。
- [ ] 检查最终变更文件及依赖清单，预期仅包含计划内前端、测试和索引文件，且无托盘、Rust/Tauri、网络或依赖变更。

## Out of Scope

不新增自动化截图基础设施，不在未授权时操作浏览器，不扩展 MD5 以外功能，不修改计划文件、系统托盘、Rust/Tauri、依赖或外部接口。
