import {
  ArrowDown,
  ArrowUp,
  Edit,
  GripVertical,
  Pin,
  PinOff,
  Trash2,
} from "lucide-react";
import type { TFunction } from "i18next";
import { launcherIcon } from "../helpers";
import type {
  LauncherDragState,
  LauncherItem,
  LauncherQuickToolEntry,
} from "../types";

type LauncherGridProps = {
  t: TFunction;
  loading: boolean;
  searchTerm: string;
  groupedItems: Array<{ type: LauncherItem["type"]; items: LauncherItem[] }>;
  quickInternalTools: LauncherQuickToolEntry[];
  drag: LauncherDragState;
  typeLabelMap: Record<LauncherItem["type"], string>;
  pinnedOrderIds: string[];
  appIconCache: Record<string, string | null>;
  appIconCacheKey: (target: string) => string;
  formatRelativeTime: (ts?: number) => string;
  onOpenInternalTarget: (target: string) => void;
  onLaunch: (item: LauncherItem) => void;
  onTogglePin: (item: LauncherItem, e: React.MouseEvent) => void;
  onMovePinned: (
    itemId: string,
    direction: "up" | "down",
    e: React.MouseEvent,
  ) => void;
  onEdit: (item: LauncherItem) => void;
  onDelete: (item: LauncherItem, e: React.MouseEvent) => void;
};

