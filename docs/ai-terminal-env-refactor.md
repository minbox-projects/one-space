# AI 终端环境界面重构方案

## Context

当前 `AiEnvironments/index.tsx` 是一个 3077 行的单体组件，采用左侧列表（w-80）+ 右侧详情的 master-detail 布局。Claude Profile 使用卡片式列表，Codex/Gemini/OpenCode 使用简单列表项。需要将布局重构为**全宽手风琴列表**方案，所有工具统一风格，同时保留顶部 CLI 版本卡片的切换功能。

## 设计原型（AI 执行时务必参考）

| 工具 | 原型文件 | 打开方式 |
|---|---|---|
| Claude Profile | `docs/reports/20260601/claude-profile-layout-c-refined.html` | 浏览器直接打开 |
| Codex / Gemini / OpenCode | `docs/reports/20260601/claude-profile-layout-c-tools.html` | 浏览器直接打开 |

## 关键约束

1. **数据层不变**：`claudeProfiles`、`state.providers`、Tauri 命令调用全部保留
2. **CLI 版本卡片不变**：顶部 4 列网格保持现有功能和样式
3. **i18n 保留**：所有现有翻译 key 继续使用，新增少量 UI 文本
4. **Handler 保留**：所有事件处理函数逻辑不变
5. **导入弹窗不变**：现有的导入预览弹窗（2852-3073 行）保持原有实现，不重构
6. **ConfirmDialog 不变**：继续使用 `useConfirmDialog` 做删除确认等操作

---

## 实施步骤

### 步骤 1：拆分巨型组件为子组件

**文件**：在 `src/components/AiEnvironments/` 下新建文件

| 新文件 | 内容 | 行数预估 |
|---|---|---|
| `CliVersionCards.tsx` | 顶部 4 列 CLI 版本卡片（从现有 1507-1640 行提取） | ~120 |
| `AccordionItem.tsx` | 单个手风琴行组件（折叠行 + 展开面板容器） | ~80 |
| `ClaudeProfilePanel.tsx` | Claude Profile 展开后的编辑表单 | ~300 |
| `ProviderPanel.tsx` | Codex/Gemini/OpenCode 展开后的编辑表单 | ~400 |
| `SyncedDevices.tsx` | 同步设备区域 | ~100 |
| `ToolSectionHeader.tsx` | 操作栏（搜索+导入/导出/新建）+ 筛选 Pill 组合 | ~80 |
| `index.tsx` | 主组件（容器），组合子组件 + 保留所有 state/handler | ~500 |

**拆分策略**：
- 所有 state、hooks、handler **保留在 `index.tsx` 主组件**中
- 通过 props 将数据和回调传递给子组件
- 子组件只负责渲染，不包含业务逻辑
- 不拆分过度：操作栏和筛选 pill 合并为一个 `ToolSectionHeader`

### 新增状态

| 状态 | 类型 | 用途 |
|---|---|---|
| `openIds` | `Set<string>` | 当前展开的手风琴行 ID 集合（支持多开） |
| `handleToggleOpen` | `(id: string) => void` | 切换展开/折叠 |

### 步骤 2：重构 Claude Profile 列表区域

**当前**（1697-1798 行）：左侧 w-80 侧栏内的 Profile 卡片
**目标**：全宽手风琴列表

关键变更：
- 移除左侧 w-80 侧栏容器（`<div className="w-80 border-r ...">`）
- 移除右侧详情面板容器（`<div className="flex-1 flex flex-col ...">`）
- 替换为全宽 `<div className="accordion">` 容器
- 每个 `claudeProfiles` 项渲染为一个 `<AccordionItem>`：
    - **折叠行**：头像（渐变色）+ 名称 + default 徽章 + 认证 badge（API Key/OAuth）+ 权限 badge + model badge + 配置目录（等宽字体）+ 操作按钮（启动/复制/目录）
    - **展开面板**：包裹在白色圆角卡片内，分组展示：基本信息、认证&端点、模型路由、权限&高级、工作空间隔离
- 保留 Profile 特有的操作：`handleClaudeLaunch`、`handleClaudeCopyCommand`、`handleClaudeOpenDir`、`handleClaudeSetDefault`、`handleClaudeMaterialize`、`handleClaudeApplyGlobal`
- 展开面板上方保留两个提示框（从现有 2100-2126 行）：
    - **Imported but Inactive**：黄色警告卡片（当有导入但未激活的 Provider 时显示）
    - **Scope 提示框**：橙色提示框，解释 Claude Profile 配置的作用范围
