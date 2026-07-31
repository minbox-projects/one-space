# 02 - 实现 Keychain 与凭据加密安全边界

- task_id: `02-keychain-credential-security`
- order: `02`
- blocked_by: `01-shared-sqlite-schema`
- source_plan: `../plan.md`
- source_plan_digest: `385b139e1c25f8e8112982ed63ac3c3f0282be095c8322006f82f45d9070cf6d`
- write_scope: `src-tauri/src/ai_routing_gateway/{security.rs,key_service.rs,credentials.rs,tests/security.rs}、src-tauri/Cargo.toml、src-tauri/Cargo.lock`

## Outcome

OAuth Token 和第三方 API Key 能以逐记录 AES-256-GCM 密文持久化，根密钥由 macOS Keychain 管理，密钥缺失或密文异常时进入可诊断的锁定或凭据不可用状态。

## Implementation Checklist

- [ ] 使用兼容当前 Rust/Tauri 工具链的 `keyring` 封装独立网关根密钥服务，不复用 `.local_key`。
- [ ] 实现带随机 nonce、算法版本以及记录类型和实体 ID AAD 的 AES-256-GCM 加解密。
- [ ] 仅在全新且不存在网关密文时创建根密钥；已有密文但 Keychain 项缺失时进入锁定状态。
- [ ] 区分 Keychain 暂时不可访问、密钥缺失、未知版本、AAD 不匹配和认证失败，并对错误信息脱敏。
- [ ] 提供 OAuth 与第三方带类型凭据载荷的写入、替换、读取和永久删除接口。

## Acceptance Criteria

- [ ] 相同明文重复加密产生不同 nonce 和密文，正确 AAD 可解密，篡改、错误 AAD 和未知版本均失败。
- [ ] 全新数据库可创建并重新加载根密钥；已有密文但根密钥缺失时不会生成替代密钥。
- [ ] Keychain 暂时不可访问时不把根密钥或明文凭据缓存到磁盘。
- [ ] 解密失败仅暴露安全类别和实体 ID，不包含 Token、API Key、密文内容或内部堆栈。
- [ ] 凭据读取接口只向领域层返回受控敏感值，任何 IPC DTO 都无法直接取得凭据明文。
- [ ] 测试验证实现未读取或复用 `.local_key`。

## Verification Steps

- [ ] 执行隔离 keyring 替身测试，覆盖首建、加载、暂时失败、密钥丢失、禁止覆盖和重新录入恢复。
- [ ] 执行加密测试，覆盖随机 nonce、AAD、篡改、版本和错误脱敏。
- [ ] 执行 `cargo test` 与 `cargo check`。

## Out of Scope

不实现 OAuth 流程、网关 API Key 鉴权、账号管理或其他平台的凭据存储承诺。
