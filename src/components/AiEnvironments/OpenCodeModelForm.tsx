import { useEffect, useState } from 'react';
import { ChevronDown, ChevronRight, Plus, Trash2 } from 'lucide-react';
import {
  OPEN_CODE_COMMON_MODEL_OPTIONS,
  type OpenCodeModelConfigError,
  type OpenCodeModelFormValue,
  type OpenCodeModelsFormValue,
  type OpenCodeOptionRow,
  type OpenCodeOptionValueType,
  type OpenCodeVariantFormValue,
} from './opencodeModelConfig';

type Translate = (key: string, fallback: string, options?: Record<string, unknown>) => string;

interface OpenCodeModelFormProps {
  value: OpenCodeModelsFormValue;
  errors?: OpenCodeModelConfigError[];
  frozen?: boolean;
  frozenReason?: string | null;
  saving?: boolean;
  onChange: (value: OpenCodeModelsFormValue) => void;
  t?: Translate;
}

let rowSequence = 0;

function nextRowId(prefix: string) {
  rowSequence += 1;
  return `${prefix}-${rowSequence}`;
}

function modelUiFingerprint(model: OpenCodeModelFormValue) {
  return JSON.stringify({
    id: model.id,
    name: model.name,
    cost: model.cost,
    limit: model.limit,
    options: model.options.map(({ key, value, valueType, custom }) => ({ key, value, valueType, custom })),
    variants: model.variants.map(({ name, options }) => ({
      name,
      options: options.map(({ key, value, valueType, custom }) => ({ key, value, valueType, custom })),
    })),
  });
}

type ModelUiEntry = { id: string; model: OpenCodeModelFormValue; fingerprint: string };

function reconcileModelUiEntries(previous: ModelUiEntry[], models: OpenCodeModelFormValue[]) {
  const availableEntries = [...previous];
  return models.map((model) => {
    const fingerprint = modelUiFingerprint(model);
    let entryIndex = availableEntries.findIndex((entry) => entry.model === model);
    if (entryIndex < 0) {
      entryIndex = availableEntries.findIndex((entry) => entry.fingerprint === fingerprint);
    }
    const id = entryIndex < 0 ? nextRowId('model') : availableEntries.splice(entryIndex, 1)[0].id;
    return { id, model, fingerprint };
  });
}

function emptyOption(prefix: string): OpenCodeOptionRow {
  return { id: nextRowId(prefix), key: '', value: '', valueType: 'json', custom: true };
}

function emptyVariant(): OpenCodeVariantFormValue {
  const id = nextRowId('variant');
  return { id, name: '', options: [] };
}

function emptyModel(): OpenCodeModelFormValue {
  return {
    id: '',
    name: '',
    cost: { enabled: false, input: '', output: '', cacheRead: '', cacheWrite: '' },
    limit: { enabled: false, context: '', output: '' },
    options: [],
    variants: [],
  };
}

function FieldError({ errors }: { errors: OpenCodeModelConfigError[] }) {
  if (errors.length === 0) return null;
  return <p className="mt-1 text-xs text-destructive">{errors[0].message}</p>;
}