- 当 `claudeProfiles.length === 0` 时，展示空状态引导（"No profiles configured" + 新建按钮）
- **新建 Profile**：点击 `handleAddCustom('claude')` 后，自动创建新 Provider 并自动展开对应手风琴行

### 步骤 3：重构 Codex/Gemini/OpenCode 列表区域

**当前**（1800-1812 行）：左侧 w-80 侧栏内的简单列表项
**目标**：全宽手风琴列表

关键变更：
- 与 Claude 使用相同的手风琴结构
- 每个 Provider 渲染为一个 `<AccordionItem>`：
    - **折叠行**：活跃圆点（绿色=活跃/灰色=未活跃）+ 工具图标头像 + 名称 + 认证 badge + model badge + 操作按钮（应用/复制）
    - **展开面板**：
        - Codex：基本信息、认证&端点、模型配置、高级选项、Reasoning 配置、审批&沙箱
        - Gemini：基本信息、认证&端点、认证方式、模型配置、行为配置
        - OpenCode：基本信息、认证&端点、全局配置、高级配置、JSON 编辑器
- 修复现有代码中的重复字段 bug（Codex 2372-2415 行、Gemini 2490-2516 行的重复字段）
- 当 Provider 列表为空时，展示空状态引导（"No providers configured" + 新建按钮）
- **新建 Provider**：点击 `handleAddCustom(tool)` 后，自动创建并自动展开对应手风琴行
- **OpenCode JSON 编辑器**：保留 `react-simple-code-editor` + `prismjs` JSON 语法高亮，保留"AI 历史"弹窗（`historyRef` + `handleClickOutside`），保留格式化/回滚功能

### 步骤 4：统一布局结构

**当前布局**：
```
flex flex-col h-full space-y-6
  ├─ Header
  ├─ CLI Version Cards (border rounded-xl)
  └─ flex-1 flex border rounded-xl (master-detail)
       ├─ Left Sidebar (w-80)
       └─ Right Panel (flex-1)
```

**目标布局**：
```
flex flex-col h-full space-y-4
  ├─ Header
  ├─ CLI Version Cards（不变，点击切换 activeTool）
  ├─ ToolSectionHeader（搜索 + 导入/导出/新建 + 筛选 Pill）
  └─ Accordion（全宽手风琴列表，可滚动）
       ├─ Imported-but-Inactive 警告卡片（条件显示，仅 Claude）
       ├─ Scope 提示框（条件显示，仅 Claude）
       ├─ Claude Profiles（当 activeTool === 'claude'）
       ├─ Codex Providers（当 activeTool === 'codex'）
       ├─ Gemini Providers（当 activeTool === 'gemini'）
       └─ OpenCode Providers（当 activeTool === 'opencode'）
  └─ SyncedDevices（底部，条件显示：当有同步设备时）
  └─ Import Modal（不变，导入预览弹窗）
```

### 步骤 5：新增 CSS 样式

使用 Tailwind CSS 内联类 + 少量自定义 CSS。优先使用 Tailwind 类，只在 Tailwind 无法表达时使用自定义 CSS 类（放在 `src/index.css` 中）。

**自定义 CSS 类**（参考 `src/index.css` 中已有的 shadcn/ui CSS 变量）：

```css
/* 手风琴容器 & 行 — 大部分用 Tailwind，少量自定义 */
.acc-item { border-bottom: 1px solid hsl(var(--border)); }
.acc-item.open { background: hsl(var(--primary) / 0.05); }
.acc-panel { display: none; padding: 0 18px 20px 72px; }
.acc-item.open .acc-panel { display: block; }
```

**优先使用的 Tailwind 类**（原型中对应的 HTML 类 → Tailwind 映射）：

