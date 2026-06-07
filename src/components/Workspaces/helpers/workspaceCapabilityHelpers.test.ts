import { describe, expect, it } from "vitest";
import {
  buildInstallStateFromCatalog,
  buildInstallTargetFromRepository,
  buildPartialInstallSummary,
  getRepositorySourceTypeLabel,
  matchesRepositoryItem,
  normalizeSourceNameMap,
  toggleSelectableModel,
} from "@/components/Workspaces/helpers/workspaceCapabilityHelpers";

describe("workspaceCapabilityHelpers", () => {
  it("normalizes source name maps", () => {
    expect(normalizeSourceNameMap({ skills_sources: [{ id: "repo", name: "Repo" }] }, "skills_sources")).toEqual({
      repo: "Repo",
    });
  });

  it("matches repository items by source path, id, or dir name", () => {
    const repo = { source_id: "src", source_rel_path: "a/b", dir_name: "demo", capability_id: "skill-1" };
    expect(matchesRepositoryItem(repo, { source_id: "src", source_rel_path: "a/b", id: "x" })).toBe(true);
    expect(matchesRepositoryItem(repo, { source_id: "other", source_rel_path: "x", id: "skill-1" })).toBe(true);
    expect(matchesRepositoryItem(repo, { source_id: "other", source_rel_path: "x", id: "y", dir_name: "demo" })).toBe(true);
  });

  it("maps repository source types to translated labels", () => {
    const t = (_key: string, defaultValue: string) => defaultValue;
    expect(getRepositorySourceTypeLabel("remote", "skills", t)).toBe("Recommended Source");
    expect(getRepositorySourceTypeLabel("local_import", "subagents", t)).toBe("Local Import");
    expect(getRepositorySourceTypeLabel("mirror", "skills", t)).toBe("Mirror");
    expect(getRepositorySourceTypeLabel("custom", "subagents", t)).toBe("custom");
    expect(getRepositorySourceTypeLabel("", "skills", t)).toBe("-");
  });

  it("builds install targets, install state, model toggles, and partial summaries", () => {
    expect(
      buildInstallTargetFromRepository({
        repo_key: "repo",
        capability_id: "skill-1",
        source_id: "src",
        source_rel_path: "a/b",
        dir_name: "demo",
        name: "Skill",
        description: "Desc",
        models: ["claude"],
        icon_seed: "x",
        source_type: "remote",
        has_update: false,
        installed: { claude: false, gemini: false, codex: false, opencode: false },
      }),
    ).toMatchObject({ id: "skill-1", repo_key: "repo" });

    expect(
      buildInstallStateFromCatalog(
        { source_id: "src", id: "skill-1", rel_path: "a/b", name: "Skill", description: "", models: ["claude", "gemini"] },
        {
          claude: [{ source_id: "src", source_rel_path: "a/b", id: "other" }],
          gemini: [],
        },
      ),
    ).toMatchObject({ claude: true, gemini: false });

    expect(toggleSelectableModel(["claude"], "gemini", ["claude", "gemini"])).toEqual(["claude", "gemini"]);
    expect(toggleSelectableModel(["claude"], "claude", ["claude", "gemini"])).toEqual([]);
    expect(buildPartialInstallSummary({ success: 1, failed: 2, failedModels: ["gemini", "codex"] })).toEqual({
      success: 1,
      failed: 2,
      models: "gemini, codex",
    });
  });
});
