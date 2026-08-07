# 本地 RootKey 存储迁移计划

## 计划元数据

- plan-id: `local-root-key-storage`
- status: `ready-for-implementation`
- source_spec: `.ai-work-flow/plans/local-root-key-storage/spec.md`
- source_spec_digest: `c0b60f6c039735bc8b4cef5566242ba3ab38ee2bc3ef627ab750e2a315f6887f`
- task_mode: `single`

## 技术与代码上下文

- `src-tauri/src/ai_routing_gateway/security.rs` 的 `RootKeyStore` 目前只有 `load` 与 `store`，`MacOsKeychainStore` 是唯一实现；`initialize_security` 在此选择并缓存 RootKey。
- `src-tauri/src/commands/mod.rs` 的 `GatewayLifecycle` 持有默认存储和 RootKey 缓存，但 `security()` 以及多条账户凭据、网关 API Key 命令仍直接构造 `MacOsKeychainStore`。启动编排在同文件的 1048--1117 行，安全失败清理在 1028--1045 行。
- SQLite 已有密文的判定必须覆盖 `ai_gateway_credentials` 和 `ai_gateway_api_keys.ciphertext`。验证必须复用现有 AES-256-GCM 与 AAD 解密路径，不修改数据结构或密文。
- `mcp_servers/config_parse.rs` 与 `app_runtime/oauth_open.rs` 提供临时文件加替换的原子写入先例；`runtime_profiles.rs` 提供 Unix `0700`/`0600` 权限先例。`keyring` 目前仅由 `security.rs` 使用，且仅为 macOS target-specific 依赖。

## 实施方案

将日常 RootKey 来源改为文件型 `RootKeyStore`，并把 macOS Keychain 限定为本地文件不可用且已有密文时的一次性迁移来源。确定本地文件为 `~/.config/onespace/ai-routing-gateway-root-key-v1`，内容为未经编码的 32 字节 RootKey；该名称与旧 Keychain 账户版本一致，但不接触通用 `.local_key`。

文件存储以同目录的锁文件串行化初始化和迁移：持有独占文件锁后重新读取最终密钥文件，再决定复用、创建或迁移。锁覆盖存在性检查、生成/迁移写入和密文验证，确保同一进程及并发进程不会生成不同 RootKey。写入使用同目录、不可覆盖创建的临时文件：先以 `0600` 创建，写满 32 字节并 `sync_all`，再原子 `rename` 到最终路径并同步目录；任何写入或替换失败均删除临时文件且不报告成功。配置目录在使用前创建或修正为 `0700`，既有有效密钥文件在读取后修正为 `0600`；无法安全设置或读取权限即视为本地存储不可用。

macOS 迁移在锁内按状态机执行：有效本地文件直接返回，绝不访问 Keychain；本地文件缺失/无效时先检查全部受保护密文。不存在密文时生成并原子持久化新 32 字节随机 RootKey；存在密文时才读取旧 `MacOsKeychainStore`。旧密钥写入本地后，重新通过文件型存储读取，并对两个受支持表中每条现有密文按其现有 AAD 解密验证。仅全部成功后才把该文件型存储作为生命周期日常来源；随后尝试删除旧 Keychain 项。删除失败只保留非敏感诊断，不回滚本地文件、不阻塞使用。读取、持久化或验证失败则保留 SQLite 和旧 Keychain 项，移除本次未完成迁移产生的本地文件（不删除迁移前已存在的有效文件），并返回分阶段且不包含 RootKey 的锁定原因。

## 顺序执行步骤

1. 在 `security.rs` 增加文件型存储及最小必要的初始化协调、权限和原子写入辅助逻辑；固定路径、原始 32 字节文件格式及安全错误类型。
2. 重构 `initialize_security`，先取得生命周期注入的文件型日常存储，再在必要时执行 macOS 旧 Keychain 迁移状态机和完整密文验证。
3. 调整 `commands/mod.rs` 的 `GatewayLifecycle`、`security()` 和全部账户凭据及网关 API Key 命令，使其仅消费启动时注入的 `RootKeyStore`/已解析 RootKey；删除命令层对 `MacOsKeychainStore` 的构造。
4. 将启动跟踪、锁定原因和安全失败清理从 `keychain` 命名改为通用安全存储或本地 RootKey 命名，保留现有启动失败资源清理顺序。
5. 补齐存储、迁移和命令层回归测试，确认不再有 `keyring` 使用后移除 macOS target-specific 依赖及其锁文件条目。

## 任务边界与依赖

`task_mode` 为 `single`；不生成拆分任务清单或任务文件。实现按上述顺序在同一变更中完成，后续命令层注入依赖安全初始化先建立稳定的文件型日常来源。

## 具体改动

