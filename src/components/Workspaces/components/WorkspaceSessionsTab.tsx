import { Play } from "lucide-react";
import { AiSessionsList, type AiSessionListItem, type AiSessionsQueryState } from "../../AiSessionsList";
import type { TFunction } from "i18next";

export function WorkspaceSessionsTab(args: {
  t: TFunction;
  activeSessions: AiSessionListItem[];
  sessionsLoading: boolean;
  sessionsInitialized: boolean;
  sessionQuery: AiSessionsQueryState;
  sessionsTotal: number;
  sessionToolOptions: string[];
  sessionModelOptions: string[];
  onQueryChange: (query: AiSessionsQueryState) => void;
  onLaunch: (session: AiSessionListItem) => void;
  onDelete: (sessionId: string) => void;
  onRename: (session: AiSessionListItem, nextName: string) => void;
  onFavoriteChange: (session: AiSessionListItem, favorite: boolean) => void;
  onQuickLaunch: () => void;
}) {
  const {
    t,
    activeSessions,
    sessionsLoading,
    sessionsInitialized,
    sessionQuery,
    sessionsTotal,
    sessionToolOptions,
    sessionModelOptions,
    onQueryChange,
    onLaunch,
    onDelete,
    onRename,
    onFavoriteChange,
    onQuickLaunch,
  } = args;

  return (
    <>
      <div className="space-y-4 pb-24">
        <div className="rounded-xl border bg-card p-4">
          <div className="flex flex-col gap-3">
            <div>
              <h3 className="text-lg font-semibold tracking-tight">{t("terminalSessions", "Terminal Sessions")}</h3>
              <p className="mt-1 text-sm text-muted-foreground">
                {t(
                  "workspaceSessionsSectionDesc",
                  "Filter terminal sessions for this workspace by tool, model, or name, then continue, rename, or remove them without leaving the current context.",
                )}
              </p>
            </div>
          </div>
        </div>
        <AiSessionsList
          sessions={activeSessions}
          loading={sessionsLoading || !sessionsInitialized}
          queryState={sessionQuery}
          onQueryChange={onQueryChange}
          serverFiltered
          totalSessions={sessionsTotal}
          availableToolOptions={sessionToolOptions}
          availableModelOptions={sessionModelOptions}
          onLaunch={onLaunch}
          onDelete={onDelete}
          onRename={onRename}
          onFavoriteChange={onFavoriteChange}
        />
      </div>

      <button
        type="button"
        onClick={onQuickLaunch}
        className="fixed bottom-4 right-4 z-40 inline-flex h-12 items-center gap-2 rounded-full bg-primary px-4 text-sm font-medium text-primary-foreground shadow-lg shadow-primary/20 transition-all hover:-translate-y-0.5 hover:bg-primary/90 hover:shadow-xl sm:bottom-6 sm:right-6"
        title={t("workspaceQuickLaunch", "New AI Session")}
      >
        <Play className="h-4 w-4" />
        {t("workspaceQuickLaunch", "New AI Session")}
      </button>
    </>
  );
}
