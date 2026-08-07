# 本地 RootKey 存储迁移规格

## 规格元数据

- plan-id: `local-root-key-storage`
- status: `approved`
- source_context_id: `ctx-local-root-key-storage-20260807`
- source_context_digest: `11cafebe5fa3ebf151ac454d80654a2b51a1a25989fc9dacb81ca76992b60d27`

## 问题陈述

AI 路由网关当前将 RootKey 存于 macOS Keychain。该密钥用于解密 SQLite 中既有的 AI 路由凭据；因此不能以重新生成的密钥替代。日常启动和凭据操作访问 Keychain 会触发授权提示，影响首次安装后的正常使用。

现状事实：RootKey 是固定长度为 32 字节的随机值；SQLite 中保存 AES-256-GCM 密文；旧 Keychain 项的服务名为 `com.onespace.ai-routing-gateway`，账户名为 `root-data-key-v1`。仅 macOS 存在旧 Keychain 数据及迁移读取路径。

## 目标与成功标准

- 将 RootKey 的日常存储改为 `~/.config/onespace` 下的应用私有本地密钥文件，具体文件名由实施确定。
- 新安装首次启动生成并使用本地 RootKey，不访问 Keychain。
- 升级用户可一次读取旧 Keychain RootKey，原子保存到本地并验证既有密文可解密后完成切换。
- 后续启动和账户凭据、网关 API Key 命令仅使用文件型 `RootKeyStore`。
- 目录权限为 `0700`、密钥文件权限为 `0600`，写入必须为原子操作。
- 保持 SQLite schema、AES-256-GCM 密文格式和 AAD 不变，不重新加密业务凭据。

## 用户与用户故事

- 首次安装 OneSpace 的 macOS 用户：首次启动自动创建本地 RootKey，过程中不出现钥匙串授权提示。
- 已在 SQLite 保存 AI 路由凭据的升级用户：首次升级启动可在必要时接受一次 Keychain 授权，迁移后原有凭据继续可用，后续不再访问 Keychain。
- AI 路由网关启动与凭据命令消费者：通过统一注入的 `RootKeyStore` 取得 RootKey，不直接依赖 `MacOsKeychainStore`。

## 功能需求

1. 新增文件型 `RootKeyStore`，负责本地 RootKey 的读取、首次生成、持久化及并发初始化协调。
2. RootKey 必须恰为 32 字节。读取到的本地表示无效、长度错误或无法安全读取时不得作为有效密钥使用。
3. 配置目录不存在时创建为 `0700`；密钥文件创建或修正为 `0600`；临时文件与最终替换流程不得留下可见的部分密钥文件。
4. 在不存在已有 SQLite 密文的全新配置中，本地 RootKey 缺失时生成新的 32 字节随机密钥并原子保存。
5. 在 SQLite 已存在受保护密文时，本地 RootKey 缺失或无效时禁止生成替代密钥。macOS 上须进入旧 Keychain 迁移流程；其他平台或无可迁移密钥时返回锁定状态。
6. macOS 迁移必须读取旧 `MacOsKeychainStore` 的既有 RootKey，先原子写入本地，再用该密钥验证现有受支持密文均可解密，验证成功后才将文件存储设为日常来源。
7. 迁移成功后尝试清理旧 Keychain 项。该清理为非阻塞后处理；失败不得回滚已验证的本地密钥或阻止日常使用。
8. 迁移的读取、写入或验证失败时，保留旧 SQLite 密文及旧 Keychain 项，并返回可诊断的锁定原因，不得泄露 RootKey。
9. 统一启动、账户凭据和网关 API Key 的增删改查路径对 `RootKeyStore` 进行注入；不再直接构造 `MacOsKeychainStore`。
10. 启动跟踪、锁定原因和测试应以通用安全存储或本地 RootKey 命名，不再将该步骤统称为 `keychain`。
11. 仅在确认无其他引用后，移除不再需要的 macOS `keyring` 依赖。

## 非功能需求

- 不得在日志、错误、Debug 输出、遥测或断言消息中暴露 RootKey 或其可恢复表示。
- 并发启动或命令初始化必须收敛为同一 RootKey，且不能产生部分文件或竞争生成的不同密钥。
- 所有失败路径必须可诊断，但诊断信息只能描述存储阶段、验证阶段或锁定原因。
- 本地日常读取不得依赖或访问 macOS Keychain；仅迁移判定要求时允许一次性访问。

## 范围

包含：

- 文件型 RootKeyStore、首次生成、本地读取、权限控制、原子写入和并发初始化。
- 启动与命令层的统一依赖注入。
- 从 `MacOsKeychainStore` 一次性迁移、既有密文验证和迁移后的旧项清理。
- 启动步骤命名、锁定原因及测试更新。
- 对 `keyring` 依赖的引用审查与条件移除。

