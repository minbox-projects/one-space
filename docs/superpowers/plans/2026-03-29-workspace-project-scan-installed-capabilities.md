# Workspace Project Scan Installed Capabilities Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将工作空间详情页及其后端 project scope 查询改为“从项目目录扫描 skills/subagents”，不再依赖 `installed_state.json` 保存项目级安装记录；`installed_state.json` 仅保留全局安装状态。

**Architecture:** 保持前端调用接口不变，把 `scope=project` 的后端语义整体切换为“即时扫描项目目录 + 用 catalog/repository 元数据补全来源信息”。全局安装继续沿用本地 `installed_state.json`，项目安装不再写入本地索引，从而消除工作空间详情页对 project installed_state 的脆弱依赖。

**Tech Stack:** Rust, Tauri 2, React 19, TypeScript, serde, workspace-local JSON state

---

### Task 1: 明确 project scope 的新数据语义与边界

**Files:**
- Modify: `src-tauri/src/skills.rs`
- Modify: `src-tauri/src/subagents.rs`

- [ ] 将 `project scope` 的 source of truth 定义为项目目录中的真实文件系统内容，而不是 `installed_state.json`。
- [ ] 保持 `global scope` 继续读取和维护 `installed_state.json`。
- [ ] 明确 project 扫描目录：
  - Skills: Claude `.claude/skills`、Codex `.agents/skills`（兼容 `.codex/skills` 仅用于同步，不作为主扫描源）、Gemini `.gemini/skills`、OpenCode `.opencode/skills`
  - Subagents: Claude `.claude/agents`、Codex `.codex/agents`、Gemini `.gemini/agents`、OpenCode `.opencode/agents`
- [ ] 约定 project 扫描结果至少要产出与当前 `SkillRecord` / `SubagentRecord` 兼容的字段：`id`、`dir_name`、`model`、`models`、`name`、`description`、`source_id`、`source_rel_path`、`scope`、`project_root`、`target_path`
- [ ] 明确哪些字段为“扫描可得”，哪些字段需要 hydration：
  - 扫描可得：`name`、`description`、`models`、`dir_name`、`local_hash`
  - hydration 补全：`id`、`source_id`、`source_rel_path`、`remote_hash`、`icon_seed`
  - 无法稳定恢复的历史字段不再作为 project scope 的强依赖：`installed_at`、`updated_at`、`last_synced_at`

### Task 2: 为 skills/subagents 增加项目目录扫描与 hydration helper

**Files:**
- Modify: `src-tauri/src/skills.rs`
- Modify: `src-tauri/src/subagents.rs`
- Test: `src-tauri/src/skills.rs`
- Test: `src-tauri/src/subagents.rs`

- [ ] 在 `skills.rs` 中新增 project 扫描 helper，按 `project_root + model` 扫描技能目录并解析 `SKILL.md`。
- [ ] 在 `subagents.rs` 中新增 project 扫描 helper，按 `project_root + model` 扫描 agent 目录并解析 `AGENT.md`。
- [ ] 让扫描 helper 返回“临时本地记录”，默认以 `source_id=local` 起步，并填入 `scope=project`、`project_root`、`target_path`。
- [ ] 复用或抽取现有 `hydrate_local_records_from_catalog` 思路，让 project 扫描记录在命中 catalog/repository 时补齐来源信息，而不是长期停留在 `local`。
- [ ] 补测试覆盖：
  - project 目录中存在 skill/subagent 时，扫描结果能生成正确记录
  - Claude/Codex/Gemini/OpenCode 至少各覆盖一个目录解析用例
  - Codex skills 只从主目录扫描，不因兼容目录重复计数
  - hydration 成功时能把 `local` 记录提升为 repo/source 对应标识

### Task 3: 切换 project scope 查询接口到扫描式实现

**Files:**
- Modify: `src-tauri/src/skills.rs`
- Modify: `src-tauri/src/subagents.rs`
- Test: `src-tauri/src/skills.rs`
- Test: `src-tauri/src/subagents.rs`

- [ ] 修改 `skills_list_installed(scope=project)`，改为直接返回扫描结果，不再读取 `local_state.skills` 中的 project 记录。
- [ ] 修改 `subagents_list_installed(scope=project)`，改为直接返回扫描结果，不再读取 `local_state.subagents` 中的 project 记录。
- [ ] 修改 `skills_repo_list(scope=project)`，让 repository installed 标记基于“项目扫描结果”计算，而不是 project installed_state。
- [ ] 修改 `subagents_repo_list(scope=project)`，让 repository installed 标记同样基于“项目扫描结果”计算。
- [ ] 保持前端接口协议不变，确保 [WorkspaceSkillsPanel.tsx](/Users/yuqiyu/AiHistorys/one-space/onespace-app/src/components/Workspaces/WorkspaceSkillsPanel.tsx) 和 [WorkspaceSubagentsPanel.tsx](/Users/yuqiyu/AiHistorys/one-space/onespace-app/src/components/Workspaces/WorkspaceSubagentsPanel.tsx) 无需感知底层来源切换。
- [ ] 补测试覆盖：
  - `list_installed(scope=project)` 返回项目扫描结果
  - `repo_list(scope=project)` 的 `installed` 标记与扫描结果一致
  - 父目录 workspace 不会误匹配子目录 `project_root`

