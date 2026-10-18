# Agent Note: macOS Release Signing Uses a Dedicated Self-Signed Certificate

Status: implemented

[English](2026-10-18-macos-release-signing.md) | 中文

## Problem

每个以 ad-hoc 方式签名的 macOS 构建，其指定要求（designated requirement）都锚定到该构建的 cdhash。由于 cdhash 随代码内容变化，macOS 会把每次发布视为不同的应用，用户授予的钥匙串、蓝牙与辅助功能权限便在每次更新时丢失，提示反复出现。发布工作流也没有配置任何 Apple 签名材料，因此签名缺失或配置错误会静默产出 ad-hoc 产物。

## Decision

仓库用一张专用的自签名代码签名证书 `OneSpace Release Signing` 签署每个 macOS 发布产物，该证书由 `skills/onespace-release/scripts/create_signing_certificate.sh` 一次性创建。证书把指定要求锚定到证书哈希而非 cdhash，因此指定要求文本在不同构建间保持一致，后续的签名更新得以保留用户已有的授权。

引导脚本生成一张 RSA 2048 自签名证书（`codeSigning` 扩展密钥用法、有效期十年），把它导入供 `codesign` 使用，并写出 `onespace-release-signing.p12`、`certificate-base64.txt` 与 `password.txt`（权限 `0600`），分别用于 GitHub secrets `APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD` 与 `APPLE_SIGNING_IDENTITY`。证书存在时它拒绝再次运行，导出的材料必须备份到仓库之外。

发布工作流在任一签名 secret 为空时、于任何构建之前让 macOS 任务失败，把证书导入当次运行专用的钥匙串并断言指定身份存在，并在 `tauri-action` 之后用 `skills/onespace-release/scripts/verify_release_artifact.sh` 验证产物：要求 `codesign --verify --strict` 通过、identifier 为 `com.onespace.desktop`、签名非 ad-hoc，且指定要求锚定到证书（接受 root 或 leaf 锚定形式，拒绝 cdhash）。

## Alternatives considered

- 使用 Apple Developer ID 签名并公证：未采纳，因为它需要付费的 Apple Developer 账户与公证基础设施；免费的自签名路径对本项目非 App Store 的分发方式提供了同样的跨构建权限稳定性。
- 维持 ad-hoc 现状：未采纳，因为锚定到 cdhash 的指定要求无法跨构建保持稳定，用户会在每次更新时持续丢失并重新授予权限。
- 每次构建或顺手时重新生成证书：未采纳，因为证书是用户授权所绑定的锚点；重新生成会使 macOS 把应用视为新的签署者，从而作废全部既有授权。

## Consequences

- 锚定到证书的指定要求跨构建稳定，因此签名更新无需重新授权；只有首次签名发布会让每位现有用户一次性确认钥匙串、蓝牙与辅助功能，该一次性提示应写入 release body。
- 丢失或重新生成证书等同于要求每位用户重新授予全部三项权限，因此 p12 及其口令必须备份到仓库之外，且发布之后绝不再以 `--force` 运行引导脚本。
- 构建前的守卫与构建后的验证步骤充当触发线：secret 缺失会在构建前失败，ad-hoc 或错误签名的产物会在验证时失败。
- 回滚会移除守卫、导入与验证三个步骤以及 `APPLE_SIGNING_IDENTITY` 环境项；证书与 secrets 可以保留，代价是反复出现的权限提示重新出现。
