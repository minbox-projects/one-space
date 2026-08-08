export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export type OpenCodeOptionValueType = "string" | "number" | "boolean" | "json";

export type OpenCodeOptionRow = {
  id: string;
  key: string;
  value: string;
  valueType: OpenCodeOptionValueType;
  custom: boolean;
};

export type OpenCodeVariantFormValue = {
  id: string;
  name: string;
  options: OpenCodeOptionRow[];
};

export type OpenCodeCostFormValue = {
  enabled: boolean;
  input: string;
  output: string;
  cacheRead: string;
  cacheWrite: string;
};

export type OpenCodeLimitFormValue = {
  enabled: boolean;
  context: string;
  output: string;
};

export type OpenCodeModelFormValue = {
  id: string;
  sourceId?: string;
  name: string;
  cost: OpenCodeCostFormValue;
  limit: OpenCodeLimitFormValue;
  options: OpenCodeOptionRow[];
  variants: OpenCodeVariantFormValue[];
};

export type OpenCodeModelsFormValue = {
  models: OpenCodeModelFormValue[];
};

export type OpenCodeModelConfigError = {
  code: string;
  path: string;
  message: string;
  modelIndex?: number;
  optionIndex?: number;
  variantIndex?: number;
};

export type OpenCodeModelConfigParseResult =
  | { ok: true; snapshot: JsonObject; form: OpenCodeModelsFormValue }
  | { ok: false; errors: OpenCodeModelConfigError[] };

export type OpenCodeModelFormValidationResult =
  | { ok: true }
  | { ok: false; errors: OpenCodeModelConfigError[] };

export type OpenCodeModelConfigMergeResult =
  | { ok: true; snapshot: JsonObject; json: string }
  | { ok: false; errors: OpenCodeModelConfigError[] };

export type OpenCodeCommonOption = {
  key: string;
  valueType: Exclude<OpenCodeOptionValueType, "json">;
};

export const OPEN_CODE_COMMON_MODEL_OPTIONS: readonly OpenCodeCommonOption[] = [
  { key: "temperature", valueType: "number" },
  { key: "topP", valueType: "number" },
  { key: "maxTokens", valueType: "number" },
  { key: "reasoningEffort", valueType: "string" },
  { key: "thinking", valueType: "boolean" },
] as const;

const COMMON_OPTION_TYPES = new Map(
  OPEN_CODE_COMMON_MODEL_OPTIONS.map(({ key, valueType }) => [key, valueType]),
);

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function cloneJson<T extends JsonValue>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function error(
  code: string,
  path: string,
  message: string,
  location: Partial<Pick<OpenCodeModelConfigError, "modelIndex" | "optionIndex" | "variantIndex">> = {},
): OpenCodeModelConfigError {
  return { code, path, message, ...location };
}

function optionValueType(key: string, value: unknown): OpenCodeOptionValueType {
  const commonType = COMMON_OPTION_TYPES.get(key);
  return commonType && typeof value === commonType ? commonType : "json";
}

function optionValueText(value: unknown, valueType: OpenCodeOptionValueType) {
  if (valueType === "string") return value as string;
  if (valueType === "number" || valueType === "boolean") return String(value);
  return JSON.stringify(value);
}

function optionsToRows(value: Record<string, unknown>, idPrefix: string): OpenCodeOptionRow[] {
  return Object.entries(value).map(([key, optionValue], index) => {
    const valueType = optionValueType(key, optionValue);
    return {
      id: `${idPrefix}-option-${index}`,
      key,
      value: optionValueText(optionValue, valueType),
      valueType,
      custom: !COMMON_OPTION_TYPES.has(key),
    };
  });
}

function optionalNumber(value: unknown) {
  return typeof value === "number" ? String(value) : "";
}

function structuralError(path: string, expected: string) {
  return error("invalid_structure", path, `${path} must be ${expected}`);
}

