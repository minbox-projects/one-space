---
name: onespace-release
description: 自动化 OneSpace 桌面端发布流程，包括版本号提升、发布配置一致性校验、以及 git tag 创建和推送。用于处理“发布版本”“提升版本号”“创建 tag”或同步 package.json、src-tauri/tauri.conf.json、src-tauri/Cargo.toml 三处版本配置的请求。
---

# OneSpace 发布

使用确定性命令完成 OneSpace 的发布准备和打标流程。确保所有版本字段一致，并使用附注标签（annotated tag）。

## 工作流

1. 查看当前版本号。
2. 校验发布配置一致性。
3. 将所有发布版本提升到目标 semver。
4. 再次校验并执行构建检查。
5. 创建附注发布标签，必要时推送。

## 命令

在仓库根目录执行以下命令。

```bash
python3 skills/onespace-release/scripts/release_tool.py show
python3 skills/onespace-release/scripts/release_tool.py validate
python3 skills/onespace-release/scripts/release_tool.py bump 0.1.5
python3 skills/onespace-release/scripts/release_tool.py tag 0.1.5
python3 skills/onespace-release/scripts/release_tool.py tag 0.1.5 --push
```

在风险操作前先做 dry run：

```bash
python3 skills/onespace-release/scripts/release_tool.py bump 0.1.5 --dry-run
python3 skills/onespace-release/scripts/release_tool.py tag 0.1.5 --dry-run
```

## 安全规则

1. 版本不一致或不符合 semver 时必须失败。
2. 校验未通过前禁止创建 tag。
3. 使用附注标签（`git tag -a`），消息格式为 `release: v<version>`。
4. 默认使用 `v` 前缀，除非仓库策略明确要求变更。

## 文件范围

编辑发布相关字段前，先阅读 [references/release-files.md](references/release-files.md)。

## 资源

### scripts/

- `release_tool.py`：读取版本、执行一致性校验、提升版本、创建/推送附注标签。

### references/

- `release-files.md`：发布文件清单、tag 规范、发布前检查项。
