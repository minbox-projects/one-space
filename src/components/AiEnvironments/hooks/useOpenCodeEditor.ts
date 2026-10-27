import type { Dispatch, MutableRefObject, SetStateAction } from 'react';
import type { TFunction } from 'i18next';
import type { AiProvider } from '@/lib/serviceProviders';
import {
  mergeOpenCodeModelConfig,
  parseOpenCodeModelConfig,
  type JsonObject,
  type OpenCodeModelConfigError,
  type OpenCodeModelsFormValue,
} from '../opencodeModelConfig';

interface UseOpenCodeEditorParams {
  t: TFunction;
  setRawJson: Dispatch<SetStateAction<string>>;
  setOriginalJson: Dispatch<SetStateAction<string>>;
  setJsonError: Dispatch<SetStateAction<string | null>>;
  setOpenCodeModelForm: Dispatch<SetStateAction<OpenCodeModelsFormValue>>;
  setOpenCodeModelErrors: Dispatch<SetStateAction<OpenCodeModelConfigError[]>>;
  lastValidOpenCodeSnapshot: JsonObject | null;
  setLastValidOpenCodeSnapshot: Dispatch<SetStateAction<JsonObject | null>>;
  isRollbackMode: boolean;
  setIsRollbackMode: Dispatch<SetStateAction<boolean>>;
  setDetailProvider: Dispatch<SetStateAction<any | null>>;
  openCodeDetailRequestIdRef: MutableRefObject<number>;
}

export function useOpenCodeEditor({
  t,
  setRawJson,
  setOriginalJson,
  setJsonError,
  setOpenCodeModelForm,
  setOpenCodeModelErrors,
  lastValidOpenCodeSnapshot,
  setLastValidOpenCodeSnapshot,
  isRollbackMode,
  setIsRollbackMode,
  setDetailProvider,
  openCodeDetailRequestIdRef,
}: UseOpenCodeEditorParams) {
  const openCodeErrorMessage = (errors: OpenCodeModelConfigError[]) =>
    errors[0]?.message || t('invalidJson', 'Invalid OpenCode JSON');

  const syncDetailFromOpenCodeSnapshot = (snapshot: JsonObject) => {
    const options = snapshot.options && typeof snapshot.options === 'object' && !Array.isArray(snapshot.options)
      ? snapshot.options as Record<string, unknown>
      : {};
    setDetailProvider((previous: AiProvider | null) => previous ? {
      ...previous,
      name: typeof snapshot.name === 'string' ? snapshot.name : previous.name,
      api_key: typeof options.apiKey === 'string' ? options.apiKey : '',
      base_url: typeof options.baseURL === 'string' ? options.baseURL : '',
    } : previous);
  };

  const initializeOpenCodeEditor = (json: string, updateOriginal = false) => {
    setRawJson(json);
    if (updateOriginal) setOriginalJson(json);
    const parsed = parseOpenCodeModelConfig(json);
    if (!parsed.ok) {
      setLastValidOpenCodeSnapshot(null);
      setOpenCodeModelForm({ models: [] });
      setOpenCodeModelErrors([]);
      setJsonError(openCodeErrorMessage(parsed.errors));
      return false;
    }
    setLastValidOpenCodeSnapshot(parsed.snapshot);
    setOpenCodeModelForm(parsed.form);
    setOpenCodeModelErrors([]);
    setJsonError(null);
    return true;
  };

  const handleOpenCodeJsonChange = (value: string) => {
    openCodeDetailRequestIdRef.current += 1;
    setRawJson(value);
    if (isRollbackMode) setIsRollbackMode(false);
    const parsed = parseOpenCodeModelConfig(value);
    if (!parsed.ok) {
      setJsonError(openCodeErrorMessage(parsed.errors));
      return;
    }
    setLastValidOpenCodeSnapshot(parsed.snapshot);
    setOpenCodeModelForm(parsed.form);
    setOpenCodeModelErrors([]);
    setJsonError(null);
    syncDetailFromOpenCodeSnapshot(parsed.snapshot);
  };

  const handleOpenCodeModelFormChange = (form: OpenCodeModelsFormValue) => {
    openCodeDetailRequestIdRef.current += 1;
    setOpenCodeModelForm(form);
    if (!lastValidOpenCodeSnapshot) return;
    const merged = mergeOpenCodeModelConfig(lastValidOpenCodeSnapshot, form);
    if (!merged.ok) {
      setOpenCodeModelErrors(merged.errors);
      return;
    }
    const reparsed = parseOpenCodeModelConfig(merged.json);
    if (!reparsed.ok) {
      setOpenCodeModelErrors(reparsed.errors);
      return;
    }
    setRawJson(merged.json);
    setLastValidOpenCodeSnapshot(merged.snapshot);
    setOpenCodeModelForm(reparsed.form);
    setOpenCodeModelErrors([]);
    setJsonError(null);
  };

  return {
    openCodeErrorMessage,
    initializeOpenCodeEditor,
    handleOpenCodeJsonChange,
    handleOpenCodeModelFormChange,
  };
}

export type OpenCodeEditorBindings = ReturnType<typeof useOpenCodeEditor>;