function parseModel(modelId: string, value: unknown, modelIndex: number) {
  const path = `models.${modelId}`;
  if (!isObject(value)) {
    return { errors: [structuralError(path, "an object")] };
  }

  const errors: OpenCodeModelConfigError[] = [];
  if (value.name !== undefined && typeof value.name !== "string") {
    errors.push(structuralError(`${path}.name`, "a string"));
  }
  for (const key of ["cost", "limit", "options", "variants"] as const) {
    if (value[key] !== undefined && !isObject(value[key])) {
      errors.push(structuralError(`${path}.${key}`, "an object"));
    }
  }
  if (errors.length > 0) return { errors };

  const cost = isObject(value.cost) ? value.cost : undefined;
  const limit = isObject(value.limit) ? value.limit : undefined;
  const options = isObject(value.options) ? value.options : {};
  const variants = isObject(value.variants) ? value.variants : {};

  for (const key of ["input", "output", "cache_read", "cache_write"] as const) {
    if (cost?.[key] !== undefined && typeof cost[key] !== "number") {
      errors.push(structuralError(`${path}.cost.${key}`, "a number"));
    }
  }
  for (const key of ["context", "output"] as const) {
    if (limit?.[key] !== undefined && typeof limit[key] !== "number") {
      errors.push(structuralError(`${path}.limit.${key}`, "a number"));
    }
  }

  const variantRows: OpenCodeVariantFormValue[] = [];
  Object.entries(variants).forEach(([name, variant], variantIndex) => {
    if (!isObject(variant)) {
      errors.push(structuralError(`${path}.variants.${name}`, "an object"));
      return;
    }
    variantRows.push({
      id: `model-${modelIndex}-variant-${variantIndex}`,
      name,
      options: optionsToRows(variant, `model-${modelIndex}-variant-${variantIndex}`),
    });
  });

  if (errors.length > 0) return { errors };

  const model: OpenCodeModelFormValue = {
    id: modelId,
    sourceId: modelId,
    name: typeof value.name === "string" ? value.name : "",
    cost: {
      enabled: cost !== undefined,
      input: optionalNumber(cost?.input),
      output: optionalNumber(cost?.output),
      cacheRead: optionalNumber(cost?.cache_read),
      cacheWrite: optionalNumber(cost?.cache_write),
    },
    limit: {
      enabled: limit !== undefined,
      context: optionalNumber(limit?.context),
      output: optionalNumber(limit?.output),
    },
    options: optionsToRows(options, `model-${modelIndex}`),
    variants: variantRows,
  };
  return { model, errors: [] };
}

export function parseOpenCodeModelConfig(source: string): OpenCodeModelConfigParseResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(source);
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : "Invalid JSON";
    return { ok: false, errors: [error("invalid_json", "$", message)] };
  }

  if (!isObject(parsed)) {
    return { ok: false, errors: [structuralError("$", "an object")] };
  }
  if (parsed.models !== undefined && !isObject(parsed.models)) {
    return { ok: false, errors: [structuralError("models", "an object")] };
  }

  const models: OpenCodeModelFormValue[] = [];
  const errors: OpenCodeModelConfigError[] = [];
  Object.entries(isObject(parsed.models) ? parsed.models : {}).forEach(([modelId, modelValue], modelIndex) => {
    const result = parseModel(modelId, modelValue, modelIndex);
    if (result.model) models.push(result.model);
    errors.push(...result.errors.map((item) => ({ ...item, modelIndex })));
  });
  if (errors.length > 0) return { ok: false, errors };

  const form = { models };
  const validation = validateOpenCodeModelForm(form);
  if (!validation.ok) return validation;

  return { ok: true, snapshot: cloneJson(parsed as JsonObject), form };
}

function parseNumber(
  value: string,
  path: string,
  minimum: number,
  location: Partial<OpenCodeModelConfigError>,
) {
  if (value.trim() === "") {
    return { error: error("required", path, `${path} is required`, location) };
  }
  const number = Number(value);
  if (!Number.isFinite(number) || number < minimum) {
    const boundary = minimum === 0 ? "a non-negative number" : "a positive number";
    return { error: error("invalid_number", path, `${path} must be ${boundary}`, location) };
  }
  return { value: number };
}

function parseOptionValue(row: OpenCodeOptionRow) {
  if (row.valueType === "string") return { value: row.value as JsonValue };
  if (row.valueType === "boolean") {
    if (row.value === "true") return { value: true as JsonValue };
    if (row.value === "false") return { value: false as JsonValue };
    return { error: "must be true or false" };
  }
  if (row.valueType === "number") {
    if (row.value.trim() === "" || !Number.isFinite(Number(row.value))) {
      return { error: "must be a number" };
    }
    return { value: Number(row.value) as JsonValue };
  }
  try {
    return { value: JSON.parse(row.value) as JsonValue };
  } catch {
    return { error: "must be a valid JSON value" };
  }
}

function validateOptionRows(
  rows: OpenCodeOptionRow[],
  path: string,
  modelIndex: number,
  variantIndex?: number,
) {
  const errors: OpenCodeModelConfigError[] = [];
  const keys = new Map<string, number>();
  rows.forEach((row, optionIndex) => {
    const rowPath = `${path}.${optionIndex}`;
    const key = row.key.trim();
    const location = { modelIndex, optionIndex, ...(variantIndex === undefined ? {} : { variantIndex }) };
    if (!key) {
      errors.push(error("required", `${rowPath}.key`, "Option key is required", location));
    } else if (keys.has(key)) {
      errors.push(error("duplicate", `${rowPath}.key`, "Option key must be unique", location));
    } else {
      keys.set(key, optionIndex);
    }
    const parsed = parseOptionValue(row);
    if (parsed.error) {
      errors.push(error("invalid_option_value", `${rowPath}.value`, parsed.error, location));
    }
  });
  return errors;
}

