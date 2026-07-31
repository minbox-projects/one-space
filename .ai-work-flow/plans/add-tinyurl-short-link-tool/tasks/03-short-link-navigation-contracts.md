# 03 - 扩展导航契约与国际化资源

- task_id: `short-link-navigation-contracts`
- order: `03`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `bd1294e58912b10cedf6e2f834807758b2ebaf29b9f2f71d80a8caaa70a145e4`
- write_scope: `src/lib/navigation.ts；src/lib/moreToolPresentation.ts；src/lib/launcherToolVisibility.ts；src/lib/launcherToolVisibility.test.ts；src/i18n.ts`

## Outcome

`short-link` 成为导航、More Tools 展示、Launcher 可见性和中英文资源中的完整稳定成员，且现有穷举类型和持久化兼容行为保持有效。

## Implementation Checklist

- [x] 将稳定 ID `short-link` 加入 `MoreToolsSection`，在 alias map 中让 canonical alias `short-link` 解析到同名 section，不增加其他公开 ID。
- [x] 在 `moreToolPresentation` 的穷举 `Record` 中加入短链接展示项，使用现有 Lucide `Link` 图标和青绿色 `teal` 强调色，沿用当前字段及 class 约定。
- [x] 将 `short-link` 加入 Launcher 工具可见性联合类型和默认集合，默认值为可见。
- [x] 保留现有持久化设置的默认合并行为，使旧设置缺少新 ID 时自动获得默认可见值。
- [x] 在 `src/i18n.ts` 增加“生成短链接”/“Short Link”的名称、简介、表单、凭据状态、替换/删除、历史、确认、复制及本地记录边界文案。
- [x] 为九个稳定后端错误代码、剪贴板失败、历史读取/损坏恢复/写入失败提供中英文文案。
- [x] 文案明确删除和清空的是“本地记录”，不得暗示 TinyURL 远端链接失效。
- [x] 更新可见性测试，覆盖默认值、旧持久化值合并、显式隐藏和重新显示。

## Acceptance Criteria

- [x] `MoreToolsSection`、alias map、展示 `Record` 和 Launcher 可见性类型均包含且只使用稳定 ID `short-link`。
- [x] `moreToolPresentation` 对全部 section 保持类型穷举，短链接使用 `Link` 图标和独立 teal 强调色。
- [x] `[SC-1/可见性]` 新安装默认显示短链接工具；旧可见性设置在合并默认值后也显示该工具，用户仍可显式隐藏。
- [x] 中英文资源覆盖 Token 配置、生成、当前结果、历史、确认、复制和全部错误状态，组件无需硬编码语言分支。
- [x] `[SC-6]` 删除文案明确限定为当前设备本地记录，不表示远端链接被删除或失效。
- [x] 既有工具 ID、路由 alias 和持久化值保持兼容。

## Verification Steps

- [x] 运行 `npm run test -- src/lib/launcherToolVisibility.test.ts`，默认值和持久化兼容测试通过。
- [x] 运行 `npm run build`，确认导航及展示穷举类型完整。
- [x] 检查 `src/i18n.ts` 的中英文资源，确认所有新增 key 成对存在且不存在 Token 明文展示文案。

## Out of Scope

不修改 More Tools Hub、Launcher 或 App 的渲染逻辑，不实现短链接页面和后端命令，不改变既有工具 ID。
