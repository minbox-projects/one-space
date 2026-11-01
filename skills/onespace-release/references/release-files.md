# OneSpace 发布文件

## 版本来源

以下四个文件的版本号必须保持一致：

1. `package.json` -> root `version`
2. `src-tauri/tauri.conf.json` -> root `version`
3. `src-tauri/Cargo.toml` -> `[package]` section `version`
4. `src-tauri/Cargo.lock` -> `[[package]] name = "OneSpace"` block `version`

## 产物命名校验（重点）

macOS DMG 文件名会带版本号，默认形态：

- `OneSpace_<version>_aarch64.dmg`
- `OneSpace_<version>_x64.dmg`

如果打包产物仍是旧版本号，优先检查上述四个文件是否都已更新并提交。

## Tag 规范

使用带 `v` 前缀的附注 git 标签：

- `v0.1.5`
- `v1.0.0`
- `v0.1.58-beta.1`
- `v0.1.58-beta.2`

Tag 消息格式：

- `release: v<version>`

Beta 标签是合法的 semver prerelease，推送后在 GitHub 上以 prerelease 发布。

## Beta 版本线

`beta <base>` 只接受显式的 `X.Y.Z` 基版本，并同时更新上述四个文件：

- 当前稳定版低于基版本时写入 `<base>-beta.1`。
- 当前版本已是同一基版本的 Beta 时递增为 `<base>-beta.N+1`。
- 更高的基版本从 `<base>-beta.1` 重新开始。
- 非法或非 `X.Y.Z` 基版本、等于或低于当前稳定版的基版本、低于当前 Beta 基版本的基版本，以及四个文件不一致时，命令失败且不写任何文件。
- `--dry-run` 只报告下一个版本，不修改文件。

发布范围由发布助手解析：`previous-tag` 对稳定发布返回上一个稳定标签（跳过 Beta 标签），对 Beta 发布返回上一个标签；`is-prerelease` 对 Beta 标签返回 `true`、对稳定标签返回 `false`。

## 发布前检查

1. 工作区必须干净（`git status --short` 预期为空）
2. `python3 skills/onespace-release/scripts/release_tool.py validate` 必须通过
3. 构建必须成功（按需执行 `npm run build` 和 `npm run tauri build`）
4. Tag 必须唯一（`git tag --list v<version>` 结果为空）
5. `python3 skills/onespace-release/scripts/release_tool.py tag <version> --dry-run` 预检通过
6. macOS 签名 secrets 已配置且非空：`APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_SIGNING_IDENTITY`（缺失会在任何构建之前失败，见 [release-signing.md](release-signing.md)）
7. macOS 签名产物已通过 `bash skills/onespace-release/scripts/verify_release_artifact.sh <path-to-OneSpace.app>` 验证
8. Beta 发布先执行 `python3 skills/onespace-release/scripts/release_tool.py beta <base> --dry-run` 确认目标版本，再提交四个发布文件并用 `v<base>-beta.N` 打标
