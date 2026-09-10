import { describe, expect, it } from "vitest";
import { skillModelOptions } from "./skillsModelOptions";
import { subagentModelOptions } from "./subagentsModelOptions";

const EXPECTED_IDS = ["claude", "antigravity", "codex", "opencode"];

describe("Skills 与 Subagents 模型选项", () => {
  it("skillModelOptions 恰好枚举四个工具且不含 gemini", () => {
    const ids = skillModelOptions.map((option) => option.id);
    expect([...ids].sort()).toEqual([...EXPECTED_IDS].sort());
    expect(ids).not.toContain("gemini");
  });

  it("subagentModelOptions 恰好枚举四个工具且不含 gemini", () => {
    const ids = subagentModelOptions.map((option) => option.id);
    expect([...ids].sort()).toEqual([...EXPECTED_IDS].sort());
    expect(ids).not.toContain("gemini");
  });
});
