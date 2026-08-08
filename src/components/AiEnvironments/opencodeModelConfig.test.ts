import { describe, expect, it } from "vitest";
import {
  mergeOpenCodeModelConfig,
  parseOpenCodeModelConfig,
  validateOpenCodeModelForm,
  type OpenCodeModelFormValue,
  type OpenCodeModelsFormValue,
} from "./opencodeModelConfig";

function model(overrides: Partial<OpenCodeModelFormValue> = {}): OpenCodeModelFormValue {
  return {
    id: "model-a",
    name: "Model A",
    cost: { enabled: false, input: "", output: "", cacheRead: "", cacheWrite: "" },
    limit: { enabled: false, context: "", output: "" },
    options: [],
    variants: [],
    ...overrides,
  };
}

function expectParsed(source: string) {
  const result = parseOpenCodeModelConfig(source);
  expect(result.ok).toBe(true);
  if (!result.ok) throw new Error("Expected OpenCode config to parse");
  return result;
}

describe("parseOpenCodeModelConfig", () => {
  it("rejects invalid JSON, non-object roots, and malformed model structures", () => {
    const invalidJson = parseOpenCodeModelConfig("{");
    const invalidRoot = parseOpenCodeModelConfig("[]");
    const invalidModels = parseOpenCodeModelConfig('{"models":[]}');
    const invalidModel = parseOpenCodeModelConfig('{"models":{"x":null}}');

    expect(invalidJson.ok ? [] : invalidJson.errors).toEqual([
      expect.objectContaining({ code: "invalid_json", path: "$" }),
    ]);
    expect(invalidRoot.ok ? [] : invalidRoot.errors).toEqual([
      expect.objectContaining({ code: "invalid_structure", path: "$" }),
    ]);
    expect(invalidModels.ok ? [] : invalidModels.errors).toEqual([
      expect.objectContaining({ path: "models" }),
    ]);
    expect(invalidModel.ok ? [] : invalidModel.errors).toEqual([
      expect.objectContaining({ path: "models.x", modelIndex: 0 }),
    ]);
  });

  it("creates a detached snapshot and form with cost, limit, options, and variants", () => {
    const source = {
      providerMeta: { region: "west" },
      models: {
        "model-a": {
          name: "Model A",
          cost: { input: 0, output: 2, cache_read: 0.5 },
          limit: { context: 128000, output: 4096 },
          options: { temperature: 0.2, reasoningEffort: "high", custom: { depth: 2 } },
          variants: { fast: { temperature: 0, flags: ["a"] } },
        },
      },
    };
    const result = expectParsed(JSON.stringify(source));

    expect(result.form.models[0]).toMatchObject({
      id: "model-a",
      name: "Model A",
      cost: { enabled: true, input: "0", output: "2", cacheRead: "0.5", cacheWrite: "" },
      limit: { enabled: true, context: "128000", output: "4096" },
    });
    expect(result.form.models[0].options).toEqual([
      expect.objectContaining({ key: "temperature", value: "0.2", valueType: "number", custom: false }),
      expect.objectContaining({ key: "reasoningEffort", value: "high", valueType: "string", custom: false }),
      expect.objectContaining({ key: "custom", value: '{"depth":2}', valueType: "json", custom: true }),
    ]);
    expect(result.form.models[0].variants[0]).toMatchObject({ name: "fast" });
    result.snapshot.providerMeta = { region: "changed" };
    expect(source.providerMeta.region).toBe("west");
  });
});

describe("validateOpenCodeModelForm", () => {
  it("locates empty and duplicate model IDs", () => {
    const result = validateOpenCodeModelForm({
      models: [model({ id: " " }), model({ id: "same" }), model({ id: "same" })],
    });

    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toEqual(expect.arrayContaining([
      expect.objectContaining({ code: "required", path: "models.0.id", modelIndex: 0 }),
      expect.objectContaining({ code: "duplicate", path: "models.2.id", modelIndex: 2 }),
    ]));
  });

  it("enforces cost and limit boundaries while allowing zero cost", () => {
    const valid = model({
      cost: { enabled: true, input: "0", output: "0", cacheRead: "", cacheWrite: "0" },
      limit: { enabled: true, context: "1", output: "0.5" },
    });
    expect(validateOpenCodeModelForm({ models: [valid] })).toEqual({ ok: true });

    const invalid = model({
      cost: { enabled: true, input: "", output: "-1", cacheRead: "bad", cacheWrite: "" },
      limit: { enabled: true, context: "0", output: "-1" },
    });
    const result = validateOpenCodeModelForm({ models: [invalid] });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors.map(({ path }) => path)).toEqual([
      "models.0.cost.input",
      "models.0.cost.output",
      "models.0.cost.cacheRead",
      "models.0.limit.context",
      "models.0.limit.output",
    ]);
  });

  it("locates option and variant row errors", () => {
    const result = validateOpenCodeModelForm({
      models: [model({
        options: [
          { id: "a", key: "same", value: "1", valueType: "json", custom: true },
          { id: "b", key: "same", value: "no-json", valueType: "json", custom: true },
        ],
        variants: [
          { id: "v1", name: "fast", options: [] },
          {
            id: "v2",
            name: "fast",
            options: [{ id: "vo", key: "enabled", value: "yes", valueType: "boolean", custom: true }],
          },
        ],
      })],
    });

    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toEqual(expect.arrayContaining([
      expect.objectContaining({ code: "duplicate", path: "models.0.options.1.key", optionIndex: 1 }),
      expect.objectContaining({ code: "invalid_option_value", path: "models.0.options.1.value", optionIndex: 1 }),
      expect.objectContaining({ code: "duplicate", path: "models.0.variants.1.name", variantIndex: 1 }),
      expect.objectContaining({
        code: "invalid_option_value",
        path: "models.0.variants.1.options.0.value",
        variantIndex: 1,
        optionIndex: 0,
      }),
    ]));
  });
});

