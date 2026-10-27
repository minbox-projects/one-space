import { useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';
import type { TFunction } from 'i18next';
import { invoke } from '@tauri-apps/api/core';
import { v4 as uuidv4 } from 'uuid';
import type { ApiResp } from '@/lib/serviceProviders';
import {
  normalizeClaudePresetTemplate,
  type ServiceProviderPresetRecord,
  type ServiceProviderPresetsState,
} from '../providerPresets';
import { buildPresetClaudeMappings } from '../helpers/claudeProvider';
import { errorToDisplayMessage, type PresetDialogDraft } from '../helpers/providerUtils';

interface UseProviderPresetsParams {
  t: TFunction;
  setMessage: Dispatch<SetStateAction<{ type: string; text: string }>>;
}

export function useProviderPresets({ t, setMessage }: UseProviderPresetsParams) {
  const [providerPresets, setProviderPresets] = useState<ServiceProviderPresetRecord[]>([]);
  const [presetPickerOpen, setPresetPickerOpen] = useState(false);
  const [presetManagerOpen, setPresetManagerOpen] = useState(false);
  const [editingPresetId, setEditingPresetId] = useState<string | null>(null);
  const [presetDraft, setPresetDraft] = useState<PresetDialogDraft>({
    id: '',
    name: '',
    description: '',
    icon: '',
    openai_base_url: '',
    anthropic_base_url: '',
    claude_default_model: '',
    claude_reasoning_effort: '',
    claude_model_mappings: buildPresetClaudeMappings(),
  });

  const loadProviderPresets = async () => {
    const res = await invoke<ApiResp<ServiceProviderPresetsState>>('service_provider_presets_list');
    setProviderPresets(res.data.presets || []);
  };

  const openPresetEditor = (preset?: ServiceProviderPresetRecord) => {
    const claudeTemplate = normalizeClaudePresetTemplate(preset?.template);
    setEditingPresetId(preset?.id || null);
    setPresetDraft({
      id: preset?.id || '',
      name: preset?.name || '',
      description: preset?.description || '',
      icon: preset?.icon || '',
      openai_base_url: preset?.endpoints?.openai_base_url || '',
      anthropic_base_url: preset?.endpoints?.anthropic_base_url || '',
      claude_default_model: claudeTemplate.claude_default_model || '',
      claude_reasoning_effort: claudeTemplate.claude_reasoning_effort || '',
      claude_model_mappings: buildPresetClaudeMappings(claudeTemplate.claude_model_mappings),
    });
    setPresetManagerOpen(true);
  };

  const savePresetDraft = async () => {
    const now = Math.floor(Date.now() / 1000);
    const template = normalizeClaudePresetTemplate({
      claude_default_model: presetDraft.claude_default_model,
      claude_reasoning_effort: presetDraft.claude_reasoning_effort,
      claude_model_mappings: presetDraft.claude_model_mappings,
    });
    const payload: ServiceProviderPresetRecord = {
      id: presetDraft.id || `preset-${uuidv4()}`,
      name: presetDraft.name.trim(),
      description: presetDraft.description.trim() || undefined,
      icon: presetDraft.icon.trim() || undefined,
      endpoints: {
        openai_base_url: presetDraft.openai_base_url.trim() || undefined,
        anthropic_base_url: presetDraft.anthropic_base_url.trim() || undefined,
      },
      template,
      created_at: now,
      updated_at: now,
    };
    if (!payload.name) {
      setMessage({ type: 'error', text: t('providerPresetNameRequired', 'Preset name is required') });
      return;
    }
    try {
      await invoke<ApiResp<ServiceProviderPresetRecord>>('service_provider_presets_upsert', { preset: payload });
      await loadProviderPresets();
      setPresetManagerOpen(false);
      setEditingPresetId(null);
    } catch (error) {
      setMessage({ type: 'error', text: errorToDisplayMessage(error) });
    }
  };

  const deleteProviderPreset = async (presetId: string) => {
    try {
      await invoke<ApiResp<{ deleted: boolean }>>('service_provider_presets_delete', { presetId });
      await loadProviderPresets();
    } catch (error) {
      setMessage({ type: 'error', text: errorToDisplayMessage(error) });
    }
  };

  return {
    providerPresets,
    setProviderPresets,
    presetPickerOpen,
    setPresetPickerOpen,
    presetManagerOpen,
    setPresetManagerOpen,
    editingPresetId,
    setEditingPresetId,
    presetDraft,
    setPresetDraft,
    loadProviderPresets,
    openPresetEditor,
    savePresetDraft,
    deleteProviderPreset,
  };
}

export type ProviderPresetsBindings = ReturnType<typeof useProviderPresets>;