- `src-tauri/src/ai_routing_gateway/security.rs`：保留 `RootKeyStore` 作为日常访问边界，新增文件实现及必要的存储/初始化接口；将 `MacOsKeychainStore` 收窄为仅 macOS 迁移读取和迁移后删除的实现细节。校验读取值恰为 32 字节，所有错误仅携带阶段和原因。
- `src-tauri/src/ai_routing_gateway/tests.rs`：在现有安全测试区覆盖文件路径、二进制格式、长度拒绝、权限、原子失败清理、同一 RootKey 的并发初始化、无密文首次生成及有密文时禁止生成替代密钥。
- `src-tauri/src/commands/mod.rs`：让 `GatewayLifecycle` 持有单一注入存储/RootKey，替换 `security()` 和所有凭据/API Key CRUD 的直接 Keychain 构造；更新启动阶段名称、锁定理由和失败清理测试。
- `src-tauri/Cargo.toml` 及对应锁文件：仅当检索确认 `keyring` 无剩余引用时移除其 macOS target-specific 依赖；若文件锁需要新增最小跨平台 Rust 依赖，将其限定为安全模块并记录用途。

## 接口与数据流

启动先创建文件型存储并通过 `initialize_security` 解析 RootKey，结果缓存到 `GatewayLifecycle`。正常路径为文件读取；本地文件不存在或无效时，先枚举 SQLite 密文存在性，再在无密文场景生成，或在 macOS 有密文场景从旧 Keychain 迁移。命令层经生命周期获取相同的存储/RootKey，继续调用既有加解密和 SQLite 仓储接口。

迁移验证必须枚举 `ai_gateway_credentials` 的所有受保护记录，以及 `ai_gateway_api_keys.ciphertext` 的所有非空密文；每项均用既有记录上下文构造 AAD 并走生产解密函数。存在性检查与完整验证分开：前者决定是否允许新建密钥，后者决定是否允许迁移切换。

## 失败处理

- 有既有密文而本地密钥缺失、无效或权限不安全：不生成密钥；macOS 尝试一次迁移，其他平台返回锁定状态。
- Keychain 读取、目录创建、权限修正、临时写入、同步、替换或验证失败：不切换日常来源，不清理旧 Keychain，保留 SQLite；删除本次新建但未验证的文件和临时文件，并提供非敏感阶段诊断。
- 有效本地文件存在时：完全跳过 Keychain，即使旧项仍存在或此前清理失败。
- 旧 Keychain 删除失败：本地迁移仍成功，记录非阻塞诊断；后续启动不再读取 Keychain。
- 锁获取或锁内复查失败：不生成或替换最终密钥文件，向调用方返回本地存储锁定原因。

## 测试与验证

- 扩展 `ai_routing_gateway/tests.rs:177-325`：全新初始化不调用 Keychain；有效本地文件复用；缺失/长度错误文件被拒绝；`0700` 目录与 `0600` 文件；原子写入不暴露部分内容；并发初始化收敛为同一值。
- 覆盖 macOS 迁移成功、Keychain 拒绝/缺失、原子持久化失败、两张表任一密文解密失败和 Keychain 删除失败。断言失败时旧 Keychain 与 SQLite 不被删除、成功后日常读取不再访问 Keychain。
- 扩展 `commands/mod.rs:2000-2015`、`2067-2072`、`2532-2543` 附近测试：所有账户凭据和网关 API Key 路径使用生命周期注入的存储，启动跟踪不再标记为 `keychain`，安全失败继续触发现有清理。
- 运行 `cd src-tauri && cargo fmt --check`、`cd src-tauri && cargo clippy --all-targets --all-features -- -D warnings`、`cd src-tauri && cargo test`；在 macOS 上执行迁移相关测试，并检索 `keyring` 与 `MacOsKeychainStore` 的调用点确认仅保留迁移边界。

## 验收标准

- 新配置在 `~/.config/onespace/ai-routing-gateway-root-key-v1` 原子创建原始 32 字节 RootKey，目录为 `0700`、文件为 `0600`，且不访问 Keychain。
- 有效本地 RootKey 的启动和所有凭据/API Key 操作均复用同一密钥且无 Keychain 访问。
- 旧安装迁移后，`ai_gateway_credentials` 和 `ai_gateway_api_keys.ciphertext` 的所有受支持密文均可按原格式和 AAD 解密；仅成功后才清理旧 Keychain。
- 迁移读取、写入或验证失败时不生成替代密钥，不修改 SQLite，不删除旧 Keychain，且不泄露 RootKey。
- 命令层不存在直接构造 `MacOsKeychainStore` 的日常路径，启动跟踪使用通用安全存储/本地 RootKey 术语。

## 兼容、迁移与发布

不修改 SQLite schema、记录内容、AES-256-GCM 密文格式或 AAD，不重新加密业务凭据。macOS 升级仅在本地文件不可用且检测到既有密文时可能访问一次 Keychain；非 macOS 不读取 Keychain，并在已有密文且本地密钥不可用时保持锁定。发布前确认移除 `keyring` 不影响其他目标平台，并以现有凭据数据库副本验证一次升级迁移和后续无 Keychain 日常启动。