describe("mergeOpenCodeModelConfig", () => {
  it("round-trips while deeply preserving unknown provider, model, cost, and limit fields", () => {
    const parsed = expectParsed(JSON.stringify({
      providerMeta: { nested: { keep: [1, { yes: true }] } },
      models: {
        "model-a": {
          name: "Old",
          modelUnknown: { nested: { keep: true } },
          cost: { input: 1, output: 2, unitHint: { keep: "unknown" } },
          limit: { context: 10, output: 2, nested: { keep: true } },
          options: { custom: { deep: { keep: true } } },
          variants: { high: { custom: { keep: true } } },
        },
      },
    }));
    parsed.form.models[0].name = "New";
    parsed.form.models[0].cost.output = "3";
    parsed.form.models[0].limit.context = "20";

    const merged = mergeOpenCodeModelConfig(parsed.snapshot, parsed.form);
    expect(merged.ok).toBe(true);
    if (!merged.ok) return;
    expect(JSON.parse(merged.json)).toEqual({
      models: {
        "model-a": {
          cost: { input: 1, output: 3, unitHint: { keep: "unknown" } },
          limit: { context: 20, nested: { keep: true }, output: 2 },
          modelUnknown: { nested: { keep: true } },
          name: "New",
          options: { custom: { deep: { keep: true } } },
          variants: { high: { custom: { keep: true } } },
        },
      },
      providerMeta: { nested: { keep: [1, { yes: true }] } },
    });
  });

  it("preserves unknown model fields when its ID changes", () => {
    const parsed = expectParsed('{"models":{"old-id":{"name":"Old","unknown":{"keep":true}}}}');
    parsed.form.models[0].id = "new-id";

    const merged = mergeOpenCodeModelConfig(parsed.snapshot, parsed.form);
    expect(merged.ok).toBe(true);
    if (!merged.ok) return;
    expect(JSON.parse(merged.json)).toEqual({
      models: { "new-id": { name: "Old", unknown: { keep: true } } },
    });
  });

  it("omits disabled cost and limit and does not serialize empty cache values or currency", () => {
    const form: OpenCodeModelsFormValue = {
      models: [model({
        cost: { enabled: true, input: "0", output: "1", cacheRead: "", cacheWrite: "" },
      })],
    };
    const merged = mergeOpenCodeModelConfig({}, form);
    expect(merged.ok).toBe(true);
    if (!merged.ok) return;
    expect(JSON.parse(merged.json)).toEqual({
      models: { "model-a": { cost: { input: 0, output: 1 }, name: "Model A" } },
    });
    expect(merged.json).not.toContain("currency");

    form.models[0].cost.enabled = false;
    const withoutCost = mergeOpenCodeModelConfig(merged.snapshot, form);
    expect(withoutCost.ok && JSON.parse(withoutCost.json).models["model-a"].cost).toBeUndefined();
  });

  it("supports every valid custom JSON value and variant overrides", () => {
    const values = ["null", "true", "12.5", '"text"', '[1,"two"]', '{"nested":false}'];
    const form: OpenCodeModelsFormValue = {
      models: [model({
        options: values.map((value, index) => ({
          id: `option-${index}`,
          key: `custom${index}`,
          value,
          valueType: "json",
          custom: true,
        })),
        variants: [{
          id: "variant-fast",
          name: "fast",
          options: [{ id: "variant-option", key: "temperature", value: "0", valueType: "number", custom: false }],
        }],
      })],
    };
    const merged = mergeOpenCodeModelConfig({}, form);
    expect(merged.ok).toBe(true);
    if (!merged.ok) return;
    expect(JSON.parse(merged.json).models["model-a"]).toMatchObject({
      options: {
        custom0: null,
        custom1: true,
        custom2: 12.5,
        custom3: "text",
        custom4: [1, "two"],
        custom5: { nested: false },
      },
      variants: { fast: { temperature: 0 } },
    });
  });

  it("does not produce JSON when validation fails", () => {
    const merged = mergeOpenCodeModelConfig(
      { untouched: true },
      { models: [model({ options: [{ id: "bad", key: "x", value: "{", valueType: "json", custom: true }] })] },
    );

    expect(merged.ok).toBe(false);
    expect("json" in merged).toBe(false);
    expect("snapshot" in merged).toBe(false);
  });

  it("serializes deterministically regardless of source key insertion order", () => {
    const first = expectParsed('{"z":1,"models":{"b":{"name":"B"},"a":{"name":"A"}},"a":2}');
    const second = expectParsed('{"a":2,"models":{"a":{"name":"A"},"b":{"name":"B"}},"z":1}');
    const firstMerged = mergeOpenCodeModelConfig(first.snapshot, first.form);
    const secondMerged = mergeOpenCodeModelConfig(second.snapshot, second.form);

    expect(firstMerged.ok && secondMerged.ok && firstMerged.json).toBe(secondMerged.ok ? secondMerged.json : "");
  });
});
