import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { CheckCircle2, ChevronDown, FolderOpen, Loader2, Plus, RefreshCw, Save, Trash2, Wand2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { ToolIcon } from './AiEnvironments';
import { useConfirmDialog } from './ConfirmDialogProvider';
import {
  workflowsApplyDependencies,
  workflowsCheckDependencies,
  workflowsDeletePreset,
  workflowsListPresets,
  workflowsUpsertPreset,
  type WorkflowDependencyState,
  type WorkflowLaunchScope,
  type WorkflowPreset,
  type WorkflowPresetInput,
  type WorkflowTool,
} from '@/lib/workflows';

type ProviderLite = { id: string; name: string; tool: string };
type MCPServerLite = { id: string; name: string };
type SkillOptionGroup = 'recommended' | 'repository';
type SkillOptionLite = { value: string; name: string; group: SkillOptionGroup; aliases: string[] };
type ProvidersListResp = {
  data?: {
    providers?: Array<{ id?: string; name?: string; tool?: string }>;
    active_claude?: string;
    active_codex?: string;
    active_gemini?: string;
    active_opencode?: string | string[];
  };
};
type MCPStateResp = { servers?: Array<{ id?: string; name?: string }> };
type SkillsCatalogResp = { data?: Array<{ id?: string; name?: string; source_id?: string; rel_path?: string }> };
type SkillsRepoListResp = {
  data?: Array<{
    repo_key?: string;
    skill_id?: string;
    name?: string;
    source_id?: string;
    source_rel_path?: string;
    models?: string[];
  }>;
};

const TOOL_OPTIONS: WorkflowTool[] = ['claude', 'codex', 'gemini', 'opencode'];

function toolLabel(tool: WorkflowTool, t: (key: string, fallback?: string) => string): string {
  if (tool === 'claude') return t('workflowToolClaude', 'Claude Code');
  if (tool === 'codex') return t('workflowToolCodex', 'Codex');
  if (tool === 'gemini') return t('workflowToolGemini', 'Gemini');
  return t('workflowToolOpenCode', 'OpenCode');
}

function launchScopeLabel(scope: WorkflowLaunchScope, t: (key: string, fallback?: string) => string): string {
  if (scope === 'strict') return t('workflowLaunchScopeStrictShort', 'Strict');
  return t('workflowLaunchScopeSharedShort', 'Shared');
}

function encodeCatalogSkillValue(sourceId: string, relPath: string): string {
  return `catalog::${sourceId}::${relPath}`;
}

function encodeRepoSkillValue(repoKey: string): string {
  return `repo::${repoKey}`;
}

interface DraftPreset {
  id?: string;
  name: string;
  tool: WorkflowTool;
  working_dir: string;
  provider_id: string;
  mcp_server_ids: string[];
  required_skill_ids: string[];
  launch_prompt: string;
  launch_scope: WorkflowLaunchScope;
}

const EMPTY_DRAFT: DraftPreset = {
  name: '',
  tool: 'claude',
  working_dir: '',
  provider_id: '',
  mcp_server_ids: [],
  required_skill_ids: [],
  launch_prompt: '',
  launch_scope: 'shared',
};

function presetToDraft(preset: WorkflowPreset): DraftPreset {
  return {
    id: preset.id,
    name: preset.name,
    tool: preset.tool,
    working_dir: preset.working_dir || '',
    provider_id: preset.provider_id || '',
    mcp_server_ids: preset.mcp_server_ids || [],
    required_skill_ids: preset.required_skill_ids || [],
    launch_prompt: preset.launch_prompt || '',
    launch_scope: preset.launch_scope || 'shared',
  };
}

export function WorkflowPresetsPanel({
  onChanged,
  onSelectPreset,
}: {
  onChanged?: (presets: WorkflowPreset[]) => void;
  onSelectPreset?: (presetId: string | null) => void;
}) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const [presets, setPresets] = useState<WorkflowPreset[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<string | null>(null);
  const [draft, setDraft] = useState<DraftPreset>({ ...EMPTY_DRAFT });
  const [providers, setProviders] = useState<ProviderLite[]>([]);
  const [activeProviderIds, setActiveProviderIds] = useState<Record<WorkflowTool, string>>({
    claude: '',
    codex: '',
    gemini: '',
    opencode: '',
  });
  const [mcpServers, setMcpServers] = useState<MCPServerLite[]>([]);
  const [skillOptions, setSkillOptions] = useState<SkillOptionLite[]>([]);
  const [toolDropdownOpen, setToolDropdownOpen] = useState(false);
  const [mcpDropdownOpen, setMcpDropdownOpen] = useState(false);
  const [skillsDropdownOpen, setSkillsDropdownOpen] = useState(false);
  const [deps, setDeps] = useState<WorkflowDependencyState | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [checkingDeps, setCheckingDeps] = useState(false);
  const [applyingDeps, setApplyingDeps] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);
  const toolDropdownRef = useRef<HTMLDivElement | null>(null);
  const mcpDropdownRef = useRef<HTMLDivElement | null>(null);
  const skillsDropdownRef = useRef<HTMLDivElement | null>(null);

  const filteredProviders = useMemo(
    () => providers.filter((p) => p.tool === draft.tool),
    [providers, draft.tool],
  );
  const selectedMcpNames = useMemo(() => {
    if (!draft.mcp_server_ids.length) return '';
    const byId = new Map(mcpServers.map((server) => [server.id, server.name]));
    return draft.mcp_server_ids.map((id) => byId.get(id) || id).join(', ');
  }, [draft.mcp_server_ids, mcpServers]);
  const groupedSkillOptions = useMemo(() => {
    const recommended = skillOptions.filter((item) => item.group === 'recommended');
    const repository = skillOptions.filter((item) => item.group === 'repository');
    return { recommended, repository };
  }, [skillOptions]);
  const selectedSkillNames = useMemo(() => {
    if (!draft.required_skill_ids.length) return '';
    const byId = new Map(skillOptions.map((skill) => [skill.value, skill.name]));
    return draft.required_skill_ids.map((id) => byId.get(id) || id).join(', ');
  }, [draft.required_skill_ids, skillOptions]);
  const providerNameById = useMemo(
    () => new Map(providers.map((provider) => [provider.id, provider.name])),
    [providers],
  );
  const mcpNameById = useMemo(
    () => new Map(mcpServers.map((server) => [server.id, server.name])),
    [mcpServers],
  );
  const skillNameByKey = useMemo(() => {
    const map = new Map<string, string>();
    skillOptions.forEach((skill) => {
      map.set(skill.value, skill.name);
      skill.aliases.forEach((alias) => {
        if (alias) map.set(alias, skill.name);
      });
    });
    return map;
  }, [skillOptions]);
  const formatSkillList = (items: string[]) =>
    items.map((item) => skillNameByKey.get(item) || item).join(', ') || '-';
  const formatMcpList = (items: string[]) =>
    items.map((item) => mcpNameById.get(item) || item).join(', ') || '-';
  const formatMcpDepsList = (ids: string[], names?: string[]) =>
    (names && names.length > 0 ? names.join(', ') : formatMcpList(ids)) || '-';
  const formatSkillDepsList = (ids: string[], names?: string[]) =>
    (names && names.length > 0 ? names.join(', ') : formatSkillList(ids)) || '-';
  const providerLabelById = (providerId?: string | null) => {
    if (!providerId) return t('workflowPresetNone', '(none)');
    return providerNameById.get(providerId) || providerId;
  };
  const providerLabelForPreset = (preset: WorkflowPreset): string => {
    const rawTool = String(preset.tool || '').toLowerCase();
    const tool = (TOOL_OPTIONS.includes(rawTool as WorkflowTool) ? rawTool : 'claude') as WorkflowTool;
    if (preset.provider_id) {
      return providerLabelById(preset.provider_id);
    }
    const activeId = activeProviderIds[tool];
    return providerLabelById(activeId);
  };

  useEffect(() => {
    if (!toolDropdownOpen) return;
    const onDocClick = (event: MouseEvent) => {
      if (!toolDropdownRef.current) return;
      if (!toolDropdownRef.current.contains(event.target as Node)) {
        setToolDropdownOpen(false);
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [toolDropdownOpen]);

  useEffect(() => {
    if (!mcpDropdownOpen) return;
    const onDocClick = (event: MouseEvent) => {
      if (!mcpDropdownRef.current) return;
      if (!mcpDropdownRef.current.contains(event.target as Node)) {
        setMcpDropdownOpen(false);
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [mcpDropdownOpen]);

  useEffect(() => {
    if (!skillsDropdownOpen) return;
    const onDocClick = (event: MouseEvent) => {
      if (!skillsDropdownRef.current) return;
      if (!skillsDropdownRef.current.contains(event.target as Node)) {
        setSkillsDropdownOpen(false);
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [skillsDropdownOpen]);

  const loadProvidersAndMcp = async () => {
    try {
      const providersResp = (await invoke('service_providers_list')) as ProvidersListResp;
      const providerList = Array.isArray(providersResp?.data?.providers)
        ? providersResp.data.providers
        : [];
      const normalizedProviders: ProviderLite[] = providerList
        .map((p) => ({
          id: String(p?.id || ''),
          name: String(p?.name || ''),
          tool: String(p?.tool || '').toLowerCase(),
        }))
        .filter((p: ProviderLite) => p.id && p.tool);
      setProviders(normalizedProviders);
      const opencodeActive = providersResp?.data?.active_opencode;
      setActiveProviderIds({
        claude: String(providersResp?.data?.active_claude || ''),
        codex: String(providersResp?.data?.active_codex || ''),
        gemini: String(providersResp?.data?.active_gemini || ''),
        opencode: Array.isArray(opencodeActive) ? String(opencodeActive[0] || '') : String(opencodeActive || ''),
      });
    } catch (e) {
      console.error('Failed to load providers for workflow presets', e);
    }

    try {
      const mcpResp = (await invoke('get_mcp_servers')) as MCPStateResp;
      const list = Array.isArray(mcpResp?.servers)
        ? mcpResp.servers
        : [];
      const normalized: MCPServerLite[] = list
        .map((s) => ({ id: String(s?.id || ''), name: String(s?.name || '') }))
        .filter((s: MCPServerLite) => s.id);
      setMcpServers(normalized);
    } catch (e) {
      console.error('Failed to load MCP servers for workflow presets', e);
    }
  };

  const loadSkillsForTool = async (tool: WorkflowTool) => {
    try {
      const [catalogResp, repoResp] = await Promise.all([
        invoke('skills_list_catalog', { model: tool }) as Promise<SkillsCatalogResp>,
        invoke('skills_repo_list') as Promise<SkillsRepoListResp>,
      ]);
      const catalog = Array.isArray(catalogResp?.data) ? catalogResp.data : [];
      const repository = Array.isArray(repoResp?.data) ? repoResp.data : [];
      const recommendedMap = new Map<string, SkillOptionLite>();
      const recommendedSourceRefSet = new Set<string>();
      catalog.forEach((item) => {
        const sourceId = String(item?.source_id || '').trim();
        const skillRef = String(item?.rel_path || item?.id || '').trim();
        const name = String(item?.name || '').trim();
        if (!sourceId || !skillRef || !name) return;
        const sourceRef = `${sourceId}::${skillRef}`;
        if (recommendedMap.has(sourceRef)) return;
        const aliases = [encodeCatalogSkillValue(sourceId, skillRef), sourceRef, skillRef];
        const rawId = String(item?.id || '').trim();
        if (rawId) {
          aliases.push(rawId);
          aliases.push(encodeCatalogSkillValue(sourceId, rawId));
        }
        recommendedMap.set(sourceRef, {
          value: encodeCatalogSkillValue(sourceId, skillRef),
          name,
          group: 'recommended',
          aliases,
        });
        recommendedSourceRefSet.add(sourceRef);
      });

      const repositoryMap = new Map<string, SkillOptionLite>();
      repository.forEach((item) => {
        const repoKey = String(item?.repo_key || '').trim();
        const name = String(item?.name || '').trim();
        if (!repoKey || !name) return;

        const models = Array.isArray(item?.models)
          ? item.models.map((m) => String(m || '').toLowerCase().trim()).filter(Boolean)
          : [];
        // Only keep repository skills available for the currently selected tool.
        if (models.length > 0 && !models.includes(tool)) return;

        const sourceId = String(item?.source_id || '').trim();
        const sourceRelPath = String(item?.source_rel_path || '').trim();
        if (sourceId && sourceRelPath && recommendedSourceRefSet.has(`${sourceId}::${sourceRelPath}`)) {
          return;
        }

        // Deduplicate repository items by source+path first, then fallback to skill id, finally repo key.
        const skillId = String(item?.skill_id || '').trim();
        const dedupeKey = sourceId && sourceRelPath
          ? `src:${sourceId}::${sourceRelPath}`
          : (skillId ? `skill:${skillId}` : `repo:${repoKey}`);
        if (repositoryMap.has(dedupeKey)) return;
        const aliases = [encodeRepoSkillValue(repoKey), repoKey];
        if (skillId) aliases.push(skillId);
        if (sourceId && sourceRelPath) {
          aliases.push(`${sourceId}::${sourceRelPath}`);
          aliases.push(sourceRelPath);
          aliases.push(encodeCatalogSkillValue(sourceId, sourceRelPath));
        }

        repositoryMap.set(dedupeKey, {
          value: encodeRepoSkillValue(repoKey),
          name,
          group: 'repository',
          aliases,
        });
      });

      const recommendedOptions = Array.from(recommendedMap.values());
      const repositoryOptions = Array.from(repositoryMap.values());
      const normalized = [...recommendedOptions, ...repositoryOptions].sort((a, b) => a.name.localeCompare(b.name));
      setSkillOptions(normalized);
    } catch (e) {
      console.error('Failed to load skills catalog for workflow presets', e);
      setSkillOptions([]);
    }
  };

  const loadPresets = async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await workflowsListPresets();
      const list = resp.data || [];
      setPresets(list);
      onChanged?.(list);
      if (selectedPresetId) {
        const found = list.find((item) => item.id === selectedPresetId);
        if (found) {
          setDraft(presetToDraft(found));
        } else {
          setSelectedPresetId(null);
          onSelectPreset?.(null);
          setDraft({ ...EMPTY_DRAFT });
        }
      }
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadProvidersAndMcp();
    void loadPresets();
  }, []);

  useEffect(() => {
    void loadSkillsForTool(draft.tool);
  }, [draft.tool]);

  useEffect(() => {
    if (!selectedPresetId) {
      setDeps(null);
      return;
    }
    const run = async () => {
      setCheckingDeps(true);
      try {
        const resp = await workflowsCheckDependencies(selectedPresetId);
        setDeps(resp.data);
      } catch {
        setDeps(null);
      } finally {
        setCheckingDeps(false);
      }
    };
    void run();
  }, [selectedPresetId]);

  const selectPreset = (preset: WorkflowPreset) => {
    setToolDropdownOpen(false);
    setMcpDropdownOpen(false);
    setSkillsDropdownOpen(false);
    setSelectedPresetId(preset.id);
    onSelectPreset?.(preset.id);
    setDraft(presetToDraft(preset));
    setSuccessMsg(null);
    setError(null);
  };

  const handleNew = () => {
    setToolDropdownOpen(false);
    setMcpDropdownOpen(false);
    setSkillsDropdownOpen(false);
    setSelectedPresetId(null);
    onSelectPreset?.(null);
    setDraft({ ...EMPTY_DRAFT });
    setDeps(null);
    setError(null);
    setSuccessMsg(null);
  };

  const handleSave = async () => {
    if (!draft.name.trim()) {
      setError(t('workflowPresetNameRequired', 'Preset name is required'));
      return;
    }
    setSaving(true);
    setError(null);
    setSuccessMsg(null);
    try {
      const input: WorkflowPresetInput = {
        id: draft.id,
        name: draft.name.trim(),
        tool: draft.tool,
        working_dir: draft.working_dir.trim(),
        provider_id: draft.provider_id.trim() || undefined,
        mcp_server_ids: draft.mcp_server_ids,
        required_skill_ids: draft.required_skill_ids,
        launch_prompt: draft.launch_prompt.trim() || undefined,
        launch_scope: draft.launch_scope,
      };
      const saved = await workflowsUpsertPreset(input);
      setSelectedPresetId(saved.data.id);
      onSelectPreset?.(saved.data.id);
      setDraft(presetToDraft(saved.data));
      setSuccessMsg(t('workflowPresetSaved', 'Preset saved'));
      await loadPresets();
      const depsResp = await workflowsCheckDependencies(saved.data.id);
      setDeps(depsResp.data);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!selectedPresetId) return;
    const confirmed = await confirmDialog(t('workflowPresetDeleteConfirm', 'Delete selected preset?'), {
      okLabel: t('ok'),
      cancelLabel: t('cancel'),
    });
    if (!confirmed) return;
    setSaving(true);
    setError(null);
    setSuccessMsg(null);
    try {
      await workflowsDeletePreset(selectedPresetId);
      handleNew();
      setSuccessMsg(t('workflowPresetDeleted', 'Preset deleted'));
      await loadPresets();
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleApplyDeps = async () => {
    if (!selectedPresetId) return;
    setApplyingDeps(true);
    setError(null);
    setSuccessMsg(null);
    try {
      const resp = await workflowsApplyDependencies(selectedPresetId);
      setDeps(resp.data.dependencies_after);
      setSuccessMsg(
        t('workflowPresetAppliedSummary', {
          defaultValue:
            'Dependencies applied: MCP linked {{linked}}, MCP enabled {{enabled}}, Skills installed {{installed}}',
          linked: resp.data.linked_mcp_count,
          enabled: resp.data.enabled_mcp_switch_count,
          installed: resp.data.installed_skill_count,
        }),
      );
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setApplyingDeps(false);
    }
  };

  const handleSelectWorkingDir = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
      });
      if (selected && typeof selected === 'string') {
        setDraft((prev) => ({ ...prev, working_dir: selected }));
      }
    } catch (e) {
      setError(
        t('selectFileFailed', {
          defaultValue: 'Failed to select file: {{error}}',
          error: String(e),
        }),
      );
    }
  };

  return (
    <div className="bg-card border rounded-xl p-4 shadow-sm space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-base font-semibold">{t('workflowPresets', 'Workflow Presets')}</h3>
          <p className="text-xs text-muted-foreground">
            {t('workflowPresetsDesc', 'Create reusable AI workflow launch templates')}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => void loadPresets()}
            className="px-2.5 py-1.5 text-xs rounded-md border hover:bg-muted transition-colors"
            title={t('workflowRecentRunsRefresh', 'Refresh')}
          >
            <RefreshCw className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={handleNew}
            className="px-2.5 py-1.5 text-xs rounded-md border hover:bg-muted transition-colors flex items-center gap-1"
          >
            <Plus className="w-3.5 h-3.5" />
            {t('workflowPresetNew', 'New')}
          </button>
        </div>
      </div>

      {error && (
        <div className="text-sm text-destructive bg-destructive/10 border border-destructive/20 rounded-md px-3 py-2">
          {error}
        </div>
      )}
      {successMsg && (
        <div className="text-sm text-green-600 bg-green-500/10 border border-green-500/20 rounded-md px-3 py-2 flex items-center gap-2">
          <CheckCircle2 className="w-4 h-4" />
          {successMsg}
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <div className="border rounded-lg divide-y overflow-hidden">
          {loading && (
            <div className="px-3 py-2 text-xs text-muted-foreground flex items-center gap-2">
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
              {t('workflowPresetLoading', 'Loading presets...')}
            </div>
          )}
          {!loading && presets.length === 0 && (
            <div className="px-3 py-3 text-xs text-muted-foreground">
              {t('workflowPresetNoItems', 'No presets yet.')}
            </div>
          )}
          {!loading &&
            presets.map((preset) => (
              <button
                key={preset.id}
                onClick={() => selectPreset(preset)}
                className={`w-full text-left px-3 py-2 transition-colors ${
                  selectedPresetId === preset.id ? 'bg-primary/10' : 'hover:bg-muted/50'
                }`}
              >
                <div className="text-sm font-medium truncate">{preset.name}</div>
                <div className="text-xs text-muted-foreground truncate">
                  {preset.working_dir || t('workflowPresetNoDir', '(no dir)')}
                </div>
                <div className="text-xs text-muted-foreground flex items-center gap-2">
                  <ToolIcon tool={preset.tool} className="w-3.5 h-3.5 shrink-0" />
                  <span>{toolLabel(preset.tool as WorkflowTool, (key, fallback) => t(key, fallback || key))}</span>
                  <span>·</span>
                  <span>{launchScopeLabel((preset.launch_scope || 'shared') as WorkflowLaunchScope, (key, fallback) => t(key, fallback || key))}</span>
                  <span>·</span>
                  <span className="truncate">{providerLabelForPreset(preset)}</span>
                </div>
              </button>
            ))}
        </div>

        <div className="lg:col-span-2 space-y-3">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <label className="space-y-1 md:col-span-2">
              <span className="text-xs font-medium text-muted-foreground">{t('workflowPresetName', 'Preset Name')}</span>
              <input
                value={draft.name}
                onChange={(e) => setDraft((prev) => ({ ...prev, name: e.target.value }))}
                className="w-full h-9 rounded-md border px-2.5 text-sm bg-background"
                placeholder={t('workflowPresetPlaceholderName', 'Feature implementation flow')}
              />
            </label>
            <label className="space-y-1 md:col-span-2">
              <span className="text-xs font-medium text-muted-foreground">
                {t('workflowPresetWorkingDir', 'Working Directory')}
              </span>
              <div className="flex items-center gap-2">
                <input
                  value={draft.working_dir}
                  onChange={(e) => setDraft((prev) => ({ ...prev, working_dir: e.target.value }))}
                  className="flex-1 h-9 rounded-md border px-2.5 text-sm bg-background"
                  placeholder={t('workflowPresetPlaceholderDir', '/path/to/project')}
                />
                <button
                  type="button"
                  onClick={() => void handleSelectWorkingDir()}
                  className="h-9 px-3 rounded-md border text-sm hover:bg-muted transition-colors flex items-center gap-1.5"
                >
                  <FolderOpen className="w-4 h-4" />
                  {t('browse')}
                </button>
              </div>
            </label>
            <div className="space-y-1">
              <span className="text-xs font-medium text-muted-foreground">{t('workflowPresetTool', 'Tool')}</span>
              <div ref={toolDropdownRef} className="relative">
                <button
                  type="button"
                  onClick={() => {
                    setMcpDropdownOpen(false);
                    setSkillsDropdownOpen(false);
                    setToolDropdownOpen((prev) => !prev);
                  }}
                  className="w-full h-9 rounded-md border px-2.5 text-sm bg-background flex items-center justify-between"
                >
                  <span className="inline-flex items-center gap-2 truncate">
                    <ToolIcon tool={draft.tool} className="w-4 h-4 shrink-0" />
                    <span className="truncate">{toolLabel(draft.tool, (key, fallback) => t(key, fallback || key))}</span>
                  </span>
                  <ChevronDown className="w-3.5 h-3.5 text-muted-foreground" />
                </button>
                {toolDropdownOpen && (
                  <div className="absolute z-30 mt-1 w-full rounded-md border bg-popover shadow-md py-1">
                    {TOOL_OPTIONS.map((tool) => (
                      <button
                        key={tool}
                        type="button"
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={() => {
                          setDraft((prev) => ({ ...prev, tool, provider_id: '' }));
                          setToolDropdownOpen(false);
                        }}
                        className={`w-full text-left px-3 py-2 text-sm hover:bg-muted/50 flex items-center gap-2 ${
                          draft.tool === tool ? 'bg-primary/10' : ''
                        }`}
                      >
                        <ToolIcon tool={tool} className="w-4 h-4 shrink-0" />
                        <span>{toolLabel(tool, (key, fallback) => t(key, fallback || key))}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            </div>
            <label className="space-y-1">
              <span className="text-xs font-medium text-muted-foreground">
                {t('workflowPresetProviderOptional', 'Provider (Optional)')}
              </span>
              <select
                value={draft.provider_id}
                onChange={(e) => setDraft((prev) => ({ ...prev, provider_id: e.target.value }))}
                className="w-full h-9 rounded-md border px-2.5 text-sm bg-background"
              >
                <option value="">{t('workflowPresetUseActiveProvider', 'Use active provider for tool')}</option>
                {filteredProviders.map((provider) => (
                  <option key={provider.id} value={provider.id}>
                    {provider.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="space-y-1 md:col-span-2">
              <span className="text-xs font-medium text-muted-foreground">
                {t('workflowPresetLaunchScope', 'Launch Scope')}
              </span>
              <select
                value={draft.launch_scope}
                onChange={(e) =>
                  setDraft((prev) => ({
                    ...prev,
                    launch_scope: (e.target.value === 'strict' ? 'strict' : 'shared') as WorkflowLaunchScope,
                  }))
                }
                className="w-full h-9 rounded-md border px-2.5 text-sm bg-background"
              >
                <option value="shared">{t('workflowPresetLaunchScopeShared', 'Shared (global apply)')}</option>
                <option value="strict">{t('workflowPresetLaunchScopeStrict', 'Strict (session isolated)')}</option>
              </select>
              <div className="text-xs text-muted-foreground rounded-md border bg-muted/30 px-2.5 py-2">
                {draft.launch_scope === 'strict'
                  ? t(
                      'workflowPresetLaunchScopeStrictHint',
                      'Strict: Creates an isolated runtime profile for each run. Only preset MCP/Skills are loaded, and provider env-managed must be enabled.',
                    )
                  : t(
                      'workflowPresetLaunchScopeSharedHint',
                      'Shared: Applies MCP link/switch and Skills install to the global tool environment. Changes can affect other sessions of the same tool.',
                    )}
              </div>
            </label>
            <label className="space-y-1 md:col-span-2">
              <span className="text-xs font-medium text-muted-foreground">
                {t('workflowPresetMcpIds', 'MCP Servers (multi-select)')}
              </span>
              <div ref={mcpDropdownRef} className="relative">
                <button
                  type="button"
                  onClick={() => {
                    setToolDropdownOpen(false);
                    setSkillsDropdownOpen(false);
                    setMcpDropdownOpen((prev) => !prev);
                  }}
                  className="w-full h-10 rounded-md border px-2.5 text-sm bg-background text-left truncate"
                >
                  {selectedMcpNames || t('workflowPresetMcpSelectPlaceholder', 'Select MCP servers')}
                </button>
                {mcpDropdownOpen && (
                  <div className="absolute z-30 mt-1 w-full rounded-md border bg-popover shadow-md max-h-56 overflow-auto py-1">
                    {mcpServers.length === 0 ? (
                      <div className="px-3 py-2 text-xs text-muted-foreground">
                        {t('noMcpServersConfigured', 'No MCP servers configured')}
                      </div>
                    ) : (
                      mcpServers.map((server) => {
                        const checked = draft.mcp_server_ids.includes(server.id);
                        return (
                          <label
                            key={server.id}
                            className="flex items-center gap-2 px-3 py-2 text-sm hover:bg-muted/50 cursor-pointer"
                          >
                            <input
                              type="checkbox"
                              checked={checked}
                              onChange={(e) => {
                                const enabled = e.target.checked;
                                setDraft((prev) => {
                                  const next = new Set(prev.mcp_server_ids);
                                  if (enabled) {
                                    next.add(server.id);
                                  } else {
                                    next.delete(server.id);
                                  }
                                  return { ...prev, mcp_server_ids: Array.from(next) };
                                });
                              }}
                            />
                            <span className="truncate">{server.name}</span>
                          </label>
                        );
                      })
                    )}
                  </div>
                )}
              </div>
            </label>
            <label className="space-y-1 md:col-span-2">
              <span className="text-xs font-medium text-muted-foreground">
                {t('workflowPresetSkills', 'Required Skills (multi-select)')}
              </span>
              <div ref={skillsDropdownRef} className="relative">
                <button
                  type="button"
                  onClick={() => {
                    setToolDropdownOpen(false);
                    setMcpDropdownOpen(false);
                    setSkillsDropdownOpen((prev) => !prev);
                  }}
                  className="w-full h-10 rounded-md border px-2.5 text-sm bg-background text-left truncate"
                >
                  {selectedSkillNames || t('workflowPresetSkillsSelectPlaceholder', 'Select skills')}
                </button>
                {skillsDropdownOpen && (
                  <div className="absolute z-30 mt-1 w-full rounded-md border bg-popover shadow-md max-h-56 overflow-auto py-1">
                    {skillOptions.length === 0 ? (
                      <div className="px-3 py-2 text-xs text-muted-foreground">
                        {t('workflowPresetNoSkillsAvailable', 'No skills available for current tool')}
                      </div>
                    ) : (
                      <>
                        {groupedSkillOptions.recommended.length > 0 && (
                          <div className="px-3 py-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                            {t('workflowPresetSkillsGroupRecommended', 'Recommended')}
                          </div>
                        )}
                        {groupedSkillOptions.recommended.map((skill) => {
                          const checked = draft.required_skill_ids.includes(skill.value);
                          return (
                            <label
                              key={skill.value}
                              className="flex items-center gap-2 px-3 py-2 text-sm hover:bg-muted/50 cursor-pointer"
                            >
                              <input
                                type="checkbox"
                                checked={checked}
                                onChange={(e) => {
                                  const enabled = e.target.checked;
                                  setDraft((prev) => {
                                    const next = new Set(prev.required_skill_ids);
                                    if (enabled) {
                                      next.add(skill.value);
                                    } else {
                                      next.delete(skill.value);
                                    }
                                    return { ...prev, required_skill_ids: Array.from(next) };
                                  });
                                }}
                              />
                              <span className="truncate">{skill.name}</span>
                            </label>
                          );
                        })}
                        {groupedSkillOptions.repository.length > 0 && (
                          <div className="px-3 py-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                            {t('workflowPresetSkillsGroupRepository', 'Repository')}
                          </div>
                        )}
                        {groupedSkillOptions.repository.map((skill) => {
                          const checked = draft.required_skill_ids.includes(skill.value);
                          return (
                            <label
                              key={skill.value}
                              className="flex items-center gap-2 px-3 py-2 text-sm hover:bg-muted/50 cursor-pointer"
                            >
                              <input
                                type="checkbox"
                                checked={checked}
                                onChange={(e) => {
                                  const enabled = e.target.checked;
                                  setDraft((prev) => {
                                    const next = new Set(prev.required_skill_ids);
                                    if (enabled) {
                                      next.add(skill.value);
                                    } else {
                                      next.delete(skill.value);
                                    }
                                    return { ...prev, required_skill_ids: Array.from(next) };
                                  });
                                }}
                              />
                              <span className="truncate">{skill.name}</span>
                            </label>
                          );
                        })}
                      </>
                    )}
                  </div>
                )}
              </div>
            </label>
            <label className="space-y-1 md:col-span-2">
              <span className="text-xs font-medium text-muted-foreground">
                {t('workflowPresetPromptOptional', 'Launch Prompt (Optional)')}
              </span>
              <textarea
                value={draft.launch_prompt}
                onChange={(e) => setDraft((prev) => ({ ...prev, launch_prompt: e.target.value }))}
                className="w-full min-h-[70px] rounded-md border px-2.5 py-2 text-sm bg-background"
                placeholder={t('workflowPresetPlaceholderPrompt', 'Start by scanning backend errors and draft a fix plan.')}
              />
            </label>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => void handleSave()}
              disabled={saving}
              className="px-3 py-2 rounded-md bg-primary text-primary-foreground text-sm font-medium hover:bg-primary/90 disabled:opacity-50 flex items-center gap-2"
            >
              {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
              {t('workflowPresetSave', 'Save Preset')}
            </button>
            <button
              onClick={() => void handleDelete()}
              disabled={!selectedPresetId || saving}
              className="px-3 py-2 rounded-md border text-sm font-medium hover:bg-muted disabled:opacity-50 flex items-center gap-2"
            >
              <Trash2 className="w-4 h-4" />
              {t('workflowPresetDelete', 'Delete')}
            </button>
          </div>

          {selectedPresetId && (
            <div className="rounded-md border p-3 bg-muted/20 space-y-2">
              <div className="flex items-center justify-between">
                <div className="text-sm font-medium">{t('workflowPresetDependencyCheck', 'Dependency Check')}</div>
                <button
                  onClick={() => void handleApplyDeps()}
                  disabled={checkingDeps || applyingDeps}
                  className="px-2.5 py-1.5 rounded-md border text-xs hover:bg-muted disabled:opacity-50 flex items-center gap-1.5"
                >
                  {applyingDeps ? (
                    <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  ) : (
                    <Wand2 className="w-3.5 h-3.5" />
                  )}
                  {t('workflowPresetFixDeps', 'One-click Fix')}
                </button>
              </div>
              {checkingDeps ? (
                <div className="text-xs text-muted-foreground flex items-center gap-2">
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  {t('workflowPresetCheckingDeps', 'Checking dependencies...')}
                </div>
              ) : deps ? (
                <div className="text-xs text-muted-foreground space-y-1">
                  <div>
                    {t('workflowPresetActiveProvider', 'Active Provider')}:{' '}
                    <span className="font-mono">
                      {deps.active_provider_name || providerLabelById(deps.active_provider_id)}
                    </span>
                  </div>
                  <div>
                    {t('workflowPresetMissingMcp', 'Missing MCP')}:{' '}
                    <span className="font-mono">
                      {formatMcpDepsList(deps.missing_mcp_server_ids, deps.missing_mcp_names)}
                    </span>
                  </div>
                  <div>
                    {t('workflowPresetInactiveMcp', 'Inactive MCP Link')}:{' '}
                    <span className="font-mono">
                      {formatMcpDepsList(deps.inactive_mcp_server_ids, deps.inactive_mcp_names)}
                    </span>
                  </div>
                  <div>
                    {t('workflowPresetMissingSkills', 'Missing Skills')}:{' '}
                    <span className="font-mono">
                      {formatSkillDepsList(deps.missing_skill_ids, deps.missing_skill_names)}
                    </span>
                  </div>
                  <div>
                    {t('workflowPresetInstallableSkills', 'Installable Skills')}:{' '}
                    <span className="font-mono">{formatSkillList(deps.installable_skill_ids)}</span>
                  </div>
                  <div>
                    {t('workflowPresetUnresolvedSkills', 'Unresolved Skills')}:{' '}
                    <span className="font-mono">{formatSkillList(deps.unresolved_skill_ids)}</span>
                  </div>
                </div>
              ) : (
                <div className="text-xs text-muted-foreground">
                  {t('workflowPresetNoDepsData', 'No dependency data.')}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
