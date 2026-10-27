import type { AiProvider, ClaudeProfileSummary } from '@/lib/serviceProviders';
import type { ClaudePresetModelMapping } from '../providerPresets';

export type ClaudeModelMappingDraft = {
  family: string;
  display_name: string;
  upstream_model: string;
  supports_1m?: boolean;
  supported_capabilities?: string[];
};

export type ClaudeRoutingFieldSource = {
  claude_api_format?: string;
  claude_connection_mode?: string;
  tool_config?: Record<string, any>;
};

export const getClaudeApiFormat = (source: ClaudeRoutingFieldSource) =>
  source.claude_api_format || source.tool_config?.claude_api_format || 'anthropic_messages';

export const getClaudeConnectionMode = (source: ClaudeRoutingFieldSource) => {
  const explicitMode = source.claude_connection_mode || source.tool_config?.claude_connection_mode;
  if (explicitMode) return explicitMode;
  const apiFormat = getClaudeApiFormat(source);
  return apiFormat === 'open_ai_chat' || apiFormat === 'open_ai_responses'
    ? 'protocol_router'
    : 'native_anthropic';
};

export const normalizeClaudeModelMappingDraft = (mapping: Partial<ClaudeModelMappingDraft>): ClaudeModelMappingDraft => ({
  family: String(mapping.family || '').trim(),
  display_name: String(mapping.display_name || '').trim(),
  upstream_model: String(mapping.upstream_model || ''),
  supports_1m: !!mapping.supports_1m,
  supported_capabilities: Array.isArray(mapping.supported_capabilities)
    ? mapping.supported_capabilities
        .map((value) => String(value ?? '').trim())
        .filter((value) => value.length > 0)
    : undefined,
});

export const normalizeClaudeDefaultModel = (value?: string) => {
  if (typeof value !== 'string') return undefined;
  const normalized = value.trim();
  return normalized.length > 0 ? normalized : undefined;
};

const PRESET_CLAUDE_FAMILIES: Array<Pick<ClaudePresetModelMapping, 'family' | 'display_name'>> = [
  { family: 'haiku', display_name: 'Haiku' },
  { family: 'sonnet', display_name: 'Sonnet' },
  { family: 'opus', display_name: 'Opus' },
];

export const buildPresetClaudeMappings = (
  source?: Array<Partial<ClaudePresetModelMapping>> | null,
): ClaudePresetModelMapping[] =>
  PRESET_CLAUDE_FAMILIES.map(({ family, display_name }) => {
    const mapping = source?.find((item) => String(item?.family || '').trim() === family);
    return {
      family,
      display_name: String(mapping?.display_name || display_name).trim() || display_name,
      upstream_model: String(mapping?.upstream_model || ''),
      supports_1m: !!mapping?.supports_1m,
      supported_capabilities: Array.isArray(mapping?.supported_capabilities)
        ? mapping.supported_capabilities
            .map((value) => String(value ?? '').trim())
            .filter((value) => value.length > 0)
        : undefined,
    };
  });

export const buildClaudeModelMappings = (source: Record<string, any>): ClaudeModelMappingDraft[] => {
  const explicitMappings = source.claude_model_mappings || source.tool_config?.claude_model_mappings;
  if (Array.isArray(explicitMappings) && explicitMappings.length > 0) {
    return explicitMappings.map((mapping: Partial<ClaudeModelMappingDraft>) =>
      normalizeClaudeModelMappingDraft(mapping),
    );
  }

  const fromLegacyFields = [
    {
      family: 'haiku',
      display_name: 'Haiku',
      upstream_model: String(
        source.claude_haiku_model ||
        source.tool_config?.claude_haiku_model ||
        '',
      ),
      supports_1m: false,
      supported_capabilities: undefined,
    },
    {
      family: 'sonnet',
      display_name: 'Sonnet',
      upstream_model: String(
        source.claude_sonnet_model ||
        source.tool_config?.claude_sonnet_model ||
        '',
      ),
      supports_1m: false,
      supported_capabilities: undefined,
    },
    {
      family: 'opus',
      display_name: 'Opus',
      upstream_model: String(
        source.claude_opus_model ||
        source.tool_config?.claude_opus_model ||
        '',
      ),
      supports_1m: false,
      supported_capabilities: undefined,
    },
  ].filter((mapping) => mapping.upstream_model.trim().length > 0);

  return fromLegacyFields;
};

