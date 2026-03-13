---
name: onespace-clean
description: 清空 OneSpace 所有数据和配置，包括各 AI 助手的 skills 和 agents 目录。用于彻底重置环境或解决配置冲突问题。
---

# OneSpace 清空数据

用于彻底清空 OneSpace 相关的所有配置和数据目录。执行前会列出待删除的目录并请求确认。

## 工作流

1. 检查待删除的目录是否存在
2. 列出所有待删除目录及其大小
3. 请求用户确认
4. 执行清空操作
5. 报告清空结果

## 命令

在仓库根目录执行以下命令：

```bash
python3 skills/onespace-clean/scripts/clean.py --check
python3 skills/onespace-clean/scripts/clean.py --yes
```

## 安全规则

1. 默认模式仅检查并列出待删除内容，不执行删除
2. 必须使用 `--yes` 标志才执行实际删除
3. 删除前必须显示所有目标目录
4. 遇到权限错误时继续处理其他目录，不中断整个流程
5. 删除后报告成功和失败的目录

## 清空范围

- `~/.config/onespace` - OneSpace 主配置目录
- `~/.codex/skills` - Codex skills
- `~/.codex/agents` - Codex agents
- `~/.claude/skills` - Claude skills
- `~/.claude/agents` - Claude agents
- `~/.gemini/skills` - Gemini skills
- `~/.gemini/agents` - Gemini agents
- `~/.config/opencode/skills` - OpenCode skills
- `~/.config/opencode/agents` - OpenCode agents

## 资源

### scripts/

- `clean.py`：执行检查和清空操作的主脚本

### references/

- `cleanup-guide.md`：清空操作说明和恢复指南
