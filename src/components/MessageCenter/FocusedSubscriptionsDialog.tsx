import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertCircle,
  AlertTriangle,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Info,
  RotateCcw,
  Search,
  Sparkles,
  Star,
  X,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import {
  FOCUSED_MESSAGE_TYPE_CATALOG,
  getDefaultFocusedSubscriptionConfig,
  type FocusedMessageSubscriptionConfig,
  type FocusedMessageTypeDef,
} from "@/lib/messageSubscriptions";
import { getSourceLabel } from "./types";
import type { MessageSeverity } from "@/lib/messages";

interface FocusedSubscriptionsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  config: FocusedMessageSubscriptionConfig;
  onSave: (config: FocusedMessageSubscriptionConfig) => void;
}

function renderSeverityBadge(severity: MessageSeverity, t: (key: string, fallback?: string) => string) {
  if (severity === "error") {
    return (
      <span className="inline-flex items-center gap-1 rounded bg-rose-500/10 px-1.5 py-0.5 text-[10px] font-medium text-rose-600 dark:text-rose-400">
        <AlertCircle className="h-3 w-3" />
        {t("severityError", "Error")}
      </span>
    );
  }
  if (severity === "warning") {
    return (
      <span className="inline-flex items-center gap-1 rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium text-amber-600 dark:text-amber-400">
        <AlertTriangle className="h-3 w-3" />
        {t("severityWarning", "Warning")}
      </span>
    );
  }
  if (severity === "success") {
    return (
      <span className="inline-flex items-center gap-1 rounded bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
        <CheckCircle2 className="h-3 w-3" />
        {t("severitySuccess", "Success")}
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 rounded bg-sky-500/10 px-1.5 py-0.5 text-[10px] font-medium text-sky-600 dark:text-sky-400">
      <Info className="h-3 w-3" />
      {t("severityInfo", "Info")}
    </span>
  );
}

export function FocusedSubscriptionsDialog({
  open,
  onOpenChange,
  config,
  onSave,
}: FocusedSubscriptionsDialogProps) {
  const { t } = useTranslation();
  const tHelper = useCallback(
    (key: string, fallback?: string) => t(key, fallback ?? key),
    [t],
  );

  const [draftSubscribedIds, setDraftSubscribedIds] = useState<Set<string>>(
    () => new Set(config.subscribedTypeIds),
  );
  const [draftPreferences, setDraftPreferences] = useState(config.preferences);
  const [searchQuery, setSearchQuery] = useState("");
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(new Set());

  // 按模块 source 分组
  const groupedTypes = useMemo(() => {
    const groups: Record<string, FocusedMessageTypeDef[]> = {};
    for (const item of FOCUSED_MESSAGE_TYPE_CATALOG) {
      if (!groups[item.source]) {
        groups[item.source] = [];
      }
      groups[item.source].push(item);
    }
    return groups;
  }, []);

  // 搜索过滤后的分组
  const filteredGroupedTypes = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    const result: Record<string, FocusedMessageTypeDef[]> = {};

    for (const [source, items] of Object.entries(groupedTypes)) {
      const sourceName = getSourceLabel(source, tHelper).toLowerCase();
      const matched = items.filter((item) => {
        if (!query) return true;
        const name = t(item.nameKey, item.category).toLowerCase();
        const desc = t(item.descKey, "").toLowerCase();
        return (
          sourceName.includes(query) ||
          item.category.toLowerCase().includes(query) ||
          name.includes(query) ||
          desc.includes(query)
        );
      });
      if (matched.length > 0) {
        result[source] = matched;
      }
    }
    return result;
  }, [groupedTypes, searchQuery, t, tHelper]);

  const totalTypesCount = FOCUSED_MESSAGE_TYPE_CATALOG.length;
  const currentEnabledCount = draftSubscribedIds.size;

  const toggleSubscription = (id: string) => {
    setDraftSubscribedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const toggleGroupCollapse = (source: string) => {
    setCollapsedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(source)) {
        next.delete(source);
      } else {
        next.add(source);
      }
      return next;
    });
  };

  const toggleGroupAll = (_source: string, items: FocusedMessageTypeDef[]) => {
    const allEnabled = items.every((i) => draftSubscribedIds.has(i.id));
    setDraftSubscribedIds((prev) => {
      const next = new Set(prev);
      for (const item of items) {
        if (allEnabled) {
          next.delete(item.id);
        } else {
          next.add(item.id);
        }
      }
      return next;
    });
  };

  const applyRecommended = () => {
    const recommendedIds = FOCUSED_MESSAGE_TYPE_CATALOG
      .filter((item) => item.isRecommended)
      .map((item) => item.id);
    setDraftSubscribedIds(new Set(recommendedIds));
  };

  const enableAll = () => {
    const allIds = FOCUSED_MESSAGE_TYPE_CATALOG.map((item) => item.id);
    setDraftSubscribedIds(new Set(allIds));
  };

  const disableAll = () => {
    setDraftSubscribedIds(new Set());
  };

  const resetToDefault = () => {
    const defaults = getDefaultFocusedSubscriptionConfig();
    setDraftSubscribedIds(new Set(defaults.subscribedTypeIds));
    setDraftPreferences(defaults.preferences);
  };

  const handleSave = () => {
    onSave({
      ...config,
      subscribedTypeIds: Array.from(draftSubscribedIds),
      preferences: draftPreferences,
    });
    onOpenChange(false);
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-[120] flex items-center justify-center p-4">
      {/* 遮罩背景 */}
      <button
        type="button"
        aria-label={t("close", "Close")}
        onClick={() => onOpenChange(false)}
        className="fixed inset-0 bg-black/40 backdrop-blur-xs transition-opacity animate-in fade-in duration-200"
      />

      {/* 弹窗主体 */}
      <div className="relative flex max-h-[88vh] w-full max-w-[680px] flex-col rounded-2xl border bg-background shadow-2xl animate-in zoom-in-95 duration-200">
        {/* 顶部标题区 */}
        <div className="flex shrink-0 items-start justify-between border-b px-6 py-4">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-amber-500/10 text-amber-500">
              <Star className="h-5 w-5 fill-amber-500/20" />
            </div>
            <div>
              <h3 className="text-base font-semibold text-foreground">
                {t("messageCenterSubTitle", "特别关注消息订阅")}
              </h3>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {t(
                  "messageCenterSubDesc",
                  "订阅特别关注的消息类型。发生对应事件时将优先展示并醒目提醒。",
                )}
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={() => onOpenChange(false)}
            className="rounded-lg p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            title={t("close", "Close")}
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* 搜索与快捷操作工具栏 */}
        <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b bg-card/40 px-6 py-2.5">
          <div className="relative min-w-[200px] flex-1">
            <Search className="absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t(
                "messageCenterSubSearchPlaceholder",
                "搜索消息类型、场景或关键词...",
              )}
              className="h-8 w-full rounded-lg border border-border/60 bg-muted/20 pr-7 pl-8 text-xs text-foreground placeholder:text-muted-foreground/60 transition-colors focus:border-foreground/30 focus:bg-background focus:outline-none"
            />
            {searchQuery ? (
              <button
                type="button"
                onClick={() => setSearchQuery("")}
                className="absolute top-1/2 right-2 -translate-y-1/2 rounded p-0.5 text-muted-foreground hover:text-foreground"
              >
                <X className="h-3 w-3" />
              </button>
            ) : null}
          </div>

          <div className="flex items-center gap-1.5">
            <button
              type="button"
              onClick={applyRecommended}
              className="inline-flex h-8 items-center gap-1 rounded-lg border border-amber-500/30 bg-amber-500/10 px-2.5 text-xs font-medium text-amber-600 transition-colors hover:bg-amber-500/20 dark:text-amber-400"
            >
              <Sparkles className="h-3 w-3" />
              <span>{t("messageCenterSubApplyRecommended", "推荐预设")}</span>
            </button>
            <button
              type="button"
              onClick={enableAll}
              className="inline-flex h-8 items-center rounded-lg border border-border/60 bg-muted/30 px-2 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            >
              {t("messageCenterSubSelectAll", "全部开启")}
            </button>
            <button
              type="button"
              onClick={disableAll}
              className="inline-flex h-8 items-center rounded-lg border border-border/60 bg-muted/30 px-2 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            >
              {t("messageCenterSubDeselectAll", "全部关闭")}
            </button>
          </div>
        </div>

        {/* 状态统计指示 */}
        <div className="flex shrink-0 items-center justify-between border-b bg-muted/20 px-6 py-2 text-[11px] text-muted-foreground">
          <span>
            {t("messageCenterSubEnabledCount", {
              enabled: currentEnabledCount,
              total: totalTypesCount,
              defaultValue: `已启用 ${currentEnabledCount} / ${totalTypesCount}`,
            })}
          </span>
          <span className="text-[10px] text-muted-foreground/60">
            {t(
              "messageCenterSubTip",
              "支持通过右侧开关按需开启/关闭，或点击分组快速全选",
            )}
          </span>
        </div>

        {/* 列表滚动内容区 */}
        <div className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
          {Object.keys(filteredGroupedTypes).length === 0 ? (
            <div className="py-12 text-center text-xs text-muted-foreground">
              {t("messageCenterNoSearchResults", "没有找到匹配的消息类型")}
            </div>
          ) : (
            Object.entries(filteredGroupedTypes).map(([source, items]) => {
              const isCollapsed = collapsedGroups.has(source);
              const groupEnabledCount = items.filter((i) =>
                draftSubscribedIds.has(i.id),
              ).length;
              const allGroupEnabled = groupEnabledCount === items.length;

              return (
                <div
                  key={source}
                  className="rounded-xl border border-border/70 bg-card/60 shadow-xs overflow-hidden transition-all"
                >
                  {/* 分组头部 */}
                  <div className="flex items-center justify-between bg-muted/30 px-3.5 py-2.5">
                    <button
                      type="button"
                      onClick={() => toggleGroupCollapse(source)}
                      className="flex items-center gap-2 text-left font-medium text-xs text-foreground hover:text-foreground/80 transition-colors"
                    >
                      {isCollapsed ? (
                        <ChevronRight className="h-3.5 w-3.5 text-muted-foreground" />
                      ) : (
                        <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
                      )}
                      <span>{getSourceLabel(source, tHelper)}</span>
                      <span className="rounded bg-muted px-1.5 py-0.2 font-mono text-[10px] text-muted-foreground">
                        {groupEnabledCount}/{items.length}
                      </span>
                    </button>

                    <button
                      type="button"
                      onClick={() => toggleGroupAll(source, items)}
                      className="text-[11px] text-muted-foreground hover:text-foreground transition-colors"
                    >
                      {allGroupEnabled
                        ? t("messageCenterSubDeselectAll", "关闭该组")
                        : t("messageCenterSubSelectAll", "开启该组")}
                    </button>
                  </div>

                  {/* 分组内的各类型列表 */}
                  {!isCollapsed ? (
                    <div className="divide-y divide-border/40">
                      {items.map((item) => {
                        const isChecked = draftSubscribedIds.has(item.id);
                        return (
                          <div
                            key={item.id}
                            className={`flex items-start justify-between gap-3 p-3 transition-colors ${
                              isChecked
                                ? "bg-amber-500/[0.02]"
                                : "hover:bg-muted/20"
                            }`}
                          >
                            <div className="min-w-0 flex-1">
                              <div className="flex items-center gap-2">
                                <span className="text-xs font-medium text-foreground">
                                  {t(item.nameKey, item.category)}
                                </span>
                                {renderSeverityBadge(item.defaultSeverity, tHelper)}
                                {item.isRecommended ? (
                                  <span className="rounded bg-amber-500/10 px-1 py-0.2 text-[9px] font-medium text-amber-600 dark:text-amber-400">
                                    ★ 推荐
                                  </span>
                                ) : null}
                              </div>
                              <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
                                {t(item.descKey, "")}
                              </p>
                            </div>

                            <div className="shrink-0 pt-0.5">
                              <Switch
                                checked={isChecked}
                                onCheckedChange={() => toggleSubscription(item.id)}
                                aria-label={t(item.nameKey, item.category)}
                              />
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  ) : null}
                </div>
              );
            })
          )}

          {/* 提醒偏好设置卡片 */}
          <div className="rounded-xl border border-border/70 bg-card/60 p-4 shadow-xs">
            <h4 className="text-xs font-semibold text-foreground mb-3 flex items-center gap-1.5">
              <Star className="h-3.5 w-3.5 text-amber-500" />
              {t("messageCenterSubPrefTitle", "特别关注通知偏好")}
            </h4>
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div>
                  <div className="text-xs text-foreground">
                    {t(
                      "messageCenterSubPrefHighlightToast",
                      "在全局弹窗 (Toast) 中突出高亮显示特别关注消息",
                    )}
                  </div>
                  <div className="text-[10px] text-muted-foreground">
                    {t(
                      "messageCenterSubPrefHighlightToastHint",
                      "使用琥珀金边框和特别关注标记，以便第一时间察觉",
                    )}
                  </div>
                </div>
                <Switch
                  checked={draftPreferences.highlightToast}
                  onCheckedChange={(checked) =>
                    setDraftPreferences((p) => ({ ...p, highlightToast: checked }))
                  }
                />
              </div>

              <div className="flex items-center justify-between border-t border-border/40 pt-3">
                <div>
                  <div className="text-xs text-foreground">
                    {t(
                      "messageCenterSubPrefExtendedToast",
                      "延长特别关注消息的通知停留时间 (8 秒)",
                    )}
                  </div>
                  <div className="text-[10px] text-muted-foreground">
                    {t(
                      "messageCenterSubPrefExtendedToastHint",
                      "给予充足的阅读与排障时间，避免关键告警过快消失",
                    )}
                  </div>
                </div>
                <Switch
                  checked={draftPreferences.extendedToastDuration}
                  onCheckedChange={(checked) =>
                    setDraftPreferences((p) => ({
                      ...p,
                      extendedToastDuration: checked,
                    }))
                  }
                />
              </div>
            </div>
          </div>
        </div>

        {/* 底部操作行 */}
        <div className="flex shrink-0 items-center justify-between border-t bg-muted/20 px-6 py-3">
          <button
            type="button"
            onClick={resetToDefault}
            className="inline-flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
          >
            <RotateCcw className="h-3.5 w-3.5" />
            <span>{t("messageCenterSubReset", "重置为默认")}</span>
          </button>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => onOpenChange(false)}
              className="rounded-lg border border-border/80 bg-background px-3 py-1.5 text-xs text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
            >
              {t("cancel", "取消")}
            </button>
            <button
              type="button"
              onClick={handleSave}
              className="inline-flex items-center gap-1.5 rounded-lg bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-foreground shadow-xs hover:bg-primary/90 transition-colors"
            >
              <Check className="h-3.5 w-3.5" />
              <span>{t("messageCenterSubSave", "保存设置")}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