export const buildClaudeProviderFromProfile = (profile: ClaudeProfileSummary): Partial<AiProvider> => {
  const defaultModel = normalizeClaudeDefaultModel(
    profile.tool_config?.claude_default_model || profile.model || undefined,
  );
  return ({
  id: profile.id,
  tool: 'claude',
  name: profile.name,
  icon: profile.icon || undefined,
  code: profile.code || undefined,
  api_key: profile.raw_api_key || '',
  base_url: profile.raw_base_url || '',
  model: defaultModel,
  claude_api_format: getClaudeApiFormat(profile),
  claude_connection_mode: getClaudeConnectionMode(profile),
  claude_auth_env_key: profile.tool_config?.claude_auth_env_key || 'ANTHROPIC_API_KEY',
  claude_model_mappings: buildClaudeModelMappings(profile),
  claude_enable_tool_search:
    profile.tool_config?.claude_enable_tool_search ?? profile.tool_config?.enable_tool_search ?? false,
  claude_enable_attribution:
    profile.tool_config?.claude_enable_attribution ?? profile.tool_config?.enable_attribution ?? false,
  claude_auto_memory_enabled: profile.tool_config?.claude_auto_memory_enabled ?? false,
  claude_always_thinking_enabled: profile.tool_config?.claude_always_thinking_enabled ?? false,
  claude_away_summary_enabled: profile.tool_config?.claude_away_summary_enabled ?? false,
  claude_include_git_instructions: profile.tool_config?.claude_include_git_instructions ?? false,
  remark: profile.tool_config?.remark || '',
  claude_default_model: defaultModel,
  claude_reasoning_effort: profile.tool_config?.claude_reasoning_effort,
  dangerously_skip_permissions: profile.tool_config?.dangerously_skip_permissions || false,
  enable_all_memory_features: profile.tool_config?.enable_all_memory_features || false,
  enable_mcp: profile.tool_config?.enable_mcp || false,
  allowed_tools: profile.tool_config?.allowed_tools || [],
  blocked_tools: profile.tool_config?.blocked_tools || [],
  max_session_turns: profile.tool_config?.max_session_turns,
  env_managed: true,
  is_enabled: true,
})};

export const buildClaudeProviderFromState = (provider: AiProvider): Partial<AiProvider> => {
  const defaultModel = normalizeClaudeDefaultModel(
    provider.claude_default_model || provider.model || provider.tool_config?.claude_default_model,
  );
  return ({
  ...provider,
  tool: 'claude',
  model: defaultModel,
  claude_default_model: defaultModel,
  remark: provider.tool_config?.remark || '',
  claude_api_format: getClaudeApiFormat(provider),
  claude_connection_mode: getClaudeConnectionMode(provider),
  claude_model_mappings: buildClaudeModelMappings(provider),
})};

export const normalizeProviderForSave = (provider: AiProvider): AiProvider => {
  const next = { ...provider };
  const nextToolConfig = { ...(provider.tool_config || {}) };
  const remark = typeof provider.remark === 'string' ? provider.remark : '';

  if (remark.trim()) {
    nextToolConfig.remark = remark;
  } else {
    delete nextToolConfig.remark;
  }

  if (provider.tool === 'claude') {
    const defaultModel = normalizeClaudeDefaultModel(provider.claude_default_model || provider.model);
    if (Array.isArray(provider.claude_model_mappings)) {
      next.claude_model_mappings = provider.claude_model_mappings.map((mapping) =>
        normalizeClaudeModelMappingDraft(mapping),
      );
    }
    if (defaultModel) {
      next.model = defaultModel;
      next.claude_default_model = defaultModel;
      nextToolConfig.claude_default_model = defaultModel;
    } else {
      delete next.model;
      delete next.claude_default_model;
      delete nextToolConfig.claude_default_model;
    }
    if (typeof provider.claude_reasoning_effort === 'string') {
      nextToolConfig.claude_reasoning_effort = provider.claude_reasoning_effort;
    } else {
      delete nextToolConfig.claude_reasoning_effort;
    }
  }

  next.tool_config = nextToolConfig;
  return next;
};

export const getClaudeMappingTags = (profile: ClaudeProfileSummary) => {
  const explicitMappings = Array.isArray(profile.claude_model_mappings)
    ? profile.claude_model_mappings
    : Array.isArray(profile.tool_config?.claude_model_mappings)
      ? profile.tool_config.claude_model_mappings
      : [];

  const explicitTags = explicitMappings
    .map((mapping: any) => {
      const family = String(mapping?.family || '').trim();
      const upstream = String(mapping?.upstream_model || '').trim();
      if (!upstream) return null;
      return family ? `${family}: ${upstream}` : upstream;
    })
    .filter((value: string | null): value is string => Boolean(value));

  if (explicitTags.length > 0) {
    return explicitTags;
  }

  return [
    ['default', profile.tool_config?.claude_default_model || profile.model],
  ]
    .map(([family, value]) => {
      const upstream = String(value || '').trim();
      return upstream ? `${family}: ${upstream}` : null;
    })
    .filter((value): value is string => Boolean(value));
};
