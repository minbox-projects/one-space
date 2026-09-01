# Feature navigation

| Feature | Module Root | Read Order | Symbols | Tests | Responsibility |
| --- | --- | --- | --- | --- | --- |
| AI 环境 (`ai-environments`) | `frontend-components` | `src/components/AiEnvironments/index.tsx`, `src-tauri/src/ai_env/` | `AiEnvironments` | `src/components/AiEnvironments/AiEnvironments.test.tsx` | 管理 AI 环境配置 |
| AI 助手 (`ai-assistant`) | `frontend-lib` | `src/lib/aiAssistant.ts`, `src-tauri/src/ai_assistant/` | `aiAssistant` | - | AI 助手核心功能 |
| AI 会话 (`ai-sessions`) | `tauri-source` | `src-tauri/src/ai_sessions/` | `aiSessions` | - | 管理 AI 会话 |
| 技能 (`skills`) | `frontend-components` | `src/components/Skills/index.tsx`, `src/lib/skills.ts`, `src-tauri/src/skills/` | `Skills`, `skills` | - | 管理技能系统 |
| 子代理 (`subagents`) | `frontend-components` | `src/components/Subagents/index.tsx`, `src/lib/subagents.ts`, `src-tauri/src/subagents/` | `Subagents`, `subagents` | - | 管理子代理系统 |
| MCP 服务器 (`mcp-servers`) | `frontend-components` | `src/components/MCPServers/index.tsx`, `src-tauri/src/mcp_servers/` | `MCPServers` | - | 管理 MCP 服务器 |
| SSH 隧道 (`ssh-tunnels`) | `frontend-lib` | `src/lib/sshTunnels.ts`, `src-tauri/src/ssh_tunnels/` | `sshTunnels` | - | 管理 SSH 隧道 |
| 工作流 (`workflows`) | `frontend-lib` | `src/lib/workflows.ts`, `src-tauri/src/workflows/` | `workflows` | - | 管理工作流 |
| 工作台 (`workspaces`) | `frontend-components` | `src/components/Workspaces/index.tsx`, `src/lib/aiWorkspace.ts`, `src-tauri/src/workspaces.rs` | `Workspaces`, `aiWorkspace` | `src/components/Workspaces/Workspaces.test.tsx` | 管理工作台 |
| 更多工具 (`more-tools`) | `frontend-components` | `src/App.tsx`, `src/components/MoreToolsHub.tsx`, `src/components/Launcher.tsx`, `src/lib/navigation.ts`, `src/lib/launcherToolVisibility.ts` | `navigateToTab`, `MoreToolsHub`, `Launcher` | `src/App.moreToolsNavigation.test.tsx`, `src/components/MoreToolsHub.test.tsx`, `src/components/Launcher.test.tsx`, `src/lib/navigation.test.ts`, `src/lib/launcherToolVisibility.test.ts` | More Tools 的应用外壳、启动台、导航、展示与可见性 |
