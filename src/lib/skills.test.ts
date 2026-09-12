import { describe, expect, it } from "vitest";
import {
  skillsCompatibilityGet,
  skillsDetailGet,
  skillsInitializeUnified,
  skillsInstall,
  skillsListInstalled,
  skillsRepoList,
  skillsRescanMirror,
  skillsSyncNow,
  skillsSyncStatusGet,
} from "@/lib/skills";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

describe("skills IPC command contract", () => {
  it("uses the existing Rust command names and argument shapes", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});

    await skillsListInstalled({ model: "codex", scope: "global", project_root: null });
    await skillsInstall({
      source_id: "official",
      skill_ref: "git-commit",
      model: "codex",
      scope: "global",
    });
    await skillsSyncStatusGet();
    await skillsSyncNow();
    await skillsRescanMirror();
    await skillsRepoList(false, { scope: "global" });
    await skillsDetailGet({ model: "codex", skill_id: "official-git-commit", scope: "global" });

    expect(invokeMock.mock.calls).toEqual([
      ["skills_list_installed", { model: "codex", scope: "global", projectRoot: null }],
      [
        "skills_install",
        {
          input: {
            source_id: "official",
            skill_ref: "git-commit",
            model: "codex",
            scope: "global",
          },
        },
      ],
      ["skills_sync_status_get", undefined],
      ["skills_sync_now", undefined],
      ["skills_rescan_mirror", undefined],
      ["skills_repo_list", { includeUpdate: false, scope: "global", projectRoot: null }],
      [
        "skills_detail_get",
        { input: { model: "codex", skill_id: "official-git-commit", scope: "global" } },
      ],
    ]);
  });

  it("passes through canonical unified display paths without rewriting them", async () => {
    resetTauriMocks();
    const unified = "/Users/example/.agents/skills/git-commit";
    invokeMock.mockResolvedValueOnce({
      ok: true,
      data: [
        {
          id: "official-git-commit",
          dir_name: "git-commit",
          model: "codex",
          scope: "global",
          target_path: unified,
        },
      ],
      meta: { revision: 1, ts: 1 },
    });

    const result = await skillsListInstalled<{
      data: Array<{ target_path: string }>;
    }>({ model: "codex", scope: "global" });

    expect(result.data[0].target_path).toBe(unified);
  });

  it("exposes production wrappers for unified initialization and compatibility results", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});

    await skillsInitializeUnified();
    await skillsCompatibilityGet();

    expect(invokeMock.mock.calls).toEqual([
      ["skills_initialize_unified", undefined],
      ["skills_compatibility_get", undefined],
    ]);
  });
});
