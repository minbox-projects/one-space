# OneSpace Skills 使用文档

本文面向日常使用，重点覆盖 Skills 的安装、更新、导入与同步流程。

## 1. 功能定位

Skills 是可复用的任务能力包（通常包含 `SKILL.md` 及相关文件），可按模型安装并同步到对应 CLI 环境，帮助你在不同 AI 工具中复用工作流。

支持模型：

- Claude
- Gemini
- Codex
- OpenCode

## 2. 入口与界面

进入侧栏 `Skills` 后，核心由三种视图组成：

1. `Recommended`：推荐技能（来自已配置的技能源）
2. `Repository`：仓库镜像技能（含远端与本地导入）
3. `Installed`：当前已安装到模型的技能

页面顶部可切换模型标签，按模型查看安装数量与可安装技能。

## 3. 推荐视图（Recommended）

适合从技能源直接安装技能。

常用操作：

1. 点击 `Sync Now` 拉取最新技能源索引。
2. 在卡片上点 `Install`。
3. 在弹窗中选择要安装到的模型（可多选）。
4. 确认后完成安装。

说明：

- 同一个 Skill 可安装到多个模型。
- 如果目标模型已安装，对应按钮会显示 `Installed`。

## 4. 仓库视图（Repository）

适合做跨来源统一管理（远端源 + 本地导入源）。

### 4.1 来源筛选

- `All`
- `Local`
- `Remote`

### 4.2 关键能力

- 查看每个 Skill 在 4 个模型上的安装覆盖情况（如 `Installed 2/4`）
- 对未安装模型进行补装
- 点开详情查看技能内容

## 5. 已安装视图（Installed）

用于查看当前模型已安装技能列表，支持：

- 查看详情
- 打开技能本地目录
- 卸载技能
- 检查更新 / 查看差异 / 应用更新（如果有远端新版本）

## 6. 本地导入（Import From Folder）

用于把本地目录中的技能批量纳入 OneSpace 管理。

流程：

1. 点击 `Import From Folder`。
2. 选择本地根目录。
3. 勾选要导入的技能。
4. 选择安装模型（可多选）。
5. 若存在冲突，逐项选择策略：
   - `Overwrite`：覆盖已安装版本
   - `Skip`：跳过冲突项
6. 确认导入。

导入完成后，技能会出现在 `Repository` 与 `Installed` 视图中。

## 7. 更新机制

OneSpace 支持对已安装技能进行更新管理：

1. 检查是否有更新
2. 预览差异（Diff）
3. 手动应用更新

建议在重要流程中先看 Diff 再更新，以避免行为变化影响现有习惯。

## 8. 技能源配置（Settings）

进入 `Settings -> Skills 源` 可管理同步策略：

- 添加/编辑/删除技能源（Git 仓库）
- 开关技能源启用状态
- 配置自动同步间隔
- 手动 `Sync Now`
- 导入/导出技能源配置 JSON

## 9. 与 CLI 的关系

安装后的技能会同步到对应模型 CLI 的技能目录中，常见路径：

- Claude：`~/.claude/skills`
- Gemini：`~/.gemini/skills`
- Codex：`~/.codex/skills`
- OpenCode：`~/.config/opencode/skills`

## 10. 常见问题

### Q1：`Recommended` 为空

请依次检查：

1. `Settings -> Skills 源` 中是否至少有一个已启用源
2. 是否执行过 `Sync Now`
3. 网络与仓库地址是否可访问

### Q2：导入时提示冲突

这是正常行为。请根据目标选择：

- 保留现有版本：选 `Skip`
- 用导入版本替换：选 `Overwrite`

### Q3：安装后在 CLI 中未生效

可按顺序处理：

1. 在 `Skills` 页面执行一次同步/重扫
2. 重启对应 CLI 会话
3. 确认安装模型与当前会话工具一致
