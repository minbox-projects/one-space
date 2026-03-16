# OneSpace Skills 与 Subagents 文档

本文覆盖 `Skills` 和 `Subagents` 两个页面，因为它们在 OneSpace 里的生命周期非常接近：

- 都有 `Recommended / Repository / Installed` 三视图
- 都支持按模型安装
- 都支持 `Global / Project` 两种安装范围
- 都支持源同步、本地导入、更新、打开本地目录

如果你只关心 `Skills`，可以重点看前半部分。如果你也在用 `Subagents`，请重点看第 10 节。

## 1. 两者的角色区别

### Skills

- 面向可复用任务能力包
- 通常包含 `SKILL.md` 以及参考文件、脚本、资源目录
- 更适合“让模型学会一件事”

### Subagents

- 面向自治代理预设
- 更适合“给模型一个固定角色或专门职责”
- 在 Claude/Codex/Gemini/OpenCode 的代理工作流里尤其有用

## 2. 支持的模型

两者都支持：

- `Claude`
- `Gemini`
- `Codex`
- `OpenCode`

## 3. 三种视图

### 3.1 Recommended

来源：

- 已配置源仓库同步下来的推荐项

适合：

- 第一次安装
- 看有哪些官方/团队推荐能力
- 按模型快速补装

### 3.2 Repository

来源：

- 远端同步下来的镜像
- 本地文件夹导入的仓库项

适合：

- 把所有来源放到一处统一管理
- 做更新、重装、差异预览

### 3.3 Installed

来源：

- 当前已安装到指定模型的条目

适合：

- 看当前环境到底装了什么
- 打开本地目录
- 卸载
- 检查更新并应用

## 4. 安装范围：Global 与 Project

这是当前实现里非常重要的一层。

### 4.1 Global

含义：

- 面向该工具的全局安装
- 更适合通用能力、个人长期使用能力

### 4.2 Project

含义：

- 把能力安装到某个项目目录
- 更适合仓库私有能力、团队协作能力、需要与项目一起版本化的能力

页面里会记住你最近使用的 `project root`，便于反复安装同一仓库相关能力。

## 5. Skills 的实际安装目录

### 5.1 Global Scope

OneSpace 内部会先写入自己的模型目录，再镜像到各 CLI 常用目录。

最终常见镜像位置：

- Claude：`~/.claude/skills`
- Gemini：`~/.gemini/skills`
- Codex：`~/.codex/skills`
- OpenCode：`~/.config/opencode/skills`

### 5.2 Project Scope

项目安装目录：

- Claude：`<project>/.claude/skills`
- Codex：`<project>/.agents/skills`
- Codex 兼容目录：`<project>/.codex/skills`
- Gemini：`<project>/.gemini/skills`
- OpenCode：`<project>/.opencode/skills`

## 6. Skills 页面怎么用

### 6.1 Recommended 视图

常用操作：

1. 点击 `Sync Now`
2. 选择某个 Skill
3. 选择安装模型，可以多选
4. 选择安装范围 `Global / Project`
5. 如果是 `Project`，选择项目目录
6. 安装

当前行为说明：

- 同一个 Skill 可以装到多个模型
- 同一个 Skill 也可以同时存在 `Global` 与 `Project` 两个范围

### 6.2 Repository 视图

Repository 更适合做管理动作。

当前可见能力包括：

- 按 `All / Local / Remote` 过滤来源
- 搜索名称、描述、ID、路径
- 查看每个条目的模型覆盖情况
- 从仓库直接为某个模型启用安装
- 打开详情
- 执行“从本地重新加载”预览与应用

### 6.3 Installed 视图

Installed 适合确认“已经生效的东西”。

当前支持：

- 按当前模型查看已安装项
- 查看详情
- 打开本地目录
- 卸载
- 检查更新
- 预览差异
- 应用更新

## 7. 本地导入

`Import From Folder` 可以把本地目录纳入 OneSpace 管理。

适合场景：

- 团队内部私有 Skills
- 本地开发中的试验性 Skills
- 从其他仓库搬迁已有 Skills

导入后条目会进入：

- `Repository`
- 对应模型下的 `Installed`（如果导入时选择了安装）

## 8. 更新与差异预览

OneSpace 对 Skills 的更新流程不是“直接覆盖”，而是尽量走可见化流程：

