# Workspaces Card Density Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在保留完整信息的前提下压缩工作空间列表卡片尺寸与内部布局密度。

**Architecture:** 只修改工作空间列表卡片的 JSX 与 Tailwind 样式，不改变工作空间详情视图、数据结构或交互入口。通过重新分配头部、描述、标签和底部信息区域来减少无效留白。

**Tech Stack:** React 19, TypeScript, Tailwind CSS, lucide-react

---

### Task 1: 重排工作空间列表卡片结构

**Files:**
- Modify: `src/components/Workspaces/index.tsx`

- [ ] 调整列表网格断点，让超宽屏支持更高密度展示。
- [ ] 重写卡片头部排版，压缩标题、来源 badge、路径和图标操作的空间占用。
- [ ] 收紧描述、标签和底部信息区的间距与字号，保留全部信息字段。
- [ ] 缩小 `New AI Session` 按钮尺寸并保持可点击性。

### Task 2: 验证展示结果

**Files:**
- Modify: `src/components/Workspaces/index.tsx`

- [ ] 检查卡片在无描述、无标签、多标签场景下的布局稳定性。
- [ ] 运行 `npm run build`，确认 UI 改动未引入编译问题。