function OptionRows({
  rows,
  errors,
  disabled,
  showAddButton = true,
  onChange,
  t,
}: {
  rows: OpenCodeOptionRow[];
  errors: OpenCodeModelConfigError[];
  disabled: boolean;
  showAddButton?: boolean;
  onChange: (rows: OpenCodeOptionRow[]) => void;
  t?: Translate;
}) {
  const updateRow = (index: number, changes: Partial<OpenCodeOptionRow>) => {
    onChange(rows.map((row, rowIndex) => rowIndex === index ? { ...row, ...changes } : row));
  };

  return (
    <div className="space-y-2">
      {rows.map((row, index) => {
        const rowErrors = errors.filter((item) => item.optionIndex === index);
        return (
          <div key={row.id} className="grid gap-2 sm:grid-cols-[minmax(130px,0.8fr)_110px_minmax(140px,1fr)_32px]">
            <div>
              <input
                aria-label={t?.('optionKey', 'Option key') || 'Option key'}
                disabled={disabled}
                list="opencode-common-model-options"
                placeholder={t?.('optionKey', 'Option key') || 'Option key'}
                value={row.key}
                onChange={(event) => {
                  const common = OPEN_CODE_COMMON_MODEL_OPTIONS.find((item) => item.key === event.target.value);
                  updateRow(index, {
                    key: event.target.value,
                    custom: !common,
                    valueType: common?.valueType || (row.custom ? row.valueType : 'json'),
                  });
                }}
              />
              <FieldError errors={rowErrors.filter((item) => item.path.endsWith('.key'))} />
            </div>
            <select
              aria-label={t?.('optionType', 'Value type') || 'Value type'}
              disabled={disabled || !row.custom}
              value={row.valueType}
              onChange={(event) => updateRow(index, { valueType: event.target.value as OpenCodeOptionValueType })}
            >
              <option value="string">string</option>
              <option value="number">number</option>
              <option value="boolean">boolean</option>
              <option value="json">JSON</option>
            </select>
            <div>
              {row.valueType === 'boolean' ? (
                <select
                  aria-label={t?.('optionValue', 'Option value') || 'Option value'}
                  disabled={disabled}
                  value={row.value}
                  onChange={(event) => updateRow(index, { value: event.target.value })}
                >
                  <option value="">-</option>
                  <option value="true">true</option>
                  <option value="false">false</option>
                </select>
              ) : (
                <input
                  aria-label={t?.('optionValue', 'Option value') || 'Option value'}
                  disabled={disabled}
                  inputMode={row.valueType === 'number' ? 'decimal' : undefined}
                  placeholder={row.valueType === 'json' ? 'JSON value' : row.valueType}
                  value={row.value}
                  onChange={(event) => updateRow(index, { value: event.target.value })}
                />
              )}
              <FieldError errors={rowErrors.filter((item) => item.path.endsWith('.value'))} />
            </div>
            <button
              type="button"
              aria-label={t?.('removeOption', 'Remove option') || 'Remove option'}
              title={t?.('removeOption', 'Remove option') || 'Remove option'}
              disabled={disabled}
              className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-destructive/30 text-destructive hover:bg-destructive/10 disabled:opacity-50"
              onClick={() => onChange(rows.filter((_, rowIndex) => rowIndex !== index))}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          </div>
        );
      })}
      {showAddButton ? (
        <button
          type="button"
          className="acc-btn"
          disabled={disabled}
          onClick={() => onChange([...rows, emptyOption('option')])}
        >
          <Plus className="h-3.5 w-3.5" />
          {t?.('addOption', 'Add option') || 'Add option'}
        </button>
      ) : null}
      <datalist id="opencode-common-model-options">
        {OPEN_CODE_COMMON_MODEL_OPTIONS.map((option) => <option key={option.key} value={option.key} />)}
      </datalist>
    </div>
  );
}

