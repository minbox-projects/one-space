# 05 - 接入应用导航与工具入口

- task_id: `short-link-shell-integration`
- order: `05`
- blocked_by: `short-link-navigation-contracts, short-link-tool-ui`
- source_plan: `../plan.md`
- source_plan_digest: `bd1294e58912b10cedf6e2f834807758b2ebaf29b9f2f71d80a8caaa70a145e4`
- write_scope: `src/App.tsx；src/components/MoreToolsHub.tsx；src/components/Launcher.tsx；src/components/MoreToolsHub.test.tsx；src/components/Launcher.test.tsx；src/App.moreToolsNavigation.test.tsx`

## Outcome

用户可从 More Tools 卡片或 Launcher 搜索并打开 Short Link 页面，并通过现有返回、标题、面包屑和可见性流程正常导航。

## Implementation Checklist

- [ ] 在 `MoreToolsHub` 增加 `short-link` 卡片和详情分发，渲染 `ShortLinkTool`。
- [ ] 保持 More Tools 现有卡片排序与交互约定，并使用展示映射提供的图标、强调色、名称和简介。
- [ ] 在 `Launcher` 增加可按中英文名称及简介搜索的短链接条目。
- [ ] Launcher 打开操作导航到稳定 section `short-link`，并遵守工具可见性设置。
- [ ] 在 `App.tsx` 补齐 Short Link 标题、面包屑、详情页和返回 More Tools 所需映射。
- [ ] 更新 More Tools Hub 测试，覆盖卡片显示、详情分发和返回行为。
- [ ] 更新 Launcher 测试，覆盖中英文搜索、打开行为以及隐藏后不可见。
- [ ] 更新 App 导航测试，覆盖由 More Tools 和 Launcher 进入、标题/面包屑及返回流程。
- [ ] 保持既有工具入口、路由和测试行为不变。

## Acceptance Criteria

- [ ] `[SC-1/导航]` More Tools 显示“生成短链接”卡片，点击后打开 Short Link 工具，返回行为与现有详情页一致。
- [ ] `[SC-1/Launcher]` Launcher 可通过中英文内容搜索并打开该工具，显式隐藏后不展示。
- [ ] `[SC-1/App]` 工具标题、面包屑、详情分发和返回目标正确使用稳定 ID `short-link`。
- [ ] `[SC-8/导航测试]` More Tools、Launcher、App 导航和可见性相关回归测试均覆盖新工具。
- [ ] 现有工具卡片、Launcher 条目、导航 ID 和返回行为没有回归。

## Verification Steps

- [ ] 运行 `npm run test -- src/components/MoreToolsHub.test.tsx src/components/Launcher.test.tsx src/App.moreToolsNavigation.test.tsx`。
- [ ] 运行 `npm run test -- src/lib/launcherToolVisibility.test.ts`，确认入口行为与可见性契约一致。
- [ ] 运行 `npm run build`，确认 `short-link` 已纳入全部穁举导航和展示分支。

## Out of Scope

不修改 Short Link 页面内部逻辑、Rust 命令、历史 schema、凭据存储或现有工具的设计和行为。
