import { useState } from 'react';
import type {
  JsonObject,
  OpenCodeModelConfigError,
  OpenCodeModelsFormValue,
} from '../opencodeModelConfig';

export function useOpenCodeEditorState() {
  const [rawJson, setRawJson] = useState('');
  const [originalJson, setOriginalJson] = useState('');
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [openCodeModelForm, setOpenCodeModelForm] = useState<OpenCodeModelsFormValue>({ models: [] });
  const [openCodeModelErrors, setOpenCodeModelErrors] = useState<OpenCodeModelConfigError[]>([]);
  const [lastValidOpenCodeSnapshot, setLastValidOpenCodeSnapshot] = useState<JsonObject | null>(null);

  return {
    rawJson,
    setRawJson,
    originalJson,
    setOriginalJson,
    jsonError,
    setJsonError,
    openCodeModelForm,
    setOpenCodeModelForm,
    openCodeModelErrors,
    setOpenCodeModelErrors,
    lastValidOpenCodeSnapshot,
    setLastValidOpenCodeSnapshot,
  };
}

export type OpenCodeEditorStateBindings = ReturnType<typeof useOpenCodeEditorState>;
