# OneSpace macOS 发布签名

本文档说明 OneSpace macOS 发布产物的签名流程：权限提示为什么会反复出现、仓库为什么使用专用自签名证书、一次性引导步骤、GitHub secret 配置、发布前检查、产物验证命令、绝不重新生成的规则、首次签名发布的一次性重新授权提示，以及回滚路径。

## 为什么权限提示会反复出现

未签名（ad-hoc 签名）的 macOS 应用的指定要求（designated requirement）以构建产物的 cdhash 为锚点。cdhash 由代码内容决定，因此每次构建都会变化；macOS 会把每个新构建视为不同的应用，用户此前授予的钥匙串、蓝牙与辅助功能（Accessibility）权限随即失效，每次更新后都必须重新授权。

## 为什么使用仓库专用的自签名证书

仓库使用一张专用的自签名代码签名证书 `OneSpace Release Signing`。用证书签名后，应用的指定要求锚定到证书哈希，而不是构建产物的 cdhash：

```text
identifier "com.onespace.desktop" and certificate root = H"<certificate hash>"
```

只要证书不变，不同内容、不同版本的构建都会得到相同的指定要求，macOS 因此把后续更新识别为同一个签署者，用户此前授予的三项权限得以保留。在当前的 macOS 上，自签名证书实际产生的锚定形式为 `certificate leaf = H"..."`；验证脚本同时接受 root 与 leaf 两种锚定形式，并且始终拒绝 cdhash 形式。

## 一次性引导

在 Mac 上、于仓库根目录运行一次：

```bash
bash skills/onespace-release/scripts/create_signing_certificate.sh
```

脚本默认使用登录钥匙串与 `~/onespace-release-signing`。可用参数：

- `--name NAME`：证书 Common Name，默认 `OneSpace Release Signing`。
- `--keychain PATH`：目标钥匙串，默认登录钥匙串。
- `--output-dir DIR`：输出目录，默认 `~/onespace-release-signing`。
- `--force`：替换同名证书；这会作废每一位现有用户的权限授权。

脚本创建一张 RSA 2048 自签名证书（CN `OneSpace Release Signing`、`codeSigning` 扩展密钥用法、有效期十年），把它导入目标钥匙串供 `codesign` 使用，并写出以下文件（权限均为 `0600`）：

- `onespace-release-signing.p12`：PKCS#12 导出。
- `certificate-base64.txt`：p12 的 base64 编码。
- `password.txt`：p12 口令。

必须把输出目录（p12 与口令）备份到仓库之外。证书一旦丢失或重新生成，等同于要求每一位用户重新授权。

## GitHub secret 配置

在 GitHub 仓库 secrets 中配置：

- `APPLE_CERTIFICATE`：`certificate-base64.txt` 的内容（p12 的 base64）。
- `APPLE_CERTIFICATE_PASSWORD`：`password.txt` 的内容。
- `APPLE_SIGNING_IDENTITY`：`OneSpace Release Signing`。

## 发布前检查

1. 上述三个 secret 都已配置且非空；空的 secret 值按缺失处理。
2. 发布工作流在 macOS 任务中、任何构建之前运行签名守卫（check macOS signing secrets）：只要有任一 secret 为空，任务即失败并指名缺失的 secret，不产生任何构建或上传；Windows 支路不受影响。
3. 导入步骤（import macOS signing certificate）把 p12 解码到当次运行专用的钥匙串、导入供 `/usr/bin/codesign` 使用、设置 partition list，并把该钥匙串前插到用户搜索列表，然后断言 `APPLE_SIGNING_IDENTITY` 指定的身份确实存在；身份缺失即失败。
4. `tauri-action` 之后运行产物验证步骤（verify signed macOS release artifact）；该步骤是防止未签名或 ad-hoc 产物被当作可信发布物的触发线。

## 产物验证

对已构建（或已下载）的应用运行：

```bash
bash skills/onespace-release/scripts/verify_release_artifact.sh <path-to-OneSpace.app>
```

脚本要求目标路径存在，并检查：

- `codesign --verify --strict` 通过。
- identifier 为 `com.onespace.desktop`。
- 不是 ad-hoc 签名（`Signature=adhoc`）。
- 指定要求锚定到证书，而不是 cdhash（接受 `certificate root = H"..."` 或 `certificate leaf = H"..."`，始终拒绝 cdhash）。

通过时打印解析出的指定要求行与 `Authority` 证据。

## 绝不重新生成

一旦发布过签名版本，就绝不再以 `--force` 重新运行引导脚本。重新生成会更换证书，使 macOS 把应用视为新的签署者，从而作废每一位用户已有的三项授权，等同于要求每一位用户重新授权。证书丢失与重新生成的后果相同。

## 首次签名发布的一次性重新授权提示

从上一个 ad-hoc 版本迁移到第一个签名版本时，每一位现有用户需要一次性重新确认钥匙串、蓝牙与辅助功能三项授权。请在首次签名发布的 release body 中加入该一次性提示。此后的签名更新无需任何再次确认。

## 回滚

要恢复到此前的 ad-hoc 发布路径，从工作流中移除签名守卫、导入与产物验证三个步骤，并移除 `APPLE_SIGNING_IDENTITY` 环境项。生成的证书与 GitHub secrets 可以保留，不影响回滚。回滚的代价是重新暴露于每次更新反复出现的权限提示。
