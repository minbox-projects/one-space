# 03 - 注册 MD5 入口、导航与可见性

- task_id: `register-md5-navigation`
- order: `03`
- blocked_by: `build-md5-tool`
- source_plan: `../plan.md`
- source_plan_digest: `b3abab4c25876cd7f28dc2ccbf97da0bc079bc75b9885acfe3ff0226e35a8b29`
- write_scope: `src/lib/navigation.ts, src/lib/navigation.test.ts, src/components/MoreToolsHub.tsx, src/components/MoreToolsHub.test.tsx, src/components/Launcher.tsx, src/components/Launcher.test.tsx, src/lib/launcherToolVisibility.ts, src/lib/launcherToolVisibility.test.ts, src/lib/moreToolPresentation.ts, src/App.tsx, src/App.moreToolsNavigation.test.tsx`

## Outcome

MD5 工具以唯一 ID `md5-encryption` 从 More Tools、Launcher 和应用内别名导航可达，并以向后兼容的 `md5Encryption` 可见性偏好参与现有详情进入与返回流程。

## Implementation Checklist

- [ ] 在 `MoreToolsSection` 与 `MORE_TOOLS_ALIAS_MAP` 注册唯一内部 ID `md5-encryption`，不创建其他 ID 或 MD5 专属顶层页面状态。
- [ ] 在 `MoreToolsHub` 增加 MD5 卡片、`Md5EncryptionTool` 详情分发、Launcher 可见性判断和现有返回 Launcher/More Tools 逻辑接入。
- [ ] 在 `moreToolPresentation` 为同一 ID 分配现有 Lucide 哈希/二进制语义图标及现有可用色系，不扩展设计系统。
- [ ] 在 `Launcher.quickInternalTools` 增加 MD5 快捷入口，目标固定为 `md5-encryption`，并遵循现有可见性过滤与导航方式。
- [ ] 在 `LauncherToolId`、全量 `LauncherToolVisibility` 和默认值中增加 `md5Encryption: true`。
- [ ] 将持久化读取固定为“当前完整默认值作为基底，旧对象中每个已知字段仅以有效显式布尔值覆盖”的逐字段合并；旧配置缺失新字段、配置不存在或 JSON 损坏时采用现有错误策略并使 MD5 默认可见，其他显式偏好不得丢失。
- [ ] 在 `App` 复用 `navigateToTab`、`handleSelectMoreTool` 与 `handleMoreToolsBack`，确保统一 ID 可直接进入详情并沿既有流程返回 More Tools 上下文。
- [ ] 更新对应导航、Hub、Launcher、可见性和 App 测试，覆盖卡片/详情分发、显示开关、统一 ID 进入、直接别名解析及返回流程。
- [ ] 完成本任务 checklist，并只提交 `write_scope` 内的实现与测试改动；确认未修改 `src-tauri/src/app_runtime/shortcuts_tray.rs`。

## Acceptance Criteria

- [ ] More Tools 在 `md5Encryption: true` 时显示 MD5 卡片并分发 `Md5EncryptionTool`，在 `false` 时按现有规则隐藏对应入口。
- [ ] Launcher 快捷入口使用 `md5-encryption`，可进入同一 More Tools 详情；应用内直接导航别名也解析到该详情，返回后处于既有 More Tools/Launcher 上下文。
- [ ] `MoreToolsSection`、别名映射、Hub 分发、Launcher 目标和展示元数据只使用 `md5-encryption`；可见性只使用 `md5Encryption`。
- [ ] 缺少 `md5Encryption` 的旧 localStorage 对象读取后补全为 `true`，其他工具原有显式 `true`/`false` 均保持；空配置和损坏配置沿用现有回退行为。
- [ ] 图标和颜色来自现有展示体系，未新增 npm 依赖、全局框架、顶层路由状态、Tauri command 或系统托盘入口。
- [ ] 导航、Hub、Launcher、App 和可见性相关回归测试全部通过，既有工具入口与偏好行为不回归。

## Verification Steps

- [ ] 按仓库现有 Vitest 调用方式定向运行 `src/lib/navigation.test.ts`、`src/components/MoreToolsHub.test.tsx`、`src/components/Launcher.test.tsx`、`src/lib/launcherToolVisibility.test.ts` 和 `src/App.moreToolsNavigation.test.tsx`，预期全部通过。
- [ ] 定向运行 `src/components/Md5EncryptionTool.test.tsx`，预期注册集成未改变组件独立行为。
- [ ] 运行涉及文件的 TypeScript/lint 检查，预期无穷尽分支、类型字段或未使用导入错误。
- [ ] 检查变更文件列表，预期不包含 `src-tauri/src/app_runtime/shortcuts_tray.rs`。

## Out of Scope

不增加系统托盘项，不修改 Rust/Tauri、外部 API、协议、权限或依赖，不实现文件哈希、批量处理、历史记录或其他摘要算法，也不在本任务执行最终截图验收。
