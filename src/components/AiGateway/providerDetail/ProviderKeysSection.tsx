import {
  AlertCircle,
  Check,
  Clock,
  Copy,
  Eye,
  EyeOff,
  KeyRound,
  Plus,
  RotateCcw,
  Tag,
  X,
} from "lucide-react";
import { Switch } from "@/components/ui/switch";
import {
  formatGatewayTimestamp,
  providerKeyState,
  PROVIDER_KEY_STATE_TRANSLATION_KEYS,
} from "@/lib/aiGateway";
import {
  maskSecret,
  providerKeyIconContainerClass,
  providerKeyStateClass,
  providerKeyStateDotClass,
} from "./helpers";
import type { ProviderDetailForm } from "./hooks/useProviderDetailForm";

export function ProviderKeysSection({
  form,
}: {
  form: ProviderDetailForm;
}) {
  const {
    t,
    provider,
    busy,
    onReenableKey,
    keyDrafts,
    keyNameError,
    keyValueError,
    revealedKeyIds,
    expandedReasonKeyIds,
    copiedKeyId,
    handleAddKey,
    handleEditKey,
    updateKey,
    toggleKeyVisibility,
    toggleReasonExpansion,
    handleCopyKey,
    tags,
    tagInput,
    setTagInput,
    handleRemoveTag,
    handleToggleTag,
    handleTagInputKeyDown,
    candidateTags,
  } = form;
  return (
    <>
            {/* 第 4 行：上游密钥池编辑（有序、可增删改与排序） */}
            <div className="full-span space-y-1.5">
              <label className="inline-flex items-center justify-between w-full text-xs font-semibold text-foreground">
                <span className="inline-flex items-center gap-1.5">
                  <KeyRound className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>{t("aiGatewayProviderKeys", "Upstream keys")}</span>
                  <span className="text-[11px] font-normal text-muted-foreground">
                    ({keyDrafts.length})
                  </span>
                </span>
                <button
                  type="button"
                  data-testid="ai-gateway-add-key"
                  onClick={handleAddKey}
                  disabled={busy}
                  className="inline-flex h-8 items-center justify-center gap-1.5 rounded-lg border border-primary/25 bg-primary/5 px-3 text-xs font-medium text-primary shadow-2xs transition hover:bg-primary/10 hover:border-primary/40 active:scale-95 disabled:opacity-50"
                >
                  <Plus className="h-3.5 w-3.5" />
                  {t("aiGatewayProviderKeyAdd", "Add key")}
                </button>
              </label>

              <div
                data-testid="ai-gateway-provider-keys"
                className="mt-2 grid grid-cols-1 md:grid-cols-2 gap-3"
              >
                {keyDrafts.length === 0 ? (
                  <div className="col-span-full flex flex-col items-center justify-center rounded-xl border border-dashed border-border/70 bg-muted/20 py-8 px-4 text-center">
                    <div className="h-10 w-10 rounded-full bg-muted/60 text-muted-foreground flex items-center justify-center mb-2">
                      <KeyRound className="h-5 w-5" />
                    </div>
                    <p className="text-xs font-medium text-foreground">
                      {t(
                        "aiGatewayNoKeys",
                        "No local keys yet. Add a key to route traffic.",
                      )}
                    </p>
                  </div>
                ) : (
                  keyDrafts.map((key, index) => {
                    const state = providerKeyState(key);
                    const markedTime = formatGatewayTimestamp(key.marked_at);
                    return (
                      <div
                        key={key.id}
                        data-testid={`ai-gateway-key-${index}`}
                        onClick={() => handleEditKey(index)}
                        className={`group relative flex flex-col justify-between rounded-xl border bg-card p-3.5 shadow-2xs transition-all cursor-pointer space-y-3 ${
                          key.enabled
                            ? "border-border/70 hover:border-primary/40 hover:shadow-md hover:shadow-primary/5 bg-gradient-to-b from-card to-muted/20 dark:to-muted/10"
                            : "border-border/40 bg-card/60 opacity-75 hover:opacity-100 hover:border-border"
                        }`}
                      >
                        {/* 1. 卡片头：图标徽标 + 名称与状态 + 启用开关 */}
                        <div className="flex items-center justify-between gap-2">
                          <div className="flex items-center gap-2.5 min-w-0">
                            <div
                              className={`h-8 w-8 rounded-lg flex items-center justify-center shrink-0 border transition-colors ${providerKeyIconContainerClass[state]}`}
                            >
                              <KeyRound className="h-4 w-4" />
                            </div>
                            <div className="flex items-center gap-2 min-w-0 flex-wrap">
                              <span
                                data-testid={`ai-gateway-key-name-${index}`}
                                className="font-semibold text-sm text-foreground truncate max-w-[130px] sm:max-w-[160px] group-hover:text-primary transition-colors"
                                title={key.name}
                              >
                                {key.name}
                              </span>
                              <span
                                data-testid={`ai-gateway-key-state-${index}`}
                                className={`inline-flex shrink-0 items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11px] font-medium leading-4 ${providerKeyStateClass[state]}`}
                              >
                                <span
                                  className={`h-1.5 w-1.5 rounded-full shrink-0 ${providerKeyStateDotClass[state]}`}
                                  aria-hidden="true"
                                />
                                <span>
                                  {t(PROVIDER_KEY_STATE_TRANSLATION_KEYS[state])}
                                </span>
                                {markedTime ? (
                                  <span className="font-normal opacity-75 text-[10px]">
                                    {t("aiGatewayProviderKeyMarkedAt", {
                                      time: markedTime,
                                      defaultValue: `Marked at ${markedTime}`,
                                    })}
                                  </span>
                                ) : null}
                              </span>
                              {key.daily_reset ? (
                                <span
                                  data-testid={`ai-gateway-key-daily-reset-badge-${index}`}
                                  className="inline-flex shrink-0 items-center gap-1 rounded-full border border-sky-500/30 bg-sky-500/10 px-2 py-0.5 text-[11px] font-medium leading-4 text-sky-700 dark:text-sky-400"
                                  title={t(
                                    "aiGatewayProviderKeyDailyResetHint",
                                    "When this key hits its quota, it becomes available again after the next UTC+8 midnight.",
                                  )}
                                >
                                  <Clock className="h-3 w-3 shrink-0" />
                                  <span>{t("aiGatewayProviderKeyDailyReset", "Daily reset")}</span>
                                </span>
                              ) : null}
                            </div>
                          </div>

                          {/* 快捷启用 Switch & 重新启用按钮（阻止冒泡） */}
                          <div
                            className="flex shrink-0 items-center gap-1.5"
                            onClick={(e) => e.stopPropagation()}
                          >
                            {key.auto_marked ? (
                              <button
                                type="button"
                                data-testid={`ai-gateway-reenable-key-${index}`}
                                onClick={() => onReenableKey?.(provider.id, key.id)}
                                disabled={busy}
                                className="inline-flex h-6 items-center gap-1 rounded-md border border-amber-500/30 bg-amber-500/10 px-2 text-[10px] font-medium text-amber-700 dark:text-amber-400 shadow-2xs transition hover:bg-amber-500/20 hover:border-amber-500/50 active:scale-95 disabled:opacity-50"
                              >
                                <RotateCcw className="h-3 w-3" />
                                {t(
                                  "aiGatewayProviderKeyReenable",
                                  "Re-enable",
                                )}
                              </button>
                            ) : null}
                            <label
                              className="inline-flex shrink-0 items-center gap-1.5 cursor-pointer select-none"
                              title={
                                key.enabled
                                  ? t("aiGatewayEnabled", "Enabled")
                                  : t("aiGatewayDisabled", "Disabled")
                              }
                            >
                              <Switch
                                data-testid={`ai-gateway-key-toggle-${index}`}
                                aria-label={t("aiGatewayProviderKeyToggleAria", {
                                  index: index + 1,
                                  defaultValue: `Enable key ${index + 1}`,
                                })}
                                checked={key.enabled}
                                disabled={busy}
                                onCheckedChange={(checked) =>
                                  updateKey(index, { enabled: checked })
                                }
                                className="shrink-0"
                              />
                            </label>
                          </div>
                        </div>

                        {/* 2. 卡片身：凭据盒 & 诊断信息 */}
                        <div className="space-y-1.5">
                          <div
                            className="flex items-center justify-between gap-2 rounded-lg border border-border/60 bg-muted/40 dark:bg-zinc-900/40 px-3 py-1.5 font-mono text-xs transition-colors hover:border-border/90"
                            onClick={(e) => e.stopPropagation()}
                          >
                            <span
                              data-testid={`ai-gateway-key-value-${index}`}
                              className="truncate text-muted-foreground select-all tracking-wider text-[11px]"
                            >
                              {revealedKeyIds[key.id]
                                ? key.value || t("aiGatewayKeyStoredBackend", "Stored secret")
                                : maskSecret(key.value)}
                            </span>
                            <div className="flex shrink-0 items-center gap-0.5 rounded-md border border-border/40 bg-background/60 p-0.5 dark:bg-background/40">
                              <button
                                type="button"
                                data-testid={`ai-gateway-key-toggle-visibility-${index}`}
                                onClick={() => toggleKeyVisibility(key.id)}
                                disabled={busy}
                                aria-label={
                                  revealedKeyIds[key.id]
                                    ? t("aiGatewayHideSecret", "Hide secret")
                                    : t("aiGatewayShowSecret", "Show secret")
                                }
                                title={
                                  revealedKeyIds[key.id]
                                    ? t("aiGatewayHideSecret", "Hide secret")
                                    : t("aiGatewayShowSecret", "Show secret")
                                }
                                className="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition hover:bg-muted hover:text-foreground active:scale-95 disabled:opacity-50"
                              >
                                {revealedKeyIds[key.id] ? (
                                  <EyeOff className="h-3.5 w-3.5" />
                                ) : (
                                  <Eye className="h-3.5 w-3.5" />
                                )}
                              </button>
                              {key.value ? (
                                <button
                                  type="button"
                                  data-testid={`ai-gateway-key-copy-${index}`}
                                  onClick={() => handleCopyKey(key.id, key.value)}
                                  disabled={busy}
                                  aria-label={
                                    copiedKeyId === key.id
                                      ? t("aiGatewayCopiedSecret", "Copied")
                                      : t("aiGatewayCopySecret", "Copy secret")
                                  }
                                  title={
                                    copiedKeyId === key.id
                                      ? t("aiGatewayCopiedSecret", "Copied")
                                      : t("aiGatewayCopySecret", "Copy secret")
                                  }
                                  className="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition hover:bg-muted hover:text-foreground active:scale-95 disabled:opacity-50"
                                >
                                  {copiedKeyId === key.id ? (
                                    <Check className="h-3.5 w-3.5 text-emerald-500 scale-110 transition-transform" />
                                  ) : (
                                    <Copy className="h-3.5 w-3.5" />
                                  )}
                                </button>
                              ) : null}
                            </div>
                          </div>

                          {key.auto_marked && key.reason && key.reason.trim() ? (
                            (() => {
                              const reasonText = key.reason.trim();
                              const isExpanded = Boolean(expandedReasonKeyIds[key.id]);
                              const isLongMessage = reasonText.length > 40;

                              return (
                                <div
                                  data-testid={`ai-gateway-key-reason-${index}`}
                                  className="rounded-lg border border-destructive/20 bg-destructive/10 px-2.5 py-1.5 text-[11px] text-destructive shadow-2xs transition-all"
                                  onClick={(e) => e.stopPropagation()}
                                >
                                  <div className="flex items-start gap-1.5">
                                    <AlertCircle className="h-3.5 w-3.5 shrink-0 mt-0.5 text-destructive" />
                                    <div className="flex-1 min-w-0">
                                      <span className="font-semibold mr-1 shrink-0">
                                        {t("aiGatewayProviderKeyReasonLabel", "Reason")}:
                                      </span>
                                      <span
                                        className={
                                          isExpanded
                                            ? "break-all whitespace-pre-wrap leading-relaxed text-destructive/90"
                                            : "truncate block sm:inline leading-relaxed"
                                        }
                                        title={isExpanded ? undefined : reasonText}
                                      >
                                        {reasonText}
                                      </span>
                                      {isLongMessage ? (
                                        <button
                                          type="button"
                                          data-testid={`ai-gateway-key-reason-toggle-${index}`}
                                          onClick={() => toggleReasonExpansion(key.id)}
                                          className="ml-1 inline-flex items-center text-[11px] font-medium text-destructive underline underline-offset-2 hover:opacity-80 transition cursor-pointer select-none"
                                        >
                                          {isExpanded
                                            ? t("aiGatewayProviderKeyCollapseDetails", "Collapse")
                                            : t("aiGatewayProviderKeyExpandDetails", "View details")}
                                        </button>
                                      ) : null}
                                    </div>
                                  </div>
                                </div>
                              );
                            })()
                          ) : null}
                        </div>
                      </div>
                    );
                  })
                )}
              </div>

              {keyNameError ? (
                <p
                  data-testid="ai-gateway-key-name-error"
                  role="alert"
                  className="mt-1 text-xs text-destructive"
                >
                  {t(
                    "aiGatewayProviderKeyNameRequired",
                    "Enter a key name.",
                  )}
                </p>
              ) : null}
              {keyValueError ? (
                <p
                  data-testid="ai-gateway-key-value-error"
                  role="alert"
                  className="mt-1 text-xs text-destructive"
                >
                  {t(
                    "aiGatewayProviderKeyValueRequired",
                    "Enter a value for the new key.",
                  )}
                </p>
              ) : null}
            </div>

            {/* 第 5 行：标签区块（非必填，支持多选） */}
            <div className="field full-span">
              <label className="inline-flex items-center justify-between w-full">
                <span className="inline-flex items-center gap-1.5">
                  <Tag className="h-3.5 w-3.5 text-muted-foreground" />
                  <span>{t("aiGatewayProviderTags", "Tags")}</span>
                  <span className="text-[11px] font-normal text-muted-foreground">
                    ({t("commonOptional", "Optional")})
                  </span>
                </span>
              </label>
              <div
                data-testid="ai-gateway-provider-tags-container"
                className="flex flex-wrap items-center gap-1.5 min-h-[38px] rounded-lg border border-border bg-background p-1.5 focus-within:border-primary focus-within:ring-2 focus-within:ring-primary/50"
              >
                {tags.map((tag) => (
                  <span
                    key={tag}
                    data-testid={`provider-tag-badge-${tag}`}
                    className="inline-flex items-center gap-1 rounded-md bg-primary/10 border border-primary/20 px-2 py-0.5 text-xs font-medium text-primary"
                  >
                    <span>{tag}</span>
                    <button
                      type="button"
                      data-testid={`remove-tag-${tag}`}
                      onClick={() => handleRemoveTag(tag)}
                      aria-label={`Remove tag ${tag}`}
                      className="rounded p-0.5 hover:bg-primary/20 transition-colors"
                    >
                      <X className="h-3 w-3" />
                    </button>
                  </span>
                ))}
                <input
                  data-testid="ai-gateway-provider-tags-input"
                  type="text"
                  value={tagInput}
                  onChange={(e) => setTagInput(e.target.value)}
                  onKeyDown={handleTagInputKeyDown}
                  placeholder={
                    tags.length === 0
                      ? t(
                          "aiGatewayProviderTagsPlaceholder",
                          "Add a tag and press Enter...",
                        )
                      : ""
                  }
                  aria-label={t("aiGatewayProviderTags", "Tags")}
                  className="flex-1 min-w-[140px] bg-transparent border-none outline-none text-sm text-foreground placeholder:text-muted-foreground/60 h-6 px-1"
                />
              </div>

              {candidateTags.length > 0 && (
                <div className="mt-1.5 flex flex-wrap items-center gap-1">
                  <span className="text-[11px] text-muted-foreground mr-1">
                    {t("aiGatewayRecommendedTags", "Suggested tags")}:
                  </span>
                  {candidateTags.map((candidate) => {
                    const isSelected = tags.includes(candidate);
                    return (
                      <button
                        key={candidate}
                        type="button"
                        data-testid={`suggested-tag-${candidate}`}
                        onClick={() => handleToggleTag(candidate)}
                        className={`inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] font-medium transition-all ${
                          isSelected
                            ? "bg-primary text-primary-foreground shadow-2xs"
                            : "bg-muted/70 text-muted-foreground hover:bg-muted hover:text-foreground border border-border/50"
                        }`}
                      >
                        {isSelected ? <Check className="h-3 w-3" /> : <Plus className="h-3 w-3 opacity-60" />}
                        <span>{candidate}</span>
                      </button>
                    );
                  })}
                </div>
              )}
              <p className="text-[11px] text-muted-foreground mt-1">
                {t(
                  "aiGatewayProviderTagsDesc",
                  "Assign tags to categorize providers for filtering (optional).",
                )}
              </p>
            </div>
    </>
  );
}
