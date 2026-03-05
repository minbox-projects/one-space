import { ClaudeIcon, GeminiIcon, OpenAIIcon, OpenCodeIcon } from './AiEnvironments/icons';

export type SkillModelId = 'claude' | 'gemini' | 'codex' | 'opencode';

export const skillModelOptions = [
  { id: 'claude' as const, label: 'Claude Code', Icon: ClaudeIcon },
  { id: 'gemini' as const, label: 'Gemini', Icon: GeminiIcon },
  { id: 'codex' as const, label: 'Codex', Icon: OpenAIIcon },
  { id: 'opencode' as const, label: 'OpenCode', Icon: OpenCodeIcon },
];