export function OpenCodeModelForm({
  value,
  errors = [],
  frozen = false,
  frozenReason,
  saving = false,
  onChange,
  t,
}: OpenCodeModelFormProps) {
  const disabled = frozen || saving;
  const [modelIdentity, setModelIdentity] = useState(() => ({
    models: value.models,
    entries: reconcileModelUiEntries([], value.models),
  }));
  let currentModelIdentity = modelIdentity;
  if (currentModelIdentity.models !== value.models) {
    currentModelIdentity = {
      models: value.models,
      entries: reconcileModelUiEntries(currentModelIdentity.entries, value.models),
    };
    setModelIdentity(currentModelIdentity);
  }
  const modelIds = currentModelIdentity.entries.map((entry) => entry.id);
  const [expandedSections] = useState(() => new Map<string, { options?: boolean; variants?: boolean }>());
  const [, setExpansionVersion] = useState(0);

  useEffect(() => {
    const currentKeys = new Set(modelIds);
    for (const key of expandedSections.keys()) {
      if (!currentKeys.has(key)) expandedSections.delete(key);
    }
  }, [expandedSections, modelIds]);

  const emitModels = (models: OpenCodeModelFormValue[], ids: string[]) => {
    setModelIdentity({
      models,
      entries: models.map((model, index) => ({ id: ids[index], model, fingerprint: modelUiFingerprint(model) })),
    });
    onChange({ models });
  };

  const updateModel = (index: number, changes: Partial<OpenCodeModelFormValue>) => {
    const currentModel = value.models[index];
    const updatedModel = { ...currentModel, ...changes };
    emitModels(
      value.models.map((model, modelIndex) => modelIndex === index ? updatedModel : model),
      modelIds,
    );
  };

  return (
    <div className="space-y-4">
      {frozenReason ? (
        <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {frozenReason}
        </div>
      ) : null}
      {value.models.map((model, modelIndex) => {
        const modelUiId = modelIds[modelIndex];
        const modelErrors = errors.filter((item) => item.modelIndex === modelIndex);
        const expansionKey = modelUiId;
        const optionsExpanded = expandedSections.get(expansionKey)?.options === true;
        const variantsExpanded = expandedSections.get(expansionKey)?.variants === true;
        const setSectionExpanded = (section: 'options' | 'variants', expanded: boolean) => {
          expandedSections.set(expansionKey, { ...expandedSections.get(expansionKey), [section]: expanded });
          setExpansionVersion((current) => current + 1);
        };
        return (
          <div key={modelUiId} className="rounded-md border p-3">
            <div className="mb-3 flex items-center justify-between gap-3">
              <h6 className="min-w-0 truncate text-sm font-semibold">
                {model.id || t?.('newModel', 'New model') || 'New model'}
              </h6>
              <button
                type="button"
                aria-label={t?.('removeModel', 'Remove model') || 'Remove model'}
                title={t?.('removeModel', 'Remove model') || 'Remove model'}
                disabled={disabled}
                className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-destructive/30 text-destructive hover:bg-destructive/10 disabled:opacity-50"
                onClick={() => emitModels(
                  value.models.filter((_, index) => index !== modelIndex),
                  modelIds.filter((_, index) => index !== modelIndex),
                )}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </div>

            <div className="grid gap-3 md:grid-cols-2">
              <div className="field">
                <label className="required">{t?.('modelId', 'Model ID') || 'Model ID'}</label>
                <input
                  aria-label={t?.('modelId', 'Model ID') || 'Model ID'}
                  disabled={disabled}
                  value={model.id}
                  onChange={(event) => updateModel(modelIndex, { id: event.target.value })}
                />
                <FieldError errors={modelErrors.filter((item) => item.path.endsWith('.id'))} />
              </div>
              <div className="field">
                <label>{t?.('modelName', 'Model name') || 'Model name'}</label>
                <input
                  aria-label={t?.('modelName', 'Model name') || 'Model name'}
                  disabled={disabled}
                  value={model.name}
                  onChange={(event) => updateModel(modelIndex, { name: event.target.value })}
                />
              </div>
            </div>

            <div className="mt-3 grid gap-3 lg:grid-cols-2">
              <div className="space-y-2 rounded-md border p-3">
                <label className="flex items-center gap-2 text-sm font-medium">
                  <input
                    type="checkbox"
                    disabled={disabled}
                    checked={model.cost.enabled}
                    onChange={(event) => updateModel(modelIndex, { cost: { ...model.cost, enabled: event.target.checked } })}
                  />
                  {t?.('modelCost', 'Cost per 1M tokens') || 'Cost per 1M tokens'}
                </label>
                {model.cost.enabled ? (
                  <div className="grid gap-2 sm:grid-cols-2">
                    {([
                      ['input', 'Input'],
                      ['output', 'Output'],
                      ['cacheRead', 'Cache read'],
                      ['cacheWrite', 'Cache write'],
                    ] as const).map(([key, label]) => (
                      <div className="field" key={key}>
                        <label className={key === 'input' || key === 'output' ? 'required' : ''}>{label}</label>
                        <input
                          aria-label={`${t?.('modelCost', 'Cost per 1M tokens') || 'Cost per 1M tokens'}: ${label}`}
                          disabled={disabled}
                          inputMode="decimal"
                          value={model.cost[key]}
                          onChange={(event) => updateModel(modelIndex, { cost: { ...model.cost, [key]: event.target.value } })}
                        />
                        <FieldError errors={modelErrors.filter((item) => item.path.endsWith(`.cost.${key}`))} />
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>

              <div className="space-y-2 rounded-md border p-3">
                <label className="flex items-center gap-2 text-sm font-medium">
                  <input
                    type="checkbox"
                    disabled={disabled}
                    checked={model.limit.enabled}
                    onChange={(event) => updateModel(modelIndex, { limit: { ...model.limit, enabled: event.target.checked } })}
                  />
                  {t?.('modelLimits', 'Limits') || 'Limits'}
                </label>
                {model.limit.enabled ? (
                  <div className="grid gap-2 sm:grid-cols-2">
                    {([['context', 'Context'], ['output', 'Output']] as const).map(([key, label]) => (
                      <div className="field" key={key}>
                        <label className="required">{label}</label>
                        <input
                          aria-label={`${t?.('modelLimits', 'Limits') || 'Limits'}: ${label}`}
                          disabled={disabled}
                          inputMode="numeric"
                          value={model.limit[key]}
                          onChange={(event) => updateModel(modelIndex, { limit: { ...model.limit, [key]: event.target.value } })}
                        />
                        <FieldError errors={modelErrors.filter((item) => item.path.endsWith(`.limit.${key}`))} />
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>
            </div>

            <div className="mt-3 border-t pt-2">
              <div className="flex min-h-8 items-center justify-between gap-2">
                <button
                  type="button"
                  aria-expanded={optionsExpanded}
                  aria-label={t?.('toggleModelOptions', 'Toggle model options') || 'Toggle model options'}
                  className="flex min-w-0 flex-1 items-center gap-2 text-left text-sm font-medium"
                  onClick={() => setSectionExpanded('options', !optionsExpanded)}
                >
                  {optionsExpanded ? <ChevronDown className="h-4 w-4 shrink-0" /> : <ChevronRight className="h-4 w-4 shrink-0" />}
                  <span>{t?.('modelOptions', 'Model options') || 'Model options'} ({model.options.length})</span>
                </button>
                <button
                  type="button"
                  aria-label={t?.('addOption', 'Add option') || 'Add option'}
                  title={t?.('addOption', 'Add option') || 'Add option'}
                  disabled={disabled}
                   className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md border hover:bg-muted disabled:opacity-50"
                   onClick={() => {
                     setSectionExpanded('options', true);
                     updateModel(modelIndex, { options: [...model.options, emptyOption('option')] });
                   }}
                >
                  <Plus className="h-3.5 w-3.5" />
                </button>
              </div>
              {optionsExpanded ? (
                <div className="pt-2">
                  <OptionRows
                    rows={model.options}
                    errors={modelErrors.filter((item) => item.variantIndex === undefined)}
                    disabled={disabled}
                    showAddButton={false}
                    onChange={(options) => updateModel(modelIndex, { options })}
                    t={t}
                  />
                </div>
              ) : null}
            </div>

            <div className="mt-2 border-t pt-2">
              <div className="flex min-h-8 items-center justify-between gap-2">
                <button
                  type="button"
                  aria-expanded={variantsExpanded}
                  aria-label={t?.('toggleModelVariants', 'Toggle model variants') || 'Toggle model variants'}
                  className="flex min-w-0 flex-1 items-center gap-2 text-left text-sm font-medium"
                  onClick={() => setSectionExpanded('variants', !variantsExpanded)}
                >
                  {variantsExpanded ? <ChevronDown className="h-4 w-4 shrink-0" /> : <ChevronRight className="h-4 w-4 shrink-0" />}
                  <span>{t?.('modelVariants', 'Variants') || 'Variants'} ({model.variants.length})</span>
                </button>
                <button
                  type="button"
                  aria-label={t?.('addVariant', 'Add variant') || 'Add variant'}
                  title={t?.('addVariant', 'Add variant') || 'Add variant'}
                  disabled={disabled}
                   className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md border hover:bg-muted disabled:opacity-50"
                   onClick={() => {
                     setSectionExpanded('variants', true);
                     updateModel(modelIndex, { variants: [...model.variants, emptyVariant()] });
                   }}
                >
                  <Plus className="h-3.5 w-3.5" />
                </button>
              </div>
              {variantsExpanded ? <div className="space-y-2 pt-2">{model.variants.map((variant, variantIndex) => {
                const variantErrors = modelErrors.filter((item) => item.variantIndex === variantIndex);
                return (
                  <div key={variant.id} className="rounded-md border p-2">
                    <div className="mb-2 grid gap-2 sm:grid-cols-[minmax(0,1fr)_32px]">
                      <div>
                        <input
                          aria-label={t?.('variantName', 'Variant name') || 'Variant name'}
                          disabled={disabled}
                          placeholder={t?.('variantName', 'Variant name') || 'Variant name'}
                          value={variant.name}
                          onChange={(event) => updateModel(modelIndex, {
                            variants: model.variants.map((item, index) => index === variantIndex ? { ...item, name: event.target.value } : item),
                          })}
                        />
                        <FieldError errors={variantErrors.filter((item) => item.path.endsWith('.name'))} />
                      </div>
                      <button
                        type="button"
                        aria-label={t?.('removeVariant', 'Remove variant') || 'Remove variant'}
                        title={t?.('removeVariant', 'Remove variant') || 'Remove variant'}
                        disabled={disabled}
                        className="inline-flex h-8 w-8 items-center justify-center rounded-md border border-destructive/30 text-destructive hover:bg-destructive/10 disabled:opacity-50"
                        onClick={() => updateModel(modelIndex, { variants: model.variants.filter((_, index) => index !== variantIndex) })}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                    <OptionRows
                      rows={variant.options}
                      errors={variantErrors}
                      disabled={disabled}
                      onChange={(options) => updateModel(modelIndex, {
                        variants: model.variants.map((item, index) => index === variantIndex ? { ...item, options } : item),
                      })}
                      t={t}
                    />
                  </div>
                );
              })}</div> : null}
            </div>
          </div>
        );
      })}

      <button
        type="button"
        className="acc-panel-btn"
        disabled={disabled}
        onClick={() => emitModels(
          [...value.models, emptyModel()],
          [...modelIds, nextRowId('model')],
        )}
      >
        <Plus className="h-4 w-4" />
        {t?.('addModel', 'Add model') || 'Add model'}
      </button>
    </div>
  );
}
