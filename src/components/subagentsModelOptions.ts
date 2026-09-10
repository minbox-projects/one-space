import { AntigravityIcon, ClaudeIcon, OpenAIIcon, OpenCodeIcon } from './AiEnvironments/icons';

export type SubagentModelId = 'claude' | 'antigravity' | 'codex' | 'opencode';

export const subagentModelOptions = [
  { id: 'claude' as const, label: 'Claude Code', Icon: ClaudeIcon },
  { id: 'antigravity' as const, label: 'Antigravity', Icon: AntigravityIcon },
  { id: 'codex' as const, label: 'Codex', Icon: OpenAIIcon },
  { id: 'opencode' as const, label: 'OpenCode', Icon: OpenCodeIcon },
];
