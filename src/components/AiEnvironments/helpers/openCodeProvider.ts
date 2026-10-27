import type { AiProvider } from '@/lib/serviceProviders';

export const getOpenCodeJson = (provider: Partial<AiProvider>) => {
  const internalFields = [
    'id', 'tool', 'is_enabled', 'provider_key', 'api_key', 'base_url', 'model',
    'claude_default_model', 'dangerously_skip_permissions', 'history',
    'enable_all_memory_features', 'enable_mcp', 'allowed_tools', 'blocked_tools',
    'max_session_turns', 'disable_response_storage', 'personality', 'wire_api',
    'antigravity_auth_type', 'opencode_default_model', 'opencode_default_agent',
    'opencode_sessions_dir', 'model_reasoning_effort', 'model_reasoning_summary',
    'approval_policy', 'sandbox_mode', 'theme', 'vim_mode', 'default_approval_mode',
    'small_model', 'timeout', 'share_mode', 'env_managed', 'claude_reasoning_effort',
    'tool_config', 'extra', 'favorite_at', 'fetched_models', 'icon', 'code'
  ];

  const filtered: any = {};
  Object.keys(provider).forEach(key => {
    if (!internalFields.includes(key)) {
      filtered[key] = provider[key];
    }
  });

  return JSON.stringify(filtered, null, 2);
};
