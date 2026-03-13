# OneSpace 清空操作指南

## 何时使用

使用此 skill 在以下情况：

1. **彻底重置环境** - 需要清空所有 AI 助手的配置重新开始
2. **解决配置冲突** - 配置文件损坏或冲突导致异常
3. **清理测试数据** - 开发测试后清理所有生成数据
4. **迁移或卸载** - 准备迁移配置或完全卸载

## 清空范围

此 skill 会删除以下目录：

| 目录 | 说明 |
|------|------|
| `~/.config/onespace` | OneSpace 主配置和数据目录 |
| `~/.codex/skills` | Codex 自定义技能 |
| `~/.codex/agents` | Codex 自定义代理 |
| `~/.claude/skills` | Claude 自定义技能 |
| `~/.claude/agents` | Claude 自定义代理 |
| `~/.gemini/skills` | Gemini 自定义技能 |
| `~/.gemini/agents` | Gemini 自定义代理 |
| `~/.config/opencode/skills` | OpenCode 自定义技能 |
| `~/.config/opencode/agents` | OpenCode 自定义代理 |

## 使用方法

### 1. 检查将要删除的内容

```bash
python3 skills/onespace-clean/scripts/clean.py --check
```

此命令会列出所有存在的目标目录及其大小，不会删除任何文件。

### 2. 执行清空

```bash
python3 skills/onespace-clean/scripts/clean.py --yes
```

或者不带 `--yes` 参数，会交互式确认：

```bash
python3 skills/onespace-clean/scripts/clean.py
```

### 3. 模拟运行

```bash
python3 skills/onespace-clean/scripts/clean.py --dry-run
```

显示将要删除的内容但不实际执行删除。

## 安全特性

1. **检查模式优先** - 默认只显示将要删除的内容
2. **明确确认** - 需要 `--yes` 标志或交互式确认
3. **错误继续** - 单个目录失败不影响其他目录
4. **详细报告** - 显示成功和失败的目录列表

## 恢复方法

清空后如需恢复：

1. **从备份恢复** - 如果有 Time Machine 或其他备份
2. **重新配置** - 各 AI 助手会在使用时重新创建默认配置
3. **OneSpace 配置** - 重新运行 OneSpace 设置向导

## 注意事项

⚠️ **清空前请确保：**

- 已备份重要配置文件
- 已保存自定义 skills 和 agents
- 了解清空后的影响

⚠️ **此操作不可逆** - 删除的文件无法通过此工具恢复

## 故障排除

### 权限错误

如果某些目录提示权限错误：

```bash
# 检查目录权限
ls -la ~/.config/onespace

# 修改权限后重试
chmod -R u+w ~/.config/onespace
```

### 目录被占用

确保没有 AI 助手进程正在运行：

```bash
# macOS 示例
ps aux | grep -E "(claude|codex|gemini)"
```

## 相关文件

- [SKILL.md](../SKILL.md) - Skill 定义和工作流
- [clean.py](clean.py) - 清空脚本实现
