import { skillModelOptions } from '../skillsModelOptions';
import type { MCPModelSwitchState } from './types';

export const DEFAULT_MCP_MODEL_SWITCH_STATE: MCPModelSwitchState = {
  claude: false,
  codex: false,
  antigravity: false,
  opencode: false,
};

export const MCP_MODEL_OPTIONS = skillModelOptions;
