import { Copy, FolderOpen, Loader2, Pencil, Play, Tag, Trash2 } from "lucide-react";
import { compactWorkspaceRootPath, formatTs, getSourceBadgeClassName, getSourceBadgeDescription, getSourceBadgeLabel, getSourceBadgeTranslationKeys } from "../helpers/workspaceHelpers";
import type { WorkspaceRecord, WorkspaceView } from "../types";
import type { TFunction } from "i18next";

export function WorkspaceListSection(args: {
  t: TFunction;
  loading: boolean;
  workspacesInitialized: boolean;
  workspaces: WorkspaceView[];
  allTags: string[];
  selectedTags: string[];
  selectedWorkspaceTags: Set<string>;
  visibleWorkspaces: WorkspaceView[];
  onClearTags: () => void;
  onToggleTag: (tag: string) => void;
  onSelectWorkspace: (workspaceId: string, view: WorkspaceView) => void;
  onEditWorkspace: (workspace: WorkspaceRecord) => void;
  onCopyWorkspace: (workspace: WorkspaceRecord) => void;
  onDeleteWorkspace: (workspace: WorkspaceRecord) => void;
  onLaunchWorkspace: (workspace: WorkspaceRecord) => void;
}) {
  const {
    t,
    loading,
    workspacesInitialized,
    workspaces,
    allTags,
    selectedTags,
    selectedWorkspaceTags,
    visibleWorkspaces,
    onClearTags,
    onToggleTag,
    onSelectWorkspace,
    onEditWorkspace,
    onCopyWorkspace,
    onDeleteWorkspace,
    onLaunchWorkspace,
  } = args;

  return (
    <>
      <div className="rounded-xl border bg-card p-4">
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={onClearTags}
            className={`rounded-full border px-3 py-1 text-xs transition-colors ${
              selectedTags.length === 0 ? "border-primary bg-primary/10 text-primary" : "hover:bg-muted"
            }`}
          >
            {t("all", "All")}
          </button>
          {allTags.map((tag) => (
            <button
              key={tag}
              type="button"
              onClick={() => onToggleTag(tag)}
              className={`inline-flex items-center gap-1 rounded-full border px-3 py-1 text-xs transition-colors ${
                selectedWorkspaceTags.has(tag) ? "border-primary bg-primary/10 text-primary" : "hover:bg-muted"
              }`}
            >
              <Tag className="h-3 w-3" />
              {tag}
            </button>
          ))}
        </div>
        <div className="mt-3 text-xs text-muted-foreground">
          {t("workspaceCountSummary", "Showing {{count}} workspaces", { count: visibleWorkspaces.length })}
        </div>
      </div>

      {!workspacesInitialized || (loading && workspaces.length === 0) ? (
        <div className="flex flex-1 flex-col items-center justify-center rounded-xl border bg-card p-8 text-center text-muted-foreground">
          <Loader2 className="mb-4 h-10 w-10 animate-spin opacity-70" />
          <p className="text-sm">{t("loading", "Loading...")}</p>
        </div>
      ) : visibleWorkspaces.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center rounded-xl border bg-card p-8 text-center text-muted-foreground">
          <FolderOpen className="mb-4 h-12 w-12 opacity-30" />
          <p className="text-base font-medium text-foreground">{t("workspaceEmptyTitle", "No workspaces yet")}</p>
          <p className="mt-2 max-w-xl text-sm">
            {t(
              "workspaceEmptyDesc",
              "Create a workspace manually, or let AI terminal session sync create them automatically from working directories.",
            )}
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
          {visibleWorkspaces.map((item) => {
            const workspace = item.workspace;
            const lastActiveText = formatTs(workspace.last_activity_at);
            const compactRootPath = compactWorkspaceRootPath(workspace.root_path);
            const sourceBadgeKeys = getSourceBadgeTranslationKeys(workspace.source);
            const sourceBadgeLabel = t(sourceBadgeKeys.label, getSourceBadgeLabel(workspace.source));
            const sourceBadgeDescription = t(
              sourceBadgeKeys.description,
              getSourceBadgeDescription(workspace.source),
            );
            return (
              <div
                key={workspace.id}
                role="button"
                tabIndex={0}
                onClick={() => onSelectWorkspace(workspace.id, item)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    onSelectWorkspace(workspace.id, item);
                  }
                }}
                className="group flex h-full flex-col rounded-xl border bg-card p-4 text-left transition-all hover:-translate-y-0.5 hover:border-primary/35 hover:shadow-sm"
              >
                <div className="flex items-start justify-between gap-2.5">
                  <div className="min-w-0 flex-1">
                    <div className="inline-flex max-w-full items-start gap-1.5">
                      <span className="min-w-0 shrink truncate text-base font-semibold leading-tight">{workspace.name}</span>
                      <span
                        className={`shrink-0 rounded-full border px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wide ${getSourceBadgeClassName(workspace.source)}`}
                        title={`${sourceBadgeLabel}: ${sourceBadgeDescription}`}
                      >
                        {sourceBadgeLabel}
                      </span>
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-1 self-start">
                    <button
                      type="button"
                      onClick={(event) => {
                        event.stopPropagation();
                        onEditWorkspace(workspace);
                      }}
                      className="rounded-md p-1.5 text-muted-foreground/80 transition-colors hover:bg-muted hover:text-foreground"
                      title={t("edit", "Edit")}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </button>
                    <button
                      type="button"
                      onClick={(event) => {
                        event.stopPropagation();
                        onCopyWorkspace(workspace);
                      }}
                      className="rounded-md p-1.5 text-amber-600/90 transition-colors hover:bg-amber-500/10 hover:text-amber-700 dark:text-amber-300 dark:hover:text-amber-200"
                      title={t("copy", "Copy")}
                    >
                      <Copy className="h-3.5 w-3.5" />
                    </button>
                    <button
                      type="button"
                      onClick={(event) => {
                        event.stopPropagation();
                        onDeleteWorkspace(workspace);
                      }}
                      className="rounded-md p-1.5 text-destructive/80 transition-colors hover:bg-destructive/10 hover:text-destructive"
                      title={t("delete", "Delete")}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </div>

                <div
                  className="mt-2 w-full truncate rounded-md bg-muted/40 px-2.5 py-1.5 text-[11px] leading-4 text-muted-foreground"
                  title={workspace.root_path}
                >
                  {compactRootPath}
                </div>

                <p className="mt-3 line-clamp-2 min-h-[36px] text-[13px] leading-5 text-muted-foreground">
                  {workspace.description?.trim() || t("workspaceNoDescription", "No description yet.")}
                </p>

                <div className="mt-3 flex flex-wrap gap-1.5">
                  {(workspace.tags || []).length > 0 ? (
                    workspace.tags.map((tag) => (
                      <span
                        key={`${workspace.id}-${tag}`}
                        className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[10px] leading-4 text-muted-foreground"
                      >
                        <Tag className="h-2.5 w-2.5" />
                        {tag}
                      </span>
                    ))
                  ) : (
                    <span className="rounded-full border border-dashed px-2 py-0.5 text-[10px] leading-4 text-muted-foreground">
                      {t("workspaceNoTags", "No tags")}
                    </span>
                  )}
                </div>

                <div className="mt-3 flex items-end justify-between gap-3">
                  <div className="grid min-w-0 flex-1 grid-cols-2 gap-1.5 rounded-lg bg-muted/35 p-2">
                    <div className="min-w-0">
                      <div className="text-[10px] uppercase tracking-wide text-muted-foreground/80">
                        {t("sessions", "Sessions")}
                      </div>
                      <div className="mt-0.5 truncate text-[11px] font-medium text-foreground">
                        {t("workspaceSessionsCount", "{{count}} sessions", { count: item.session_count })}
                      </div>
                    </div>
                    <div className="min-w-0">
                      <div className="text-[10px] uppercase tracking-wide text-muted-foreground/80">
                        {t("workspaceLastActive", "Last active")}
                      </div>
                      <div className="mt-0.5 truncate text-[11px] font-medium text-foreground" title={lastActiveText}>
                        {lastActiveText}
                      </div>
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={(event) => {
                      event.stopPropagation();
                      onLaunchWorkspace(workspace);
                    }}
                    className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md bg-primary px-2.5 text-xs font-medium text-primary-foreground hover:bg-primary/90"
                  >
                    <Play className="h-3.5 w-3.5" />
                    {t("workspaceQuickLaunch", "New AI Session")}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}
