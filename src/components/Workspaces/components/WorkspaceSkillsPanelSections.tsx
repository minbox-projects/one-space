import { BookOpen, Download, FolderOpen, Info, Loader2, RefreshCw, Sparkles, Trash2 } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { skillModelOptions } from "../../skillsModelOptions";
import type { WorkspaceCapabilityEntry } from "../../workspaceCapabilityContext";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "../../ui/dialog";
import { getRepositorySourceTypeLabel } from "../helpers/workspaceCapabilityHelpers";
import { getScopeBadgeClassName } from "../helpers/workspaceHelpers";
import { useWorkspaceSkillsPanelState } from "../hooks/useWorkspaceSkillsPanelState";

const modelIconMap = skillModelOptions.reduce(
  (acc, item) => {
    acc[item.id] = item.Icon;
    return acc;
  },
  {} as Record<string, React.ComponentType<{ className?: string }>>,
);

const iconPool = [Sparkles, RefreshCw, Trash2, Info, BookOpen];

function pickIcon(seed: string) {
  const sum = seed.split("").reduce((acc, c) => acc + c.charCodeAt(0), 0);
  return iconPool[sum % iconPool.length];
}

export function WorkspaceSkillsPanelHeader(props: {
  t: ReturnType<typeof useWorkspaceSkillsPanelState>["message"] extends never ? never : any;
  loading: boolean;
  message: { type: "success" | "error"; text: string } | null;
  onRefresh: () => void;
  onNavigate?: (entry: WorkspaceCapabilityEntry) => void;
}) {
  const { t, loading, message, onRefresh, onNavigate } = props;
  return (
    <>
      <div className="rounded-xl border bg-card p-4">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
          <div className="min-w-0">
            <h2 className="text-lg font-semibold tracking-tight">{t("skills", "Skills")}</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              {t("workspaceSkillsSectionDesc", "Manage effective user-level and directory-level skills available to this workspace.")}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2 lg:justify-end">
            <button
              type="button"
              onClick={onRefresh}
              disabled={loading}
              className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted disabled:opacity-60"
            >
              <RefreshCw className={`h-4 w-4 ${loading ? "animate-spin" : ""}`} />
              {t("refresh", "Refresh")}
            </button>
            <button
              type="button"
              onClick={() => onNavigate?.("recommended")}
              className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted"
            >
              <Sparkles className="h-4 w-4" />
              {t("workspaceManageSources", "Manage Sources")}
            </button>
            <button
              type="button"
              onClick={() => onNavigate?.("repository")}
              className="inline-flex items-center gap-2 rounded-md border px-3 py-2 text-sm hover:bg-muted"
            >
              <BookOpen className="h-4 w-4" />
              {t("workspaceOpenRepository", "Open Repository")}
            </button>
          </div>
        </div>
      </div>

      {(message || loading) && (
        <div className="flex flex-wrap items-center justify-end gap-2">
          {loading && (
            <div className="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs text-muted-foreground">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t("loading", "Loading...")}
            </div>
          )}
          {message && (
            <div
              className={`rounded-md border px-2.5 py-1.5 text-xs ${
                message.type === "error"
                  ? "border-destructive/20 bg-destructive/10 text-destructive"
                  : "border-green-500/20 bg-green-500/10 text-green-700"
              }`}
            >
              {message.text}
            </div>
          )}
        </div>
      )}
    </>
  );
}