export function LauncherGrid({
  t,
  loading,
  searchTerm,
  groupedItems,
  quickInternalTools,
  drag,
  typeLabelMap,
  pinnedOrderIds,
  appIconCache,
  appIconCacheKey,
  formatRelativeTime,
  onOpenInternalTarget,
  onLaunch,
  onTogglePin,
  onMovePinned,
  onEdit,
  onDelete,
}: LauncherGridProps) {
  return (
    <>
      {loading ? (
        <div className="text-sm text-muted-foreground">
          {t("loading", "Loading...")}
        </div>
      ) : groupedItems.length === 0 && quickInternalTools.length === 0 ? (
        <div className="text-sm text-muted-foreground">
          {t("noResultsFound", "No results found.")}
        </div>
      ) : (
        <div className="space-y-6">
          {quickInternalTools.length > 0 ? (
            <section className="space-y-3">
              <div>
                <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  {t("launcherInternalTools", "Internal Tools")}
                </h3>
                <p className="mt-1 text-sm text-muted-foreground">
                  {t(
                    "launcherInternalToolsDesc",
                    "Keep internal utilities close at hand without expanding the sidebar.",
                  )}
                </p>
              </div>

              <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
                {quickInternalTools.map((item) => {
                  const Icon = item.icon;
                  const isDragging = drag.draggingId === item.id;
                  const showHandle = searchTerm.trim() === "";

                  return (
                    <div key={item.id} className="relative">
                      <button
                        type="button"
                        onClick={() => onOpenInternalTarget(item.target)}
                        onPointerOver={() => drag.handleCardPointerOver(item.id)}
                        data-testid={`launcher-internal-tool-card-${item.id}`}
                        className={`group flex min-h-36 w-full flex-col justify-between rounded-xl border border-hairline bg-card p-4 text-left shadow-subtle transition-all hover-lift hover:border-foreground/30 hover:shadow-md ${
                          isDragging ? "ring-2 ring-primary" : ""
                        }`}
                      >
                        <div className="flex items-start justify-between gap-3">
                          <div
                            className={`rounded-lg p-2 ${item.iconClassName}`}
                            data-testid={`launcher-tool-icon-${item.target}`}
                          >
                            <Icon className="h-6 w-6" />
                          </div>
                          <div
                            className={`flex flex-col items-end gap-2 ${
                              showHandle ? "pr-7" : ""
                            }`}
                          >
                            {item.statusBadge}
                          </div>
                        </div>
                        <div className="space-y-1">
                          <div className="font-semibold">{item.name}</div>
                          <p className="text-sm leading-6 text-muted-foreground">
                            {item.description}
                          </p>
                        </div>
                      </button>
                      {showHandle ? (
                        <button
                          type="button"
                          aria-label={t(
                            "launcherDragToReorderHint",
                            "Drag cards to reorder",
                          )}
                          title={t(
                            "launcherDragToReorderHint",
                            "Drag cards to reorder",
                          )}
                          onPointerDown={(e) =>
                            drag.handlePointerDown(e, item.id)
                          }
                          onPointerUp={drag.handlePointerUp}
                          data-testid={`launcher-tool-drag-handle-${item.id}`}
                          className="absolute right-2 top-2 cursor-grab touch-none rounded-md p-1 text-muted-foreground hover:text-foreground"
                        >
                          <GripVertical className="h-4 w-4" />
                        </button>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            </section>
          ) : null}

          {groupedItems.map((group) => (
            <section key={group.type} className="space-y-3">
              <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                {typeLabelMap[group.type]}
              </h3>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
                {group.items.map((item) => {
                  const Icon = launcherIcon(item.type);
                  const pinnedIndex = pinnedOrderIds.findIndex(
                    (id) => id === item.id,
                  );
                  const isPinned = item.pinned;
                  const lastUsed = item.last_launched_at
                    ? formatRelativeTime(item.last_launched_at)
                    : t("neverLaunched", "Never launched");
                  const appIconDataUrl =
                    item.type === "app"
                      ? appIconCache[appIconCacheKey(item.target)]
                      : null;

                  return (
                    <div
                      key={item.id}
                      onClick={() => onLaunch(item)}
                      className="group flex flex-col justify-between p-4 rounded-xl border border-hairline bg-card text-card-foreground shadow-subtle hover-lift hover:shadow-md transition-all hover:border-foreground/30 cursor-pointer min-h-40"
                    >
                      <div className="flex justify-between items-start gap-2">
                        <div
                          className={`p-2 rounded-lg ${item.type === "app" ? "bg-blue-500/10 text-blue-500" : item.type === "internal" ? "bg-emerald-500/10 text-emerald-500" : "bg-primary/10 text-primary"}`}
                        >
                          {item.type === "app" && appIconDataUrl ? (
                            <img
                              src={appIconDataUrl}
                              alt={`${item.name} icon`}
                              className="w-7 h-7 rounded-sm object-contain"
                            />
                          ) : (
                            <Icon className="w-7 h-7" />
                          )}
                        </div>

                        <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-all">
                          <button
                            onClick={(e) => onTogglePin(item, e)}
                            className="text-muted-foreground hover:text-foreground p-1 rounded-md"
                            title={
                              isPinned ? t("unpin", "Unpin") : t("pin", "Pin")
                            }
                          >
                            {isPinned ? (
                              <PinOff className="w-4 h-4" />
                            ) : (
                              <Pin className="w-4 h-4" />
                            )}
                          </button>
                          {isPinned && (
                            <>
                              <button
                                onClick={(e) =>
                                  onMovePinned(item.id, "up", e)
                                }
                                disabled={pinnedIndex <= 0}
                                className="text-muted-foreground hover:text-foreground p-1 rounded-md disabled:opacity-40"
                                title={t("moveUp", "Move Up")}
                              >
                                <ArrowUp className="w-4 h-4" />
                              </button>
                              <button
                                onClick={(e) =>
                                  onMovePinned(item.id, "down", e)
                                }
                                disabled={
                                  pinnedIndex < 0 ||
                                  pinnedIndex >= pinnedOrderIds.length - 1
                                }
                                className="text-muted-foreground hover:text-foreground p-1 rounded-md disabled:opacity-40"
                                title={t("moveDown", "Move Down")}
                              >
                                <ArrowDown className="w-4 h-4" />
                              </button>
                            </>
                          )}
                          <button
                            onClick={(e) => {
                              e.stopPropagation();
                              onEdit(item);
                            }}
                            className="text-muted-foreground hover:text-foreground p-1 rounded-md"
                            title={t("edit", "Edit")}
                          >
                            <Edit className="w-4 h-4" />
                          </button>
                          <button
                            onClick={(e) => onDelete(item, e)}
                            className="text-muted-foreground hover:text-destructive p-1 rounded-md"
                            title={t("delete", "Delete")}
                          >
                            <Trash2 className="w-4 h-4" />
                          </button>
                        </div>
                      </div>

                      <div className="mt-3 space-y-1">
                        <div className="flex items-center gap-2">
                          <h3 className="font-semibold truncate">
                            {item.name}
                          </h3>
                          {isPinned && (
                            <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-primary/10 text-primary">
                              {t("pinned", "Pinned")}
                            </span>
                          )}
                        </div>
                        <p className="text-xs text-muted-foreground truncate font-mono opacity-80">
                          {item.target}
                        </p>
                        <p className="text-xs text-muted-foreground">
                          {t("launchCount", "Launches")}: {item.launch_count} ·{" "}
                          {t("lastUsed", "Last used")}: {lastUsed}
                        </p>
                      </div>
                    </div>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      )}
    </>
  );
}