export function validateOpenCodeModelForm(
  form: OpenCodeModelsFormValue,
): OpenCodeModelFormValidationResult {
  const errors: OpenCodeModelConfigError[] = [];
  const modelIds = new Map<string, number>();

  form.models.forEach((model, modelIndex) => {
    const path = `models.${modelIndex}`;
    const modelId = model.id.trim();
    if (!modelId) {
      errors.push(error("required", `${path}.id`, "Model ID is required", { modelIndex }));
    } else if (modelIds.has(modelId)) {
      errors.push(error("duplicate", `${path}.id`, "Model ID must be unique", { modelIndex }));
    } else {
      modelIds.set(modelId, modelIndex);
    }

    if (model.cost.enabled) {
      for (const [key, value] of [["input", model.cost.input], ["output", model.cost.output]] as const) {
        const parsed = parseNumber(value, `${path}.cost.${key}`, 0, { modelIndex });
        if (parsed.error) errors.push(parsed.error);
      }
      for (const [key, value] of [
        ["cacheRead", model.cost.cacheRead],
        ["cacheWrite", model.cost.cacheWrite],
      ] as const) {
        if (value.trim() === "") continue;
        const parsed = parseNumber(value, `${path}.cost.${key}`, 0, { modelIndex });
        if (parsed.error) errors.push(parsed.error);
      }
    }

    if (model.limit.enabled) {
      for (const [key, value] of [["context", model.limit.context], ["output", model.limit.output]] as const) {
        const parsed = parseNumber(value, `${path}.limit.${key}`, Number.MIN_VALUE, { modelIndex });
        if (parsed.error) errors.push(parsed.error);
      }
    }

    errors.push(...validateOptionRows(model.options, `${path}.options`, modelIndex));
    const variantNames = new Set<string>();
    model.variants.forEach((variant, variantIndex) => {
      const variantPath = `${path}.variants.${variantIndex}`;
      const name = variant.name.trim();
      if (!name) {
        errors.push(error("required", `${variantPath}.name`, "Variant name is required", { modelIndex, variantIndex }));
      } else if (variantNames.has(name)) {
        errors.push(error("duplicate", `${variantPath}.name`, "Variant name must be unique", { modelIndex, variantIndex }));
      } else {
        variantNames.add(name);
      }
      errors.push(...validateOptionRows(variant.options, `${variantPath}.options`, modelIndex, variantIndex));
    });
  });

  return errors.length > 0 ? { ok: false, errors } : { ok: true };
}

function rowsToObject(rows: OpenCodeOptionRow[]): JsonObject {
  return Object.fromEntries(
    rows.map((row) => [row.key.trim(), parseOptionValue(row).value as JsonValue]),
  );
}

function stableValue(value: JsonValue): JsonValue {
  if (Array.isArray(value)) return value.map(stableValue);
  if (!isObject(value)) return value as JsonPrimitive;
  return Object.fromEntries(
    Object.keys(value)
      .sort()
      .map((key) => [key, stableValue(value[key] as JsonValue)]),
  );
}

export function stringifyOpenCodeModelConfig(snapshot: JsonObject) {
  return `${JSON.stringify(stableValue(snapshot), null, 2)}\n`;
}

export function mergeOpenCodeModelConfig(
  snapshot: JsonObject,
  form: OpenCodeModelsFormValue,
): OpenCodeModelConfigMergeResult {
  const validation = validateOpenCodeModelForm(form);
  if (!validation.ok) return validation;

  const next = cloneJson(snapshot);
  const previousModels = isObject(next.models) ? next.models : {};
  const models = Object.fromEntries(form.models.map((model) => {
    const modelId = model.id.trim();
    const sourceId = model.sourceId ?? modelId;
    const previous = isObject(previousModels[sourceId])
      ? cloneJson(previousModels[sourceId] as JsonObject)
      : {};

    if (model.name === "") delete previous.name;
    else previous.name = model.name;

    if (model.cost.enabled) {
      const cost = isObject(previous.cost) ? cloneJson(previous.cost as JsonObject) : {};
      cost.input = Number(model.cost.input);
      cost.output = Number(model.cost.output);
      if (model.cost.cacheRead.trim() === "") delete cost.cache_read;
      else cost.cache_read = Number(model.cost.cacheRead);
      if (model.cost.cacheWrite.trim() === "") delete cost.cache_write;
      else cost.cache_write = Number(model.cost.cacheWrite);
      previous.cost = cost;
    } else {
      delete previous.cost;
    }

    if (model.limit.enabled) {
      const limit = isObject(previous.limit) ? cloneJson(previous.limit as JsonObject) : {};
      limit.context = Number(model.limit.context);
      limit.output = Number(model.limit.output);
      previous.limit = limit;
    } else {
      delete previous.limit;
    }

    if (model.options.length > 0 || Object.hasOwn(previous, "options")) {
      previous.options = rowsToObject(model.options);
    }
    if (model.variants.length > 0 || Object.hasOwn(previous, "variants")) {
      previous.variants = Object.fromEntries(
        model.variants.map((variant) => [variant.name.trim(), rowsToObject(variant.options)]),
      );
    }
    return [modelId, previous];
  }));

  if (form.models.length > 0 || Object.hasOwn(next, "models")) next.models = models;
  const merged = cloneJson(next);
  return { ok: true, snapshot: merged, json: stringifyOpenCodeModelConfig(merged) };
}
