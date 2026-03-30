---
name: git-commit
description: 根据规范（Gitmoji + Conventional Commits）生成 Git 提交信息
---

## 📦 Git Commit

**Git 提交专家 | Gitmoji · Conventional Commits · 规范提交**

---

## 角色定义

1. 生成符合 Gitmoji + Conventional Commits 的提交信息
2. 识别合适的提交粒度并提示拆分
3. 在明确授权时，对已选定变更执行提交
4. 保持提交语义清晰、范围可追踪

默认只负责生成提交信息；只有在用户明确要求执行提交时，才对已选定的变更进行提交。

## 核心原则

本项目采用 [Gitmoji](https://gitmoji.dev/) 风格规范，结合 Conventional Commits，旨在通过 Emoji 直观地展示提交类型。

## 提交格式

```text
<emoji> <subject>

<body>

<footer>
```

## 提交规则

- 不默认提交整个工作区
- 不把无关变更混入同一个提交
- 优先根据变更目的拆分成独立 commit
- 提交信息遵循 Gitmoji + Conventional Commits 规范

## 规范详情

### 1. Emoji 类型对照表（必须）

提交信息**必须**以对应的 Emoji 开头，用于标识提交类别：

| Emoji | 代码 | 说明 | 对应原 Type |
| :---: | :--- | :--- | :--- |
| ✨ | `:sparkles:` | 新功能 | `feat` |
| 🐛 | `:bug:` | 修补 bug | `fix` |
| 📝 | `:memo:` | 文档更新 | `docs` |
| 🎨 | `:art:` | 代码格式/样式调整 | `style` |
| ♻️ | `:recycle:` | 代码重构 | `refactor` |
| ⚡️ | `:zap:` | 性能优化 | `perf` |
| ✅ | `:white_check_mark:` | 增加/修复测试 | `test` |
| 📦️ | `:package:` | 构建/依赖更新 | `build` |
| 👷 | `:construction_worker:` | CI 配置/脚本修改 | `ci` |
| 🔧 | `:wrench:` | 杂项/配置修改 | `chore` |
| ⏪️ | `:rewind:` | 代码回滚 | `revert` |

### 2. Subject（必须）

commit 目的的简短描述，不超过 50 个字符。

- **必须**以 Emoji 开头，Emoji 后需加一个空格
- **必须**使用中文描述
- 以动词开头
- 结尾不加句号（.）

### 3. Body（可选）

对本次 commit 的详细描述，可以分成多行。

- 说明代码变动的动机
- 与之前的行为对比

### 4. Footer（可选）

- **不兼容变动**: 以 `BREAKING CHANGE:` 开头，后面是对变动的描述、以及变动理由和迁移注释。
- **关闭 Issue**: 例如 `Closes #123`, `Fixes #123`.
- **不允许包含提交工具名称**: 例如 `Co-Authored-By`.

## 适用场景

- 生成提交信息
- 拆分变更范围并建议提交粒度
- 对已选定变更执行提交

## 输入要求

### 必须输入

- 变更摘要、diff、或待提交文件列表中的至少一项

### 可选输入

- 提交粒度偏好
- 是否只生成 commit message
- 是否对已选定变更执行真实提交

## 输出规范

### 输出文件

- 无强制文件输出；默认输出提交信息文本

## 执行规则

- 先判断变更是否需要拆分，再生成提交信息。
- 默认只输出 commit message，不执行 `git commit`。
- 如用户明确要求执行提交，只对已明确选定或已暂存的变更进行提交。
- 不替用户决定提交全部工作区内容。

## 质量检查

- [ ] 提交类型与变更目的匹配
- [ ] subject 简洁明确（不超过 50 字符）
- [ ] 如存在 body，说明动机和关键变化
- [ ] 未混入未确认范围的变更
- [ ] Emoji 使用正确

## 示例

```text
:sparkles: 添加 JWT 令牌刷新机制

当访问令牌过期时，自动使用刷新令牌获取新的访问令牌。
支持并发请求下的队列处理。

Closes #45
```

```text
:bug: 修复连接断开重连时的空指针异常
```

```text
:memo: 更新部署文档
```

```text
:recycle: 重构用户认证模块

将认证逻辑从控制器中提取到独立服务层。
提高代码可测试性和复用性。

BREAKING CHANGE: 认证 API 请求路径从 /auth 改为 /api/v1/auth
```

```text
:white_check_mark: 添加用户注册流程的集成测试

覆盖正常注册、重复邮箱、弱密码等场景。
```

## 🔄 下一步流程

`git-commit` 是本轮研发流程的收尾阶段。

1. 若用户只需要提交信息，输出可直接使用的 commit message
2. 若用户明确授权真实提交，对已选定变更执行提交
3. 提交完成后，本轮需求流转结束
