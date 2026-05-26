# OneSpace Claude 多 Profile 隔离集成计划

## Summary

新增 Claude 专属 multi-profile 能力：每个 Claude profile 固定保存在本机 /Users/yuqiyu/.config/onespace/claude_profiles/<profile-id>，该目录直接作为 CLAUDE_CONFIG_DIR。OneSpace 启动 Claude 会话时注入
对应 profile 的 CLAUDE_CONFIG_DIR，实现多环境并行隔离。

现有 Claude 全局配置方式保留，仍作为兼容区支持写入 ~/.claude/settings.json；Codex/Gemini/OpenCode 的界面和行为不改。

精简后的 CLI：

- onespace claude profile <profile> [-- <claude args>]：启动一次 Claude，临时注入该 profile 的 CLAUDE_CONFIG_DIR。
- onespace claude profile set <profile>：持久设置 OneSpace 默认 Claude profile。
- onespace claude profile list：列出 Claude profiles、默认标记和 config dir。
- onespace ai claude：使用当前默认 Claude profile 启动并写入 OneSpace session。
- onespace env ... 保留为通用 provider 管理入口；Claude 新能力优先走 onespace claude profile ...。

## Key Changes

- Claude provider 记录复用为 Claude profile，并新增/暴露：
    - claude_config_dir：运行时计算出的绝对路径，如 /Users/yuqiyu/.config/onespace/claude_profiles/work，不参与同步。
    - claude_profile_dir_name：可选稳定目录名；缺省使用 provider id 的安全化版本。
- Claude profile 文件只保存在本机 config::get_app_dir()/claude_profiles，不进入 shared/local_data 同步目录。
- 保存、设为默认、启动前都会 ensure/materialize 对应 profile 目录的 settings.json，保留该目录内已有 OAuth、历史、插件等文件。
- 新增 provider_id: Option<String> 到 session schema；新建 Claude 会话绑定 active Claude provider，resume 使用创建时绑定的 profile。
- 旧 Claude session 没有 provider_id 时回退到当前默认 Claude profile，再回退到全局 ~/.claude。
- “Apply to Global CLI” 继续作为高级兼容动作，只有显式触发时才写 ~/.claude/settings.json。

## Implementation Changes

- Rust 后端：
    - 新增 Claude profile path/materialize helpers，路径基于 config::get_app_dir()/claude_profiles/<safe-id>。
    - 扩展 SessionRecord、SessionInput、session_to_legacy，加入 provider_id 并保持旧 state 兼容。
    - 修改 sessions_create、sessions_launch、lookup_env_for_session：Claude 根据 session 绑定 provider 注入 CLAUDE_CONFIG_DIR。
    - 修改 Claude history/session resolver：优先读取 LaunchOptions.env["CLAUDE_CONFIG_DIR"] 下的 history/projects，兼容 ~/.claude。
    - 扩展 internal CLI commands：解析 profile list、resolve、set，避免 shell 直接 sed 修改快照。
    - 更新 install_cli 脚本：新增 onespace claude profile ...；保留现有 env 命令，Claude 场景提示使用新入口。
- Frontend：
    - 只重构 AiEnvironments 的 Claude 分支，参考 docs/prototypes/claude-multi-env-ui.html 做 Profiles 主视图。
    - Claude cards 展示默认标记、config dir、模型、认证方式、Launch、Set Default、Copy Command、Open Dir、Edit。
    - 编辑区保留现有 Claude 连接、模型路由、权限字段，并增加隔离说明和 config dir 展示。
    - 保存 Claude profile 不自动调用 projection_apply；只有“Apply to Global CLI”写全局。
    - Codex/Gemini/OpenCode 继续走原界面。
- CLI behavior：
    - onespace claude profile work -- --model opus 等价于 env CLAUDE_CONFIG_DIR='/Users/yuqiyu/.config/onespace/claude_profiles/work' claude --model opus。
    - 一次性 profile 启动不写 OneSpace session placeholder；它作为 shell escape hatch，后续由扩展后的 Claude history sync 尽量补录。
    - onespace ai claude 仍负责创建 OneSpace session，并使用默认 profile。

## Test Plan

- Rust 单元测试：
    - Claude profile path 固定在 config::get_app_dir()/claude_profiles，目录名安全化。
    - materialize 只写 profile 目录，不修改 ~/.claude。
    - 保存、set default、启动前都会 ensure/materialize。
    - sessions_create 为新 Claude session 绑定 active provider id。
    - sessions_launch 对绑定 profile 注入正确 CLAUDE_CONFIG_DIR。
    - 旧 Claude session 无 provider id 时按默认 profile/global fallback。
    - CLI internal claude profile list/resolve/set 能按 id 或 name 找到 profile。
- Frontend validation：
    - npm run build
    - Claude profile cards、编辑表单、默认标记、copy command、open dir 行为可用。
    - Codex/Gemini/OpenCode 页面无布局或行为变化。
- Manual scenarios：
    - onespace claude profile work
    - onespace claude profile personal
    - 两者 CLAUDE_CONFIG_DIR 不同，可并行登录/配置。
    - onespace claude profile set work 后，新建 OneSpace Claude 会话默认使用 work profile。
    - “Apply to Global CLI” 后，裸 claude 仍按全局配置工作。

## Assumptions

- Claude profile 文件是本机私有运行数据，不同步；只同步 provider/profile 元数据和 API 配置。
- onespace claude profile <profile> 是一次性启动，不尝试永久修改父 shell 环境。
- profile set 是 OneSpace 内的持久默认选择，不改写 ~/.claude。
- 空 API key 的 Claude profile 允许存在，用于 OAuth/manual login 到该 profile 目录。
- 现有全局 Claude 配置入口保留，但作为兼容/高级动作，不作为新 multi-profile 的默认切换机制。