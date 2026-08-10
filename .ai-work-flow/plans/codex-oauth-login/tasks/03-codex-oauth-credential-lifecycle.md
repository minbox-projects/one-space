# 接通 OAuth 加密凭据、刷新轮换与退出生命周期

## 预期结果

依赖任务 02；复用现有稳定身份 upsert 和 RootKey + AES-256-GCM 边界，完成凭据原子保存、到期前刷新、refresh token rotation、单次刷新与原请求重试、有限退避、永久失效重新授权标记，以及本地退出和可选远端撤销。该任务已整合至 integration，后续不得重新实施。

## 实施清单

- [x] 已完成并整合至 `integration`；后续实施不得重做或改写本任务的生产实现。
- [x] 已复用 `upsert_oauth_account` 与 RootKey + AES-256-GCM 边界，按稳定身份原子保存加密凭据并保持 workspace 隔离。
- [x] 已实现到期前刷新、refresh token rotation 整组原子替换、最多一次刷新与一次原请求重试，以及有限退避和永久重新授权标记。
- [x] 已实现本地凭据清除与账号禁用，并仅在可靠 revoke endpoint 存在时执行可选远端撤销。
- [x] 已保持 API Key 路由、Gmail OAuth 数据、OAuth schema 与价格覆盖不变。

## 验收标准

- [ ] 登录成功的 OAuth 凭据只以现有 AES-256-GCM schema 加密保存，数据库与公开 metadata 中不可检索到明文 token。
- [ ] 相同稳定主体重复登录更新同一账号，不同 workspace 不互相覆盖，任一加密或事务失败不留下部分成功记录。
- [ ] 路由在到期前刷新，rotation 以整组凭据原子替换；授权失败最多发生一次刷新和一次原请求重试。
- [ ] 临时错误退避次数和时长有明确上限，永久错误停止账号参与路由并设置 `oauth_reauthorization_required`。
- [ ] 退出登录始终清除本地凭据并禁用账号；没有可靠 revoke endpoint 或远端撤销失败时，本地结果仍成功提交。
- [ ] API Key 账号的凭据读取、路由重试和编辑行为未发生改变。

## 验证步骤

- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::accounts`，预期身份去重、workspace 隔离、密文无泄漏、rotation 与退出登录测试通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml ai_routing_gateway::runtime`，预期到期前刷新、单次授权重试、有限退避和永久重新授权测试通过。
- [ ] 运行 `cargo test --manifest-path src-tauri/Cargo.toml`，预期 API Key、存储、路由和网关全量 Rust 回归通过。

## 范围外事项

- 不新增数据库 schema，不迁移 Gmail OAuth 或既有 API Key 数据。
- 不负责 Tauri invoke 注册、typed IPC facade 或 React 授权界面。