| 原型类 | Tailwind 类 |
|---|---|
| `.accordion` | `border rounded-xl overflow-hidden bg-card shadow-sm` |
| `.accordion-header` | `px-4 py-3 border-b bg-muted/30` |
| `.acc-row` | `flex items-center px-4 py-3.5 gap-3.5 cursor-pointer min-h-[60px]` |
| `.acc-avatar` | `w-10 h-10 rounded-lg flex items-center justify-center font-bold text-base` |
| `.acc-avatar.c1` | `bg-gradient-to-br from-blue-50 to-blue-100 text-blue-800` |
| `.badge-blue` | `inline-flex items-center gap-0.5 text-[10px] font-medium px-1.5 py-0.5 rounded-full bg-primary/10 text-primary` |
| `.acc-btn` | `h-7 px-2.5 text-xs font-medium rounded-md border bg-card text-muted-foreground hover:bg-muted` |
| `.acc-btn-launch` | `h-7 px-2.5 text-xs font-medium rounded-md bg-foreground text-background hover:opacity-90` |
| `.acc-panel-inner` | `bg-card border rounded-lg p-5` |
| `.field-grid` | `grid grid-cols-3 gap-3` |
| `.field-grid.col-2` | `grid grid-cols-2 gap-3` |
| `.field-grid.col-4` | `grid grid-cols-4 gap-3` |
| `.checkbox-row.info` | `flex items-start gap-2.5 p-2.5 rounded-md bg-primary/5 border border-primary/20` |
| `.checkbox-row.warn` | `flex items-start gap-2.5 p-2.5 rounded-md bg-destructive/5 border border-destructive/20` |

### 步骤 6：修复现有代码 bug

| Bug | 位置 | 修复方式 |
|---|---|---|
| 重复的 mousedown useEffect | 388-396 + 601-609 | 删除第二个 |
| Codex 重复字段 | 2372-2415 | 删除重复的 reasoningSummary/approvalPolicy/sandboxMode |
| Gemini 重复字段 | 2490-2516 | 删除重复的 vimMode/defaultApprovalMode |

---

## 文件变更清单

| 文件 | 操作 | 说明 |
|---|---|---|
| `src/components/AiEnvironments/index.tsx` | 重写 | 主组件重构为容器组件，约 500 行 |
| `src/components/AiEnvironments/CliVersionCards.tsx` | 新建 | CLI 版本卡片组件 |
| `src/components/AiEnvironments/AccordionItem.tsx` | 新建 | 手风琴行组件（折叠/展开容器） |
| `src/components/AiEnvironments/ClaudeProfilePanel.tsx` | 新建 | Claude 编辑表单 |
| `src/components/AiEnvironments/ProviderPanel.tsx` | 新建 | Codex/Gemini/OpenCode 编辑表单 |
| `src/components/AiEnvironments/SyncedDevices.tsx` | 新建 | 同步设备区域 |
| `src/components/AiEnvironments/ToolSectionHeader.tsx` | 新建 | 操作栏 + 筛选 Pill 组合 |
| `src/components/AiEnvironments/icons.tsx` | **不变** | 保留现有 4 个工具图标 |
| `src/index.css` | 增量 | 新增 `.acc-item`、`.acc-item.open`、`.acc-panel` 等少量类 |
| `src/i18n.ts` | 增量 | 新增少量翻译 key（搜索 placeholder、筛选标签等） |

**明确不变更的文件**：
- `src/components/AiEnvironments/icons.tsx` — 工具图标不动
- 导入预览弹窗相关代码（保留在 `index.tsx` 中，不拆分）
- `ConfirmDialogProvider` — 继续使用现有确认对话框
- 所有 Tauri 后端命令 — 不需要后端改动

---

## 验证方案

1. **手动验证**：
    - 启动应用，进入 AI 终端环境页面
    - 切换 Claude/Codex/Gemini/OpenCode 工具卡片，确认各自的手风琴列表正常展示
    - 点击展开/折叠每个 Profile/Provider，确认多开同时展开多个
    - 修改字段并保存，确认数据持久化
    - 测试新建 Provider 后自动展开
    - 测试删除 Provider
    - 测试 Claude Profile 的启动、复制命令、打开目录、设为默认功能
    - 测试 OpenCode JSON 编辑器的格式化、AI 历史弹窗、回滚功能
    - 测试导入/导出功能（导入弹窗保持原有实现）
    - 测试同步设备的导入激活功能
    - 空状态：当某工具无 Provider 时，显示引导文案

2. **代码验证**：
    - TypeScript 编译通过（`npx tsc --noEmit`）
    - ESLint 无报错
    - 检查所有 i18n key 都有对应的中英文翻译
    - 确认 `src/index.css` 中的 CSS 变量与 shadcn/ui token 一致

3. **响应式验证**：
    - 窗口宽度 ≥ 1200px：4 列字段网格、配置目录可见
    - 窗口宽度 900px：CLI 卡片变为 2 列、字段网格变为 2 列、配置目录隐藏
    - 窗口宽度 640px：CLI 卡片变为 1 列、字段网格变为 1 列、操作栏纵向排列