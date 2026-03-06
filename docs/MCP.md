# OneSpace MCP Servers 使用文档

本文介绍如何在 OneSpace 中配置和管理 MCP Server，并按模型控制启用状态。

## 1. 功能定位

MCP Servers 用于扩展 AI 助手能力（如访问 GitHub、文件系统、数据库、远程服务等）。

你可以在 OneSpace 中：

- 统一维护 MCP Server 配置
- 关联到环境（Provider）
- 针对不同模型单独启用/禁用
- 导入导出 MCP 配置

## 2. 页面结构

入口：侧栏 `MCP Servers`

页面主要分为两种视图：

1. `仓库`（按 Server 管理）
2. `已安装`（按模型查看已启用 Server）

## 3. 新建 MCP Server

创建方式有两种：

1. `Add Server` 手动创建
2. `Use Template` 模板创建（推荐新手）

### 3.1 传输类型（Transport）

支持三种：

- `stdio`：本地命令方式
- `http`：HTTP 服务
- `sse`：SSE 服务

### 3.2 必填字段

- `name`
- `transport`
- 当 `stdio` 时：`command` 必填
- 当 `http/sse` 时：URL 必填

### 3.3 可选字段

- `args`（stdio 参数）
- `cwd`（工作目录）
- `env`（环境变量）
- `headers`（请求头，http/sse 常用）
- `timeout`
- `trust`（是否自动信任调用）

## 4. 模板创建（Use Template）

模板可快速生成常见 MCP 配置骨架，通常会包含：

- 推荐 transport
- 预置 command/args/url
- 需要你填写的变量占位符（如 Token、API Key）

建议流程：

1. 选模板
2. 补齐必要参数
3. 保存后在“仓库”视图展开校验

## 5. 关联环境（Link To Environments）

在 Server 详情中可把 MCP Server 关联到一个或多个环境（Provider）。

用途：

- 让 MCP 配置与具体 AI 环境协同管理
- 在多环境场景下更清晰地分组与迁移

## 6. 模型开关（MCP Model Switches）

每个 Server 都可对以下模型独立开关：

- Claude
- Gemini
- Codex
- OpenCode

这意味着：

- 同一个 MCP Server 可以只在某个模型启用
- 禁用后不会删除 Server，仅停止该模型使用

## 7. 按模型视角管理（已安装）

切换到 `已安装` 视角后：

1. 先选择模型
2. 查看该模型已启用的 MCP 列表
3. 可直接对某个 MCP 执行“卸载”（仅对当前模型关闭）

如果要重新启用，回到 `仓库` 视角打开对应模型开关即可。

## 8. 导入导出

通过 `Import/Export` 可以：

- 导出当前 MCP 配置（支持选择 Server）
- 导入已有 MCP 配置文件

适合场景：

- 多设备迁移
- 团队共享基础 MCP 清单
- 备份恢复

## 9. 推荐实践

1. 优先用模板起步，再按需微调。
2. 先在单一模型启用验证，再逐步扩展到其它模型。
3. `trust` 仅在你明确理解风险时启用。
4. 对敏感参数（Token/密钥）尽量通过环境变量注入，避免硬编码。

## 10. 常见问题

### Q1：为什么在“已安装”视图看不到刚创建的 Server？

因为创建只是保存配置，未必已对当前模型开启。请到 `仓库` 视图打开该模型开关。

### Q2：删除与卸载有什么区别？

- `Delete`：删除 MCP Server 配置本身
- `卸载`（模型视角）：仅对当前模型关闭，不删除配置

### Q3：修改配置后未生效

建议检查：

1. 是否保存成功
2. 当前模型开关是否开启
3. 目标 CLI 会话是否需要重启后加载新配置
