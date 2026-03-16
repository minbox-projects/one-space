# OneSpace MCP Servers 文档

本文聚焦 OneSpace 当前已经落地的 MCP 管理能力，包括页面结构、配置字段、模型开关、模板、更新检查和导入导出。

## 1. MCP 在 OneSpace 中的定位

MCP Server 是给 AI CLI 扩能力的外部服务层。OneSpace 主要负责：

- 保存 MCP Server 配置
- 让 MCP 与具体 AI 环境建立关联
- 按工具单独启用/禁用
- 对部分 MCP 做版本更新检查
- 导入 / 导出 MCP 配置

## 2. 页面结构

入口：侧边栏 `MCP Servers`

当前页面分两种视图：

- `Server` 视图
  说明：以“Server”为中心管理配置
- `Model` 视图
  说明：以“模型/工具”为中心查看当前启用状态

页面还包含两个辅助面板：

- `Import / Export`
- `Backup Manager`

## 3. 当前支持的 Transport

每个 MCP Server 都有一个 `transport`，当前支持：

- `stdio`
- `http`
- `sse`

## 4. 创建 MCP Server

当前有两种创建路径：

1. `Add Server`
2. `Use Template`

### 4.1 手动创建时的主要字段

通用字段：

- `name`
- `description`
- `transport`
- `timeout`
- `trust`

当 `transport = stdio` 时：

- `command`
- `args`
- `cwd`
- `env`

当 `transport = http / sse` 时：

- `url`
- `headers`

### 4.2 字段含义建议

- `command`
  说明：本地启动命令，如 `npx`
- `args`
  说明：命令参数数组
- `cwd`
  说明：工作目录
- `env`
  说明：环境变量注入点，适合放 Token 占位
- `headers`
  说明：HTTP/SSE 服务常用请求头
- `trust`
  说明：表示你是否信任该 MCP 的调用行为

## 5. 模板创建

模板适合快速起步。当前代码内置了较多模板，包含但不限于：

- GitHub
- Filesystem
- PostgreSQL
- Context7
- Memory
- Sequential Thinking
- Slack
- Google Maps
- Brave Search
- GitLab
- Redis
- Google Drive
- Puppeteer
- Playwright
- Figma
- Linear
- Weather

模板一般会给出：

- 推荐的 `transport`
- 默认 `command / args / url`
- 需要你自行补齐的环境变量或请求头占位

推荐用法：

1. 用模板生成初稿
2. 把占位参数补齐
3. 保存后在 Server 视图展开检查

## 6. Server 视图

Server 视图更适合配置和维护。

你可以在这里：

- 查看所有 MCP Server
- 新增、编辑、删除 Server
- 展开单个 Server 查看详情
- 关联到一个或多个环境
- 为各模型切换启用状态
- 触发版本检查
- 应用更新

## 7. Model 视图

Model 视图更适合回答一个问题：

“某个工具当前到底启用了哪些 MCP？”

使用方式：

1. 进入 `Model` 视图
2. 选择目标工具
3. 查看该工具当前已启用的 Server
4. 直接关闭不需要的条目

注意：

- 这里的“关闭”是按模型关闭
- 不会删除 Server 配置本身

## 8. 环境关联（Link To Environments）

每个 MCP Server 可以关联到一个或多个环境 `Provider`。

用途主要有两个：

- 在多环境场景下保留 MCP 与环境的语义关系
- 给工作流和环境迁移提供更清晰的上下文

需要注意：

- “关联环境”本身不等于“已经为某个工具启用”
- 真正决定是否可用的，仍然是模型开关状态

## 9. 模型开关

当前开关粒度是按工具分别控制：

- `Claude`
- `Gemini`
- `Codex`
- `OpenCode`

这意味着：

- 同一个 MCP 可以只给 `Codex` 开
- 也可以四个工具一起开
- 关闭某个工具不会删除这个 MCP 的原始配置

## 10. 本地安装状态刷新

页面有一个“刷新本地安装状态”的动作，用于重新读取当前本机实际安装/启用状态。

适合的场景：

- 你手动改过 CLI 配置
- 你从别处同步了 MCP 文件
- UI 显示状态与你体感不一致

可以把它理解为：

- 重新做一次“本地状态对账”

## 11. MCP 更新检查

OneSpace 当前支持对一部分 MCP 做更新检查，但有边界。

### 11.1 主要适用对象

当前最适合更新检查的类型是：

- `stdio`
- `command = npx`
- `args` 中明确包含 npm 包名的 MCP

例如：

- `npx @modelcontextprotocol/server-github`
- `npx @modelcontextprotocol/server-filesystem`

### 11.2 更新状态

页面会显示类似状态：

- `up_to_date`
- `updatable`
- `floating_latest`
- `unsupported`
- `check_failed`

### 11.3 建议理解

- 如果 MCP 不是 `npx` 启动，更新检查可能无能为力
- 如果包名或版本声明无法解析，也会落到 `unsupported`

## 12. Import / Export

导出时可以：

- 选择部分 Server
- 写入备注
- 输出成 JSON

导入时可以：

- 选择 JSON 文件
- 把其中的 MCP 读入当前库

适合场景：

- 多机迁移
- 团队共享基础 MCP 清单
- 做版本归档

## 13. Backup Manager

MCP 页面里可以打开 `Backup Manager`，当前更适合把它理解为：

- 备份历史查看器
- 恢复入口
- 删除与清理入口

当前全局视图下：

- 可以查看、恢复、删除、清理旧备份
- “手动创建备份”按钮依赖具体工具上下文，通常不是这一页的主要工作流

## 14. 推荐操作顺序

一个相对稳妥的 MCP 使用流程是：

1. 优先用模板创建
2. 补齐 Token / URL / headers / env
3. 保存并在 Server 视图核对内容
4. 关联需要的环境
5. 先只给一个工具开启模型开关
6. 启动一次真实会话验证
7. 再逐步复制到其它工具

## 15. 推荐实践

- 优先把敏感信息放到 `env`，避免硬编码到命令行参数
- 不要在不理解的情况下开启 `trust`
- 对 `stdio + npx` 型 MCP，建议显式写出包名和版本，便于后续更新检查
- 调整配置后，如果会话已经在运行，通常需要重开会话让新配置生效

## 16. 常见问题

### Q1：刚创建的 Server 为什么在 Model 视图里看不到

因为“保存配置”和“为某个工具启用”是两回事。

请检查：

1. 是否已经保存
2. 是否给目标工具打开了模型开关

### Q2：删除和按模型关闭有什么区别

- 删除：移除 MCP Server 配置本身
- 按模型关闭：只关闭当前工具对它的使用

### Q3：修改以后没有生效

建议依次检查：

1. 配置是否保存成功
2. 目标工具的模型开关是否开启
3. 是否需要刷新本地安装状态
4. 目标 CLI 会话是否已经重启

### Q4：为什么某些 MCP 没法检查更新

通常是因为它不属于 `npx` 包型 `stdio` MCP，或包名/version 无法从命令参数中稳定解析。