1. 检查是否有更新
2. 预览 Markdown 差异
3. 再决定是否应用

对 Repository 条目还支持：

- 重新从本地源加载
- 预览变更文件列表
- 查看文本差异
- 应用 reload

这意味着：

- 你可以在应用前看到变化
- 更适合对关键 Skill 做谨慎升级

## 9. 命名与校验规则

Skills 有几条当前代码层面的硬规则：

- 每个目录名最终取决于 `SKILL.md` frontmatter 里的 `name`
- `name` 只能包含：
  - 小写字母
  - 数字
  - `-`
  - `_`
- 同一模型、同一安装范围下，目录名冲突会直接拒绝安装

这能避免：

- 两个不同 Skills 覆盖同一目录
- 旧目录规则与新规则混用时相互踩踏

## 10. Subagents 页面怎么用

`Subagents` 与 `Skills` 大体相同，但有两点需要单独说明。

### 10.1 安装目录

Global 镜像目录：

- Claude：`~/.claude/agents`
- Codex：`~/.codex/agents`
- Gemini：`~/.gemini/agents`
- OpenCode：`~/.config/opencode/agents`

Project 目录：

- Claude：`<project>/.claude/agents`
- Codex：`<project>/.codex/agents`
- Gemini：`<project>/.gemini/agents`
- OpenCode：`<project>/.opencode/agents`

### 10.2 源诊断

`Subagents` 额外提供 `source diagnose`，用于帮助你判断某个源为什么扫描结果不符合预期。

当前会报告的典型问题包括：

- 缺少 frontmatter
- frontmatter 缺少 `name`
- `name` 不合法
- 文件读取失败

这在排查“为什么某个子代理没出现在 Recommended/Repository 里”时很有用。

## 11. 与会话和工作流的关系

OneSpace 并不是只把 Skills/Subagents 安装到磁盘就结束了。

在实际启动会话时：

- `AI Sessions` 恢复会话前会做一次 Skills/Subagents 预检与 reconcile
- `Workflow Presets` 也会在依赖检查里考虑 Skills

因此推荐理解为：

- 页面负责管理内容
- 会话和工作流负责在启动时让内容进入正确上下文

## 12. Source 设置页

在 `Settings -> Skills 源` 和 `Settings -> Subagents 源` 中，当前支持：

- 添加源
- 删除源
- 启用/禁用源
- 配置默认模型
- 设置自动同步开关
- 设置自动同步间隔
- 设置“新内容徽标”保留小时数
- 导入 / 导出 JSON
- 手动 `Sync Now`

### 12.1 源校验规则

源配置会校验：

- `id` 不能为空
- `id` 只能包含字母、数字、点、下划线、短横线
- `repo_url` 必须是合法 Git 地址
- `base_dir` 必须以 `/` 开头，且不能包含 `..`
- 至少选择一个默认模型

## 13. Repository 中的来源类型

当前 Repository 视图会把来源区分为：

- `Recommended Source`
- `Local Import`
- `Mirror`

可以把它们理解为：

- Recommended Source：远端源同步结果
- Local Import：你手工导入的本地目录
- Mirror：OneSpace 自己维护的镜像仓库结果

## 14. 常见问题

### Q1：Recommended 为空

按顺序检查：

1. `Settings -> Skills 源` 或 `Subagents 源` 是否至少配置并启用了一个源
2. 是否执行过 `Sync Now`
3. 网络与仓库地址是否可访问

### Q2：为什么安装时要我选 Global 或 Project

因为当前实现明确支持双范围安装，而且二者用途不同：

- `Global` 更适合个人常用能力
- `Project` 更适合仓库隔离能力

### Q3：安装后 CLI 里没看到效果

建议依次检查：

1. 安装范围是否正确
2. 当前会话对应的工具是否正确
3. 是否需要重新启动会话
4. 是否从 `AI Sessions`/工作流重新发起一次启动，让 reconcile 流程跑一遍

### Q4：导入时报冲突

常见冲突类型：

- 同 ID 冲突
- 同目录名冲突
- Project / Global 同时存在但目标目录不一致

### Q5：Subagents 里为什么有些源条目扫不出来

优先使用源诊断功能，通常会直接告诉你是：

- frontmatter 缺失
- `name` 缺失
- `name` 不合法
- 文件本身读取失败
