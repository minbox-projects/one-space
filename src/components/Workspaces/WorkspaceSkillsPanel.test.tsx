import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorkspaceSkillsPanel } from "@/components/Workspaces/WorkspaceSkillsPanel";
import { renderWithProviders } from "@/test/mocks/render";
import { emitMock, invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const skillsMocks = vi.hoisted(() => ({
  skillsRescanMirror: vi.fn(),
  skillsListInstalled: vi.fn(),
  skillsListCatalog: vi.fn(),
  skillsRepoList: vi.fn(),
  skillsDetailGet: vi.fn(),
  skillsCatalogDetailGet: vi.fn(),
  skillsRepoDetailGet: vi.fn(),
  skillsOpenFolder: vi.fn(),
  skillsCatalogOpenFolder: vi.fn(),
  skillsInstall: vi.fn(),
  skillsRepoSetModel: vi.fn(),
  skillsUninstall: vi.fn(),
}));

vi.mock("@/lib/skills", () => skillsMocks);

describe("WorkspaceSkillsPanel", () => {
  function findNearestAncestor(node: HTMLElement, predicate: (element: HTMLElement) => boolean) {
    let current: HTMLElement | null = node;
    while (current) {
      if (predicate(current)) {
        return current;
      }
      current = current.parentElement;
    }
    return null;
  }

  function findRepositoryCardByName(name: string) {
    return screen.getAllByRole("button", { name: /Install to Workspace|安装到工作空间/i })
      .map((button) =>
        findNearestAncestor(button as HTMLElement, (element) => {
          const text = element.textContent || "";
          const className = typeof element.className === "string" ? element.className : "";
          return text.includes(name) && /cursor-pointer/.test(className);
        }),
      )
      .find(Boolean);
  }

  function findRecommendedCardByName(name: string, sourceName: string) {
    return screen.getAllByText(name)
      .map((node) =>
        findNearestAncestor(node as HTMLElement, (element) => {
          const text = element.textContent || "";
          const className = typeof element.className === "string" ? element.className : "";
          return text.includes(name) && text.includes(sourceName) && /cursor-pointer/.test(className);
        }),
      )
      .find(Boolean);
  }

  beforeEach(() => {
    resetTauriMocks();
    Object.values(skillsMocks).forEach((mock) => mock.mockReset());
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_storage_config") {
        return { skills_sources: [{ id: "repo", name: "Repo" }] };
      }
      throw new Error(`Unexpected invoke command: ${command}`);
    });

    skillsMocks.skillsRescanMirror.mockResolvedValue({
      data: [{ id: "skill-global", model: "claude", models: ["claude"], name: "Global Skill", description: "Global", source_id: "repo", source_rel_path: "skills/global", installed_at: 1, has_update: false, icon_seed: "a", scope: "global" }],
    });
    skillsMocks.skillsListInstalled.mockResolvedValue({
      data: [{ id: "skill-project", model: "claude", models: ["claude", "gemini"], name: "Project Skill", description: "Project", source_id: "repo", source_rel_path: "skills/project", installed_at: 2, has_update: false, icon_seed: "b", scope: "project", project_root: "/tmp/demo" }],
    });
    skillsMocks.skillsListCatalog.mockResolvedValue({
      data: [{ source_id: "repo", id: "skill-project", rel_path: "skills/project", name: "Project Skill", description: "Project", models: ["claude", "gemini"] }],
    });
    skillsMocks.skillsRepoList.mockResolvedValue({
      data: [{ repo_key: "repo-key", skill_id: "skill-project", source_id: "repo", source_rel_path: "skills/project", source_type: "remote", name: "Project Skill", description: "Project", models: ["claude", "gemini"], icon_seed: "b", has_update: false, installed: { claude: false, gemini: false, codex: false, opencode: false } }],
    });
    skillsMocks.skillsCatalogDetailGet.mockResolvedValue({
      data: {
        skill: { source_id: "repo", id: "skill-project", rel_path: "skills/project", name: "Project Skill", description: "Project", models: ["claude", "gemini"] },
        markdown: "# Skill",
        source_path: "/tmp/source",
      },
    });
    skillsMocks.skillsRepoDetailGet.mockResolvedValue({
      data: {
        skill: { source_id: "repo", id: "skill-project", rel_path: "skills/project", name: "Project Skill", description: "Project", models: ["claude", "gemini"] },
        markdown: "# Repo Skill",
        source_path: "/tmp/repo-skill",
      },
    });
    skillsMocks.skillsCatalogOpenFolder.mockResolvedValue({
      data: { repo_key: "opened-repo-key" },
    });
  });

  it("installs from repository with partial failure via wrappers and emits refresh", async () => {
    const user = userEvent.setup();
    skillsMocks.skillsRepoSetModel
      .mockResolvedValueOnce({})
      .mockRejectedValueOnce(new Error("gemini failed"));

    renderWithProviders(<WorkspaceSkillsPanel rootPath="/tmp/demo" isVisible />);

    expect((await screen.findAllByText("Project Skill")).length).toBeGreaterThan(0);
    const discoveryRegion = screen.getByRole("heading", { name: /Discover and Install|发现并安装/i }).closest("div.rounded-xl.border.bg-card");
    expect(discoveryRegion).not.toBeNull();
    await user.click(within(discoveryRegion as HTMLElement).getByRole("button", { name: /repository|仓库/i }));

    const repoCard = await waitFor(() => {
      const card = findRepositoryCardByName("Project Skill");
      expect(card).toBeTruthy();
      return card;
    });
    expect(repoCard).not.toBeNull();
    expect(repoCard?.textContent).toMatch(/Recommended Source|推荐源/i);
    expect(within(repoCard as HTMLElement).queryByText(/^remote$/i)).not.toBeInTheDocument();
    await user.click(within(repoCard as HTMLElement).getByRole("button", { name: /Install to Workspace|安装到工作空间/i }));

    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /Gemini/i }));
    await user.click(within(dialog).getByRole("button", { name: /Install to Workspace|安装到工作空间/i }));

    await waitFor(() => {
      expect(skillsMocks.skillsRepoSetModel).toHaveBeenCalledTimes(2);
      expect(emitMock).toHaveBeenCalledWith("refresh-counts");
    });
    expect(await screen.findByText(/Installed 1, failed 1|安装成功 1 个，失败 1 个/i)).toBeInTheDocument();
  });

  it("uses wrapper uninstall path and keeps user-level navigation action", async () => {
    const user = userEvent.setup();
    const onNavigateToGlobalPage = vi.fn();
    skillsMocks.skillsUninstall.mockResolvedValue({});
    renderWithProviders(<WorkspaceSkillsPanel rootPath="/tmp/demo" isVisible onNavigateToGlobalPage={onNavigateToGlobalPage} />);

    expect((await screen.findAllByText("Project Skill")).length).toBeGreaterThan(0);
    await user.click(screen.getByRole("button", { name: /Manage User-level|管理用户级/i }));
    expect(onNavigateToGlobalPage).toHaveBeenCalledWith("installed");

    await user.click(screen.getByRole("button", { name: /Uninstall|卸载/i }));
    await user.click(await screen.findByRole("button", { name: /OK|确定/i }));
    await waitFor(() => {
      expect(skillsMocks.skillsUninstall).toHaveBeenCalledWith({
        model: "claude",
        skill_id: "skill-project",
        scope: "project",
        project_root: "/tmp/demo",
      });
    });
  });

  it("clears catalog detail state when reopening a different skill detail path", async () => {
    const user = userEvent.setup();

    renderWithProviders(<WorkspaceSkillsPanel rootPath="/tmp/demo" isVisible />);

    const discoveryRegion = screen.getByRole("heading", { name: /Discover and Install|发现并安装/i }).closest("div.rounded-xl.border.bg-card");
    expect(discoveryRegion).not.toBeNull();
    await user.click(within(discoveryRegion as HTMLElement).getByRole("button", { name: /recommended|推荐/i }));

    const recommendedCard = await waitFor(() => {
      const card = findRecommendedCardByName("Project Skill", "Repo");
      expect(card).toBeTruthy();
      return card;
    });
    expect(recommendedCard).not.toBeNull();
    await user.click(recommendedCard as HTMLElement);

    let dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /Open Folder|打开文件夹/i }));
    await waitFor(() => {
      expect(skillsMocks.skillsCatalogOpenFolder).toHaveBeenCalledWith({
        source_id: "repo",
        skill_ref: "skills/project",
      });
    });
    await user.click(within(dialog).getByRole("button", { name: /Close|关闭/i }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });

    await user.click(within(discoveryRegion as HTMLElement).getByRole("button", { name: /repository|仓库/i }));
    const repositoryCard = await waitFor(() => {
      const card = findRepositoryCardByName("Project Skill");
      expect(card).toBeTruthy();
      return card;
    });
    expect(repositoryCard).not.toBeNull();
    await user.click(repositoryCard as HTMLElement);

    dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Repo Skill")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: /Install to Workspace|安装到工作空间/i }));

    dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /Install to Workspace|安装到工作空间/i }));

    await waitFor(() => {
      expect(skillsMocks.skillsRepoSetModel).toHaveBeenCalledWith({
        repo_key: "repo-key",
        model: "claude",
        enabled: true,
        scope: "project",
        project_root: "/tmp/demo",
      });
    });
    expect(skillsMocks.skillsRepoSetModel).not.toHaveBeenCalledWith(
      expect.objectContaining({ repo_key: "opened-repo-key" }),
    );
  });
});
