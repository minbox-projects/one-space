# OneSpace 发布文件

## 版本来源

以下三个文件的版本号必须保持一致：

1. `package.json` -> root `version`
2. `src-tauri/tauri.conf.json` -> root `version`
3. `src-tauri/Cargo.toml` -> `[package]` section `version`

## Tag 规范

使用带 `v` 前缀的附注 git 标签：

- `v0.1.5`
- `v1.0.0`

Tag 消息格式：

- `release: v<version>`

## 发布前检查

1. 工作区必须干净（`git status --short` 预期为空）
2. `python3 skills/onespace-release/scripts/release_tool.py validate` 必须通过
3. 构建必须成功（按需执行 `npm run build` 和 `npm run tauri build`）
4. Tag 必须唯一（`git tag --list v<version>` 结果为空）