### Task 4: 切换 project scope 的操作类接口到扫描结果寻址

**Files:**
- Modify: `src-tauri/src/skills.rs`
- Modify: `src-tauri/src/subagents.rs`
- Test: `src-tauri/src/skills.rs`
- Test: `src-tauri/src/subagents.rs`

- [ ] 梳理所有当前依赖 project installed_state 的接口，至少覆盖：
  - `*_detail_get`
  - `*_uninstall`
  - `*_open_folder`
  - `*_update_check`
  - `*_update_diff_preview`
  - `*_update_apply`
- [ ] 为 skills 增加“按 scope/model/project_root + id 从扫描结果里定位记录”的 helper，替代直接从 `local_state.skills` 查 project 记录。
- [ ] 为 subagents 增加同样的定位 helper，替代直接从 `local_state.subagents` 查 project 记录。
- [ ] 确保工作空间详情页上的 `重新安装`、`卸载`、`打开目录`、更新预览等后续操作仍然可用。
- [ ] 补测试覆盖：
  - project 安装项的 detail/open_folder 仍可返回正确路径
  - uninstall 能删除项目目录内容，即使没有 project installed_state
  - update_check / update_apply 在 project scope 下仍能命中正确 repo/source

### Task 5: 停止持久化 project 安装记录，并清理遗留数据

**Files:**
- Modify: `src-tauri/src/skills.rs`
- Modify: `src-tauri/src/subagents.rs`
- Test: `src-tauri/src/skills.rs`
- Test: `src-tauri/src/subagents.rs`

- [ ] 修改 `skills_install` / `skills_repo_set_model`，当 `scope=project` 时只写项目目录，不再向 `local_state.skills` 写入 project 记录。
- [ ] 修改 `subagents_install` / `subagents_repo_set_model`，当 `scope=project` 时只写项目目录，不再向 `local_state.subagents` 写入 project 记录。
- [ ] 修改对应 uninstall/reconcile/rescan 流程，使其不再假设 project 记录必须存在于本地状态。
- [ ] 在 `load_local_skills_state` / `load_local_subagents_state` 或单独迁移逻辑中清理历史 project 记录，避免遗留脏数据继续影响仓库 installed 计算。
- [ ] 保留全局 installed_state 的现有行为和文件结构，避免影响 Settings/全局 Skills/全局 Subagents 页面。
- [ ] 补测试覆盖：
  - project 安装后本地 installed_state 不新增 project 记录
  - 历史 project 记录存在时会被迁移或忽略，不影响查询结果
  - global 记录不受清理逻辑影响

### Task 6: 清理临时调试日志并补回归验证

**Files:**
- Modify: `src/components/Workspaces/WorkspaceSkillsPanel.tsx`
- Modify: `src/components/Workspaces/WorkspaceSubagentsPanel.tsx`
- Modify: `src-tauri/src/skills.rs`
- Modify: `src-tauri/src/subagents.rs`

- [ ] 删除为本次排查加入的 `console.debug` / `eprintln!` 临时日志，避免长期污染控制台与应用日志。
- [ ] 手工验证以下场景：
  - 打开 `onespace-app` 工作空间，已安装 cards 正常显示 `git-commit` 和 `code-reviewer`
  - 切到父目录 `one-space` workspace，不会误显示子目录安装项
  - 从 repository 安装后刷新页面，installed cards 无需依赖 installed_state 也能稳定显示
  - 重新安装、卸载后页面状态正确刷新
- [ ] 运行 `cargo test skills` 与 `cargo test subagents`（或等效针对性测试命令），确认新增用例通过。
- [ ] 运行 `cargo check` 与 `npm run build`，确认前后端编译通过。

### Task 7: 文档与迁移说明

**Files:**
- Modify: `docs/superpowers/plans/2026-03-29-workspace-project-scan-installed-capabilities.md`
- Optional Modify: `README.md`
- Optional Modify: 相关内部说明文档（若项目已有 skills/subagents 存储说明）

- [ ] 在实现提交说明中明确 project scope 与 global scope 的数据来源已经分离。
- [ ] 如仓库中已有相关存储设计文档，补充一条迁移说明：project 安装状态不再持久化到 `installed_state.json`。
- [ ] 记录已知限制：
  - project scope 的 `installed_at/updated_at` 若无额外持久化，将不再具备历史精度
  - 纯目录扫描依赖文件内容可解析，异常目录将被忽略或降级为 local 记录

