import { ArrowLeft, Check, Copy, FolderOpen, Pencil, Tag } from "lucide-react";
import { formatTs, getSourceBadgeClassName } from "../helpers/workspaceHelpers";
import type { WorkspaceDetail, WorkspaceRecord } from "../types";
import type { TFunction } from "i18next";

export function WorkspaceDetailHeader(args: {
  t: TFunction;
  activeWorkspace: WorkspaceRecord;
  activeDetail: WorkspaceDetail | null;
  copiedRootPath: boolean;
  sourceBadgeLabel: string;
  sourceBadgeDescription: string;
  onBack: () => void;
  onEdit: () => void;
  onCopyConfig: () => void;
  onCopyRootPath: () => void;
}) {
  const { t, activeWorkspace, activeDetail, copiedRootPath, sourceBadgeLabel, sourceBadgeDescription, onBack, onEdit, onCopyConfig, onCopyRootPath } = args;

  return (
    <div className="rounded-2xl border bg-card p-3.5">
      <div className="flex flex-col gap-3">
        <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2 md:gap-3">
              <button
                type="button"
                onClick={onBack}
                className="inline-flex h-8 items-center gap-1.5 rounded-md border border-primary/30 bg-primary/10 px-2.5 text-xs font-medium text-primary transition-colors hover:border-primary/40 hover:bg-primary/15"
              >
                <ArrowLeft className="h-3.5 w-3.5" />
                {t("back", "Back")}
              </button>
              <h3 className="min-w-0 truncate text-lg font-semibold tracking-tight">{activeWorkspace.name}</h3>
              <span
                className={`rounded-full border px-2 py-0.5 text-[10px] ${getSourceBadgeClassName(activeWorkspace.source)}`}
                title={`${sourceBadgeLabel}: ${sourceBadgeDescription}`}
              >
                {sourceBadgeLabel}
              </span>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2 md:justify-end">
            <button
              type="button"
              onClick={onEdit}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border px-2.5 text-xs hover:bg-muted"
            >
              <Pencil className="h-3.5 w-3.5" />
              {t("edit", "Edit")}
            </button>
            <button
              type="button"
              onClick={onCopyConfig}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border border-amber-500/30 bg-amber-500/10 px-2.5 text-xs font-medium text-amber-700 transition-colors hover:bg-amber-500/15 dark:border-amber-400/30 dark:bg-amber-400/10 dark:text-amber-300 dark:hover:bg-amber-400/15"
            >
              <Copy className="h-3.5 w-3.5" />
              {t("workspaceCopyAction", "Copy Config")}
            </button>
          </div>
        </div>
        <div className="group/rootpath flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <FolderOpen className="h-3.5 w-3.5 shrink-0" />
          <div className="min-w-0 flex-1">
            <div className="inline-flex max-w-full items-center gap-1 overflow-hidden align-middle">
              <div className="truncate" title={activeWorkspace.root_path}>
                {activeWorkspace.root_path}
              </div>
              {copiedRootPath ? (
                <Check className="h-3.5 w-3.5 shrink-0 text-green-600" aria-label={t("copied", "Copied!")} />
              ) : (
                <button
                  type="button"
                  onClick={onCopyRootPath}
                  className="shrink-0 rounded-md p-0.5 text-muted-foreground opacity-0 transition-all hover:bg-muted hover:text-foreground focus:opacity-100 group-hover/rootpath:opacity-100"
                  title={t("copyPath", "Copy path")}
                  aria-label={t("copyPath", "Copy path")}
                >
                  <Copy className="h-3.5 w-3.5" />
                </button>
              )}
            </div>
          </div>
        </div>

        {(activeWorkspace.description || (activeWorkspace.tags || []).length > 0 || activeDetail) && (
          <div className="space-y-2">
            {activeWorkspace.description && <p className="line-clamp-1 text-xs text-muted-foreground">{activeWorkspace.description}</p>}
            <div className="flex flex-wrap items-center gap-1.5">
              {(activeWorkspace.tags || []).length > 0 ? (
                (activeWorkspace.tags || []).map((tag) => (
                  <span key={`detail-tag-${tag}`} className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground">
                    <Tag className="h-3 w-3" />
                    {tag}
                  </span>
                ))
              ) : (
                <span className="rounded-full border border-dashed px-2 py-0.5 text-[10px] text-muted-foreground">
                  {t("workspaceNoTags", "No tags")}
                </span>
              )}
              <span className="rounded-full border px-2 py-0.5 text-[10px] text-muted-foreground">
                {t("workspaceSessionsCount", "{{count}} sessions", { count: activeDetail?.workspace.session_count || 0 })}
              </span>
              <span className="rounded-full border px-2 py-0.5 text-[10px] text-muted-foreground">
                {t("workspaceCreatedAt", "Created")}: {formatTs(activeWorkspace.created_at)}
              </span>
              <span className="rounded-full border px-2 py-0.5 text-[10px] text-muted-foreground">
                {t("workspaceLastActive", "Last active")}: {formatTs(activeWorkspace.last_activity_at)}
              </span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