export function WorkspaceSkillsInstalledSection(props: {
  t: any;
  state: ReturnType<typeof useWorkspaceSkillsPanelState>;
}) {
  const { t, state } = props;
  return (
    <>
      <div className="rounded-xl border bg-card p-3">
        <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
          {state.modelTabs.map((tab) => {
            const ModelIcon = modelIconMap[tab.id];
            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => state.setActiveModel(tab.id)}
                className={`rounded-lg border px-4 py-3 text-left transition-all ${
                  state.activeModel === tab.id ? "border-primary bg-primary/5" : "hover:bg-muted/40 hover:-translate-y-0.5"
                }`}
              >
                <div className="flex items-center gap-2">
                  <ModelIcon className="h-5 w-5" />
                  <span className="text-sm font-semibold">{tab.label}</span>
                </div>
                <div className="mt-2.5 text-sm leading-none text-muted-foreground">
                  {t("skillsInstalledCount", "Installed {{count}} skills", { count: state.installedCounts[tab.id] ?? 0 })}
                </div>
              </button>
            );
          })}
        </div>
      </div>
      <div className="flex items-start gap-2 rounded-lg border bg-muted/30 px-3 py-2 text-xs leading-relaxed text-muted-foreground">
        <Info className="mt-0.5 h-4 w-4 shrink-0" />
        <p>
          <span className="font-medium text-foreground">{t("workspaceEffectiveLoadRule", "Effective load rule")}:</span>{" "}
          {state.activeSkillLoadRule}
        </p>
      </div>

      {!state.initialLoadDone ? (
        <div className="py-12 text-center text-muted-foreground">
          <Loader2 className="mx-auto mb-3 h-8 w-8 animate-spin" />
          <p>{t("loading", "Loading...")}</p>
        </div>
      ) : state.activeInstalled.length === 0 ? (
        <div className="py-12 text-center">
          <Sparkles className="mx-auto mb-4 h-16 w-16 text-muted-foreground" />
          <h3 className="mb-2 text-lg font-semibold">{t("noInstalledSkillsForModel", "该模型下暂无已安装 Skills")}</h3>
          <p className="text-muted-foreground">
            {t("workspaceNoInstalledSkillsForModelDesc", "This workspace has no installed skills for the selected model yet.")}
          </p>
          <div className="mt-4 flex flex-wrap justify-center gap-2">
            <button type="button" onClick={() => state.setDiscoveryMode("recommended")} className="rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground">
              {t("workspaceInstallFromRecommended", "Install from Recommended")}
            </button>
            <button type="button" onClick={() => state.setDiscoveryMode("repository")} className="rounded-md border px-4 py-2 text-sm hover:bg-muted">
              {t("workspaceInstallFromRepository", "Install from Repository")}
            </button>
          </div>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
          {state.activeInstalled.map((skill) => {
            const Icon = pickIcon(skill.icon_seed || skill.id);
            const reinstallKey = `${skill.model}:${skill.id}`;
            const reinstalling = !!state.reinstallingKeys[reinstallKey];
            const isUserLevel = skill.scope === "global";
            return (
              <div
                key={`${skill.model}:${skill.id}`}
                className="cursor-pointer rounded-xl border bg-card p-4 transition-all duration-200 hover:border-primary/30 hover:shadow-md"
                onClick={() => {
                  void state.handleOpenDetail(skill);
                }}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="rounded-md bg-primary/10 p-2 text-primary">
                    <Icon className="h-4 w-4" />
                  </div>
                  <div className="flex max-w-[60%] flex-col items-end gap-1">
                    <span className={`rounded-full border px-2 py-0.5 text-[10px] font-medium ${getScopeBadgeClassName(skill.scope)}`}>
                      {skill.scope === "global" ? t("workspaceScopeUser", "User-level") : t("workspaceScopeDirectory", "Directory-level")}
                    </span>
                    <span className="truncate text-[10px] text-muted-foreground">{skill.dir_name || skill.source_rel_path.split("/").pop() || skill.id}</span>
                  </div>
                </div>
                <h4 className="mt-3 text-sm font-semibold">{skill.name}</h4>
                <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{skill.description}</p>
                <div className="mt-3 text-[11px] text-muted-foreground">
                  {t("lastUpdated", "Last updated")}: {state.formatTs(skill.updated_at || skill.installed_at)}
                </div>
                <div className="mt-3 flex items-center justify-end gap-2">
                  {isUserLevel ? (
                    <button
                      type="button"
                      onClick={(event) => {
                        event.stopPropagation();
                        state.onNavigateToGlobalPage?.("installed");
                      }}
                      className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs hover:bg-muted"
                    >
                      <BookOpen className="h-3.5 w-3.5" />
                      {t("workspaceManageUserLevel", "Manage User-level")}
                    </button>
                  ) : (
                    <>
                      <button
                        type="button"
                        disabled={reinstalling}
                        onClick={(event) => {
                          event.stopPropagation();
                          void state.handleReinstall(skill);
                        }}
                        className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs hover:bg-muted disabled:opacity-50"
                      >
                        <RefreshCw className={`h-3.5 w-3.5 ${reinstalling ? "animate-spin" : ""}`} />
                        {t("skillsReinstall", "重新安装")}
                      </button>
                      <button
                        type="button"
                        onClick={(event) => {
                          event.stopPropagation();
                          void state.handleUninstall(skill);
                        }}
                        className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs text-destructive hover:bg-destructive/10"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                        {t("uninstall", "Uninstall")}
                      </button>
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}

export function WorkspaceSkillsDiscoverySection(props: {
  t: any;
  state: ReturnType<typeof useWorkspaceSkillsPanelState>;
}) {
  const { t, state } = props;
  return (
    <>
      <div className="rounded-xl border bg-card p-4">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
          <div>
            <h3 className="text-base font-semibold tracking-tight">{t("workspaceDiscoverySectionTitle", "Discover and Install")}</h3>
            <p className="text-sm text-muted-foreground">
              {t("workspaceSkillsDiscoveryDesc", "Find recommended or repository skills and install them directly into this workspace.")}
            </p>
          </div>
          <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
            <button type="button" onClick={() => state.setDiscoveryMode("recommended")} className={`rounded-md px-3 py-1.5 text-sm ${state.discoveryMode === "recommended" ? "bg-black text-white" : "bg-white text-black"}`}>
              {t("recommended", "推荐")}
            </button>
            <button type="button" onClick={() => state.setDiscoveryMode("repository")} className={`rounded-md px-3 py-1.5 text-sm ${state.discoveryMode === "repository" ? "bg-black text-white" : "bg-white text-black"}`}>
              {t("repository", "仓库")}
            </button>
          </div>
        </div>
      </div>

      {state.discoveryMode === "recommended" ? (
        <>
          <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
            <input value={state.recommendedSearch} onChange={(event) => state.setRecommendedSearch(event.target.value)} placeholder={t("skillsSearchPlaceholder", "搜索 Skill 名称或描述")} className="h-10 w-full rounded-lg border px-3 text-sm lg:max-w-sm" />
            <div className="overflow-x-auto">
              <div className="inline-flex w-max rounded-lg border border-black bg-white p-1 whitespace-nowrap">
                <button type="button" onClick={() => state.setRecommendedSourceFilter("all")} className={`rounded-md px-3 py-1.5 text-sm ${state.recommendedSourceFilter === "all" ? "bg-black text-white" : "bg-white text-black"}`}>
                  {t("all", "全部")}
                </button>
                {state.catalogSources.map((source) => (
                  <button key={source.id} type="button" onClick={() => state.setRecommendedSourceFilter(source.id)} className={`rounded-md px-3 py-1.5 text-sm ${state.recommendedSourceFilter === source.id ? "bg-black text-white" : "bg-white text-black"}`}>
                    {source.label}
                  </button>
                ))}
              </div>
            </div>
          </div>

          {state.visibleCatalog.length === 0 ? (
            <div className="py-12 text-center">
              <Sparkles className="mx-auto mb-4 h-16 w-16 text-muted-foreground" />
              <h3 className="mb-2 text-lg font-semibold">{t("noRecommendedSkills", "当前没有可推荐的 Skills")}</h3>
              <p className="text-muted-foreground">{t("noRecommendedSkillsDesc", "请检查 Skills 源配置，或同步源列表后重试。")}</p>
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {state.visibleCatalog.map((item) => {
                const installed = state.installedBySourcePath.get(`${item.source_id}:${item.rel_path}`);
                const Icon = pickIcon(item.id);
                return (
                  <div key={`${item.source_id}:${item.id}`} className="cursor-pointer rounded-xl border bg-card p-4 transition-all duration-200 hover:border-primary/30 hover:shadow-md" onClick={() => { void state.handleOpenCatalogDetail(item); }}>
                    <div className="flex items-start justify-between gap-3">
                      <div className="rounded-md bg-muted p-2 text-foreground">
                        <Icon className="h-4 w-4" />
                      </div>
                      <span className="text-[10px] text-muted-foreground">{item.dir_name || item.rel_path.split("/").pop() || item.id}</span>
                    </div>
                    <h4 className="mt-3 text-sm font-semibold">{item.name}</h4>
                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{item.description}</p>
                    <div className="mt-3 flex items-center justify-between gap-2">
                      <span className="rounded border bg-muted/50 px-2 py-1 text-[10px] text-muted-foreground">{state.sourceNamesById[item.source_id] || item.source_id}</span>
                      {installed ? (
                        <span className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs text-muted-foreground">
                          <Download className="h-3.5 w-3.5" />
                          {t("installed", "Installed")}
                        </span>
                      ) : (
                        <button
                          type="button"
                          onClick={(event) => {
                            event.stopPropagation();
                            state.openInstallDialog({
                              source_id: item.source_id,
                              id: item.id,
                              rel_path: item.rel_path,
                              dir_name: item.dir_name,
                              name: item.name,
                              description: item.description,
                              models: item.models,
                            }, "catalog");
                          }}
                          className="inline-flex items-center gap-1 rounded-md bg-primary px-2.5 py-1 text-xs text-primary-foreground"
                        >
                          <Download className="h-3.5 w-3.5" />
                          {t("workspaceInstallAction", "Install to Workspace")}
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </>
      ) : (
        <>
          <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
            <input value={state.repositorySearch} onChange={(event) => state.setRepositorySearch(event.target.value)} placeholder={t("skillsSearchPlaceholder", "搜索 Skill 名称或描述")} className="h-10 w-full rounded-lg border px-3 text-sm lg:max-w-sm" />
            <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
              <button type="button" onClick={() => state.setRepositorySourceFilter("all")} className={`rounded-md px-3 py-1.5 text-sm ${state.repositorySourceFilter === "all" ? "bg-black text-white" : "bg-white text-black"}`}>{t("all", "全部")}</button>
              <button type="button" onClick={() => state.setRepositorySourceFilter("remote")} className={`rounded-md px-3 py-1.5 text-sm ${state.repositorySourceFilter === "remote" ? "bg-black text-white" : "bg-white text-black"}`}>{t("skillsSourceTypeRemote", "推荐源")}</button>
              <button type="button" onClick={() => state.setRepositorySourceFilter("local")} className={`rounded-md px-3 py-1.5 text-sm ${state.repositorySourceFilter === "local" ? "bg-black text-white" : "bg-white text-black"}`}>{t("skillsSourceTypeLocalImport", "本地导入")}</button>
            </div>
          </div>

          {state.visibleRepository.length === 0 ? (
            <div className="py-12 text-center">
              <BookOpen className="mx-auto mb-4 h-16 w-16 text-muted-foreground" />
              <h3 className="mb-2 text-lg font-semibold">{t("skillsRepositoryEmpty", "仓库中暂无 Skills")}</h3>
              <p className="text-muted-foreground">{t("skillsRepositoryEmptyDesc", "请先在左侧 Skills 页面同步来源或导入仓库。")}</p>
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
              {state.visibleRepository.map((repo) => {
                const Icon = pickIcon(repo.icon_seed || repo.skill_id);
                const installed = repo.installed[state.activeModel];
                const sourceTypeLabel = getRepositorySourceTypeLabel(repo.source_type, "skills", t);
                return (
                  <div key={repo.repo_key} className="cursor-pointer rounded-xl border bg-card p-4 transition-all duration-200 hover:border-primary/30 hover:shadow-md" onClick={() => { void state.handleOpenRepositoryDetail(repo); }}>
                    <div className="flex items-start justify-between gap-3">
                      <div className="rounded-md bg-muted p-2 text-foreground">
                        <Icon className="h-4 w-4" />
                      </div>
                      <span className="rounded border bg-muted/50 px-2 py-1 text-[10px] text-muted-foreground">{sourceTypeLabel}</span>
                    </div>
                    <h4 className="mt-3 text-sm font-semibold">{repo.name}</h4>
                    <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{repo.description}</p>
                    <div className="mt-3 flex items-center justify-between gap-2">
                      <span className="text-[10px] text-muted-foreground">{repo.dir_name || repo.source_rel_path.split("/").pop() || repo.skill_id}</span>
                      {installed ? (
                        <span className="inline-flex items-center gap-1 rounded-md border px-2.5 py-1 text-xs text-muted-foreground">
                          <Download className="h-3.5 w-3.5" />
                          {t("installed", "Installed")}
                        </span>
                      ) : (
                        <button
                          type="button"
                          onClick={(event) => {
                            event.stopPropagation();
                            state.openInstallDialog({
                              source_id: repo.source_id,
                              id: repo.skill_id,
                              rel_path: repo.source_rel_path,
                              dir_name: repo.dir_name,
                              name: repo.name,
                              description: repo.description,
                              models: repo.models,
                              repo_key: repo.repo_key,
                            }, "repository");
                          }}
                          className="inline-flex items-center gap-1 rounded-md bg-primary px-2.5 py-1 text-xs text-primary-foreground"
                        >
                          <Download className="h-3.5 w-3.5" />
                          {t("workspaceInstallAction", "Install to Workspace")}
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </>
      )}
    </>
  );
}

export function WorkspaceSkillsDetailDialogs(props: {
  t: any;
  state: ReturnType<typeof useWorkspaceSkillsPanelState>;
}) {
  const { t, state } = props;
  return (
    <>
      <Dialog open={state.catalogDetailOpen} onOpenChange={(open) => {
        if (!open) {
          state.closeCatalogDetail();
          return;
        }
        state.setCatalogDetailOpen(open);
      }}>
        {state.catalogDetailOpen && state.catalogDetailData && (
          <DialogContent className="max-w-4xl h-[85vh] max-h-[85vh] p-0 gap-0 overflow-hidden grid-rows-[auto,minmax(0,1fr),auto]">
            <DialogHeader className="border-b px-6 pt-6 pb-4">
              <DialogTitle>{state.catalogDetailData.skill.name}</DialogTitle>
              <DialogDescription>{state.catalogDetailData.skill.description}</DialogDescription>
            </DialogHeader>
            <div className="min-h-0 overflow-auto px-6 py-4">
              <div className="prose prose-sm max-w-none rounded-md border p-4 dark:prose-invert">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{state.catalogDetailData.markdown || ""}</ReactMarkdown>
              </div>
            </div>
            <DialogFooter className="flex items-center gap-2 border-t px-6 py-4">
              <button type="button" onClick={() => { void state.handleOpenCatalogFolder(); }} className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm hover:bg-muted disabled:opacity-50" disabled={state.loading}>
                <FolderOpen className="h-4 w-4" />
                {t("openFolder", "Open Folder")}
              </button>
              {state.hasInstallableRepoModels(state.catalogDetailInstallTarget) && (
                <button type="button" onClick={state.handleInstallFromCatalogDetail} className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50" disabled={state.loading}>
                  <Download className="h-4 w-4" />
                  {t("workspaceInstallAction", "Install to Workspace")}
                </button>
              )}
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>

      <Dialog open={state.detailOpen} onOpenChange={state.setDetailOpen}>
        {state.detailOpen && state.detailData && (
          <DialogContent className="max-w-4xl h-[85vh] max-h-[85vh] p-0 gap-0 overflow-hidden grid-rows-[auto,minmax(0,1fr),auto]">
            <DialogHeader className="border-b px-6 pt-6 pb-4">
              <DialogTitle>{state.detailData.skill.name}</DialogTitle>
              <DialogDescription>{state.detailData.skill.description}</DialogDescription>
            </DialogHeader>
            <div className="min-h-0 overflow-auto px-6 py-4">
              <div className="prose prose-sm max-w-none rounded-md border p-4 dark:prose-invert">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{state.detailData.markdown || ""}</ReactMarkdown>
              </div>
            </div>
            <DialogFooter className="border-t px-6 py-4">
              <button type="button" onClick={() => { if (state.detailData) void state.handleOpenFolder(state.detailData.skill); }} className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm hover:bg-muted">
                <FolderOpen className="h-4 w-4" />
                {t("openFolder", "Open Folder")}
              </button>
              {state.detailData.skill.scope === "global" ? (
                <button type="button" onClick={() => { state.setDetailOpen(false); state.onNavigateToGlobalPage?.("installed"); }} className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm hover:bg-muted">
                  <BookOpen className="h-4 w-4" />
                  {t("workspaceManageUserLevel", "Manage User-level")}
                </button>
              ) : (
                <button type="button" onClick={() => { if (state.detailData) void state.handleUninstall(state.detailData.skill); }} className="inline-flex items-center gap-2 rounded-md border px-4 py-2 text-sm text-destructive hover:bg-destructive/10">
                  <Trash2 className="h-4 w-4" />
                  {t("uninstall", "Uninstall")}
                </button>
              )}
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>
    </>
  );
}

export function WorkspaceSkillsInstallDialog(props: {
  t: any;
  state: ReturnType<typeof useWorkspaceSkillsPanelState>;
}) {
  const { t, state } = props;
  return (
    <Dialog open={state.installDialogOpen} onOpenChange={(open) => {
      if (state.installSubmitting && !open) return;
      state.setInstallDialogOpen(open);
      if (!open) {
        state.setInstallTarget(null);
        state.setInstallModels([]);
        state.setInstallError("");
      }
    }}>
      {state.installDialogOpen && state.installTarget && (
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("workspaceInstallAction", "Install to Workspace")}</DialogTitle>
            <DialogDescription>
              {t("workspaceTargetDirectory", "Target directory")}: {state.normalizedRootPath}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-2">
              <div className="text-sm font-medium">{state.installTarget.name}</div>
              <div className="text-xs text-muted-foreground">{state.installTarget.description}</div>
            </div>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              {state.modelTabs.map((tab) => {
                const allowed = state.installTarget?.models.includes(tab.id);
                const ModelIcon = modelIconMap[tab.id];
                const selected = state.installModels.includes(tab.id);
                return (
                  <button
                    key={`workspace-skills-install-${tab.id}`}
                    type="button"
                    disabled={!allowed}
                    onClick={() => state.toggleInstallModel(tab.id)}
                    className={`flex items-center gap-3 rounded-xl border px-4 py-3 text-left transition-colors ${
                      selected ? "border-primary bg-primary/10 text-primary" : "hover:bg-muted"
                    } ${!allowed ? "cursor-not-allowed opacity-40" : ""}`}
                  >
                    <ModelIcon className="h-5 w-5" />
                    <div className="min-w-0 flex-1">
                      <div className="font-medium">{tab.label}</div>
                      <div className="mt-1 text-xs text-muted-foreground">
                        {!allowed
                          ? t("skillsInstallUnavailableForModel", "This skill is not available for the selected model.")
                          : selected
                            ? t("selected", "Selected")
                            : t("clickToSelect", "Click to select")}
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
            {state.installError && <p className="text-sm text-destructive">{state.installError}</p>}
          </div>
          <DialogFooter>
            <button type="button" onClick={() => state.setInstallDialogOpen(false)} className="rounded-md border px-4 py-2 text-sm hover:bg-muted" disabled={state.installSubmitting}>
              {t("cancel", "Cancel")}
            </button>
            <button type="button" onClick={() => { void state.handleInstallConfirm(); }} className="inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50" disabled={state.installSubmitting}>
              {state.installSubmitting && <Loader2 className="h-4 w-4 animate-spin" />}
              {t("workspaceInstallAction", "Install to Workspace")}
            </button>
          </DialogFooter>
        </DialogContent>
      )}
    </Dialog>
  );
}