不包含：

- SQLite schema 变更。
- 已有业务凭据的重新加密。
- AES-256-GCM 格式或 AAD 变更。
- 通用 master password 或 `.local_key` 机制变更。
- 用户可见的迁移界面。

## 接口与数据

`RootKeyStore` 是 RootKey 的唯一日常访问抽象。启动与凭据命令通过该抽象取得 32 字节 RootKey，随后沿用现有 AES-256-GCM 解密流程和 SQLite 数据结构。

本地密钥文件是应用私有数据，不是 SQLite 数据的一部分。实现计划须确定其确切路径、编码/二进制表示及原子替换机制，但这些选择不得改变 RootKey 的 32 字节语义或现有密文格式。

迁移状态机按以下顺序执行：

1. 检查本地密钥是否存在且有效；有效时直接使用，不访问 Keychain。
2. 本地密钥缺失或无效时，检查 SQLite 是否存在受保护密文。
3. 无既有密文时，生成 32 字节随机密钥，原子保存为本地文件并使用。
4. 有既有密文时，macOS 读取旧 Keychain RootKey；其他平台或旧密钥不可用时进入锁定状态。
5. 将读取的旧 RootKey 原子写入本地文件。
6. 使用本地文件所代表的密钥验证所有受支持的既有密文可解密。
7. 验证成功后切换为本地 RootKeyStore，并尝试删除旧 Keychain 项；清理失败仅记录非敏感诊断。

## 失败模式

- 本地文件缺失或无效且 SQLite 有既有密文：禁止生成新密钥；返回说明本地密钥不可用、需要迁移或恢复的锁定状态。
- 旧 Keychain 读取失败、用户拒绝授权或旧项不存在：保留旧项与 SQLite 密文，迁移不切换，返回迁移读取锁定状态。
- 本地目录创建、权限设置、原子写入或替换失败：不宣告迁移成功；保留旧项与 SQLite 密文，返回本地持久化锁定状态。
- 迁移后任一受支持密文无法解密：不切换至本地存储，不清理旧项，返回验证失败锁定状态。
- 旧 Keychain 清理失败：迁移保持成功，本地密钥继续作为唯一日常来源；仅返回或记录非阻塞清理诊断，不回滚。
- 并发初始化冲突：调用方最终读取同一完整有效 RootKey；无法保证该条件时失败且不得留下部分文件。

## 验收标准

- 全新配置目录启动创建 32 字节 RootKey 的本地表示，目录为 `0700`、文件为 `0600`，写入原子完成，且全程不调用 Keychain。
- 存在有效本地 RootKey 时，重复启动和凭据操作复用同一密钥且不访问 Keychain。
- 存在旧 Keychain RootKey 和 SQLite 密文时，迁移后所有受支持密文仍能解密；切换后仅使用本地 RootKeyStore。
- 迁移读取、写入或验证失败时，旧 SQLite 密文和旧 Keychain 项均保留，并产生不含 RootKey 的可诊断锁定状态。
- 本地密钥缺失或无效且 SQLite 已有密文时，不会生成或写入替代密钥。
- 并发初始化不产生不同 RootKey、部分文件或不安全权限。
- 账户凭据与网关 API Key 的增删改查均不直接构造 `MacOsKeychainStore`。
- 启动跟踪和测试不再将通用安全存储步骤命名为 `keychain`。
- 确认 `keyring` 无其他用途后移除依赖，且 Rust 格式化、静态检查和相关测试通过。

## 兼容性与迁移

现有 SQLite schema、记录内容、AES-256-GCM 密文格式及 AAD 必须完全兼容。迁移只迁移 RootKey 的存储位置，不修改或重新加密业务数据。

macOS 旧安装在首次需要迁移时可能出现一次 Keychain 授权提示；迁移验证成功后，启动和日常凭据操作不得再触发该访问。非 macOS 平台不读取 Keychain，直接使用文件型 RootKeyStore；若其已有密文而本地密钥缺失或无效，则保持锁定而非创建新密钥。

## 范围外事项

- 为迁移增加 GUI、向导或任何用户可见交互界面。
- 支持不同的 RootKey 长度、轮换策略或旧密文批量改写。
- 变更 `.local_key`、通用 master password 或非 AI 路由凭据的密钥管理行为。

## 假设

- 本地密钥存放于 `~/.config/onespace` 的应用私有路径，文件名将在实施计划中确定。
- 旧 Keychain 项仍使用服务名 `com.onespace.ai-routing-gateway` 与账户名 `root-data-key-v1`。
- Keychain 仅是 macOS 上的历史迁移来源；迁移后其清理失败不会破坏已验证的本地密钥。
- 现有代码可枚举或验证全部受支持的受保护密文，以支持迁移前的完整可解密性验证。

## 开放问题

N/A
