import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorkspaceSubagentsPanel } from "@/components/Workspaces/WorkspaceSubagentsPanel";
import { renderWithProviders } from "@/test/mocks/render";
import { emitMock, invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const subagentMocks = vi.hoisted(() => ({
  subagentsRescanMirror: vi.fn(),
  subagentsListInstalled: vi.fn(),
  subagentsListCatalog: vi.fn(),
  subagentsRepoList: vi.fn(),
  subagentsDetailGet: vi.fn(),
  subagentsCatalogDetailGet: vi.fn(),
  subagentsRepoDetailGet: vi.fn(),
  subagentsOpenFolder: vi.fn(),
  subagentsCatalogOpenFolder: vi.fn(),
  subagentsInstall: vi.fn(),
  subagentsRepoSetModel: vi.fn(),
  subagentsUninstall: vi.fn(),
}));

vi.mock("@/lib/subagents", () => subagentMocks);

describe("WorkspaceSubagentsPanel", () => {
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
    Object.values(subagentMocks).forEach((mock) => mock.mockReset());
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_storage_config") {
        return { subagents_sources: [{ id: "repo", name: "Repo" }] };
      }
      throw new Error(`Unexpected invoke command: ${command}`);
    });

    subagentMocks.subagentsRescanMirror.mockResolvedValue({
      data: [{ id: "sub-global", model: "claude", models: ["claude"], name: "Global Subagent", description: "Global", source_id: "repo", source_rel_path: "subagents/global", installed_at: 1, has_update: false, icon_seed: "a", scope: "global" }],
    });
    subagentMocks.subagentsListInstalled.mockResolvedValue({
      data: [{ id: "sub-project", model: "claude", models: ["claude", "gemini"], name: "Project Subagent", description: "Project", source_id: "repo", source_rel_path: "subagents/project", installed_at: 2, has_update: false, icon_seed: "b", scope: "project", project_root: "/tmp/demo" }],
    });
    subagentMocks.subagentsListCatalog.mockResolvedValue({
      data: [{ source_id: "repo", id: "sub-project", rel_path: "subagents/project", name: "Project Subagent", description: "Project", models: ["claude", "gemini"] }],
    });
    subagentMocks.subagentsRepoList.mockResolvedValue({
      data: [{ repo_key: "repo-key", subagent_id: "sub-project", source_id: "repo", source_rel_path: "subagents/project", source_type: "remote", name: "Project Subagent", description: "Project", models: ["claude", "gemini"], icon_seed: "b", has_update: false, installed: { claude: false, gemini: false, codex: false, opencode: false } }],
    });
    subagentMocks.subagentsCatalogDetailGet.mockResolvedValue({
      data: {
        subagent: { source_id: "repo", id: "sub-project", rel_path: "subagents/project", name: "Project Subagent", description: "Project", models: ["claude", "gemini"] },
        markdown: "# Subagent",
        source_path: "/tmp/subagent-source",
      },
    });
    subagentMocks.subagentsRepoDetailGet.mockResolvedValue({
      data: {
        subagent: { source_id: "repo", id: "sub-project", rel_path: "subagents/project", name: "Project Subagent", description: "Project", models: ["claude", "gemini"] },
        markdown: "# Repo Subagent",
        source_path: "/tmp/repo-subagent",
      },
    });
    subagentMocks.subagentsCatalogOpenFolder.mockResolvedValue({
      data: { repo_key: "opened-subagent-repo-key" },
    });
  });

  it("installs from repository with partial failure via subagent wrappers and emits refresh", async () => {
    const user = userEvent.setup();
    subagentMocks.subagentsRepoSetModel
      .mockResolvedValueOnce({})
      .mockRejectedValueOnce(new Error("gemini failed"));

    renderWithProviders(<WorkspaceSubagentsPanel rootPath="/tmp/demo" isVisible />);

    expect((await screen.findAllByText("Project Subagent")).length).toBeGreaterThan(0);
    const discoveryRegion = screen.getByRole("heading", { name: /Discover and Install|发现并安装/i }).closest("div.rounded-xl.border.bg-card");
    expect(discoveryRegion).not.toBeNull();
    await user.click(within(discoveryRegion as HTMLElement).getByRole("button", { name: /repository|仓库/i }));

    const repoCard = await waitFor(() => {
      const card = findRepositoryCardByName("Project Subagent");
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
      expect(subagentMocks.subagentsRepoSetModel).toHaveBeenCalledTimes(2);
      expect(emitMock).toHaveBeenCalledWith("refresh-counts");
    });
    expect(await screen.findByText(/Installed 1, failed 1|安装成功 1 个，失败 1 个/i)).toBeInTheDocument();
  });

  it("clears catalog detail state before opening repository subagent detail", async () => {
    const user = userEvent.setup();

    renderWithProviders(<WorkspaceSubagentsPanel rootPath="/tmp/demo" isVisible />);

    const discoveryRegion = screen.getByRole("heading", { name: /Discover and Install|发现并安装/i }).closest("div.rounded-xl.border.bg-card");
    expect(discoveryRegion).not.toBeNull();
    await user.click(within(discoveryRegion as HTMLElement).getByRole("button", { name: /recommended|推荐/i }));

    const recommendedCard = await waitFor(() => {
      const card = findRecommendedCardByName("Project Subagent", "Repo");
      expect(card).toBeTruthy();
      return card;
    });
    expect(recommendedCard).not.toBeNull();
    await user.click(recommendedCard as HTMLElement);

    let dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /Open Folder|打开文件夹/i }));
    await waitFor(() => {
      expect(subagentMocks.subagentsCatalogOpenFolder).toHaveBeenCalledWith({
        source_id: "repo",
        subagent_ref: "subagents/project",
      });
    });
    await user.click(within(dialog).getByRole("button", { name: /Close|关闭/i }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });

    await user.click(within(discoveryRegion as HTMLElement).getByRole("button", { name: /repository|仓库/i }));
    const repositoryCard = await waitFor(() => {
      const card = findRepositoryCardByName("Project Subagent");
      expect(card).toBeTruthy();
      return card;
    });
    expect(repositoryCard).not.toBeNull();
    await user.click(repositoryCard as HTMLElement);

    dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Repo Subagent")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: /Install to Workspace|安装到工作空间/i }));

    dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: /Install to Workspace|安装到工作空间/i }));

    await waitFor(() => {
      expect(subagentMocks.subagentsRepoSetModel).toHaveBeenCalledWith({
        repo_key: "repo-key",
        model: "claude",
        enabled: true,
        scope: "project",
        project_root: "/tmp/demo",
      });
    });
    expect(subagentMocks.subagentsRepoSetModel).not.toHaveBeenCalledWith(
      expect.objectContaining({ repo_key: "opened-subagent-repo-key" }),
    );
  });
});
