import {
  BarChart3,
  ChevronsUpDown,
  History as HistoryIcon,
  KeyRound,
  Layers,
  Network,
  Plus,
  ScrollText,
  Server,
  Sparkles,
  TerminalSquare,
} from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { LocalKeyDialog } from "./LocalKeyDialog";
import { LocalKeyList } from "./LocalKeyList";
import { ModelListPanel } from "./ModelListPanel";
import { ModelPoolDialog } from "./ModelPoolDialog";
import { ModelPoolListPanel } from "./ModelPoolListPanel";
import { ProviderDetailDialog } from "./ProviderDetailDialog";
import { ProviderTemplateEditDialog } from "./ProviderTemplateEditDialog";
import { ProviderTemplatePickerDialog } from "./ProviderTemplatePickerDialog";
import { ProviderTemplateSection } from "./ProviderTemplateSection";
import { ProviderUsageHistoryPanel } from "./ProviderUsageHistoryPanel";
import { RuntimeStatusCard } from "./RuntimeStatusCard";
import { TemplateCreateDialog } from "./TemplateCreateDialog";
import { TerminalSyncPanel } from "./TerminalSyncPanel";
import { UpstreamProviderList } from "./UpstreamProviderList";
import { UsageLogsPanel } from "./UsageLogsPanel";
import { UsageStatsPanel } from "./UsageStatsPanel";
import { emptyProvider, type AiGatewayTab } from "./helpers/pageTypes";
import { useAiGatewayPageActions } from "./hooks/useAiGatewayPageActions";
import { useAiGatewayPageData } from "./hooks/useAiGatewayPageData";

export function AiGateway({ isVisible = true }: { isVisible?: boolean }) {
  const data = useAiGatewayPageData(isVisible);
  const actions = useAiGatewayPageActions(data);
  const {
    t,
    ToolIcon,
    iconClassName,
    config,
    loadError,
    load,
    activeTab,
    visitedTabs,
    selectTab,
    modelPoolsForView,
    modelListTabCount,
    status,
    targets,
    busy,
    addressCopied,
    anthropicAddressCopied,
    todayStats,
    todayFailedRequests,
    refreshingToday,
    handleManualRefresh,
    templates,
    templatesLoadError,
    templateExpandedIds,
    handleToggleTemplateExpand,
    handleToggleAllTemplates,
    syncingTemplates,
    isTemplateManageOpen,
    setIsTemplateManageOpen,
    isTemplatePickerOpen,
    setIsTemplatePickerOpen,
    isTemplateEditOpen,
    setIsTemplateEditOpen,
    editingTemplate,
    setEditingTemplate,
    creatingTemplate,
    setCreatingTemplate,
    isPoolDialogOpen,
    setIsPoolDialogOpen,
    poolDialogKey,
    editingPool,
    setEditingPool,
    poolStatuses,
    isKeyDialogOpen,
    setIsKeyDialogOpen,
    isDialogOpen,
    setIsDialogOpen,
    selectedProviderId,
    setSelectedProviderId,
    editingProvider,
    setEditingProvider,
    visitedUsageSubTabs,
    usageSubTab,
    selectUsageSubTab,
    usageFocusModel,
    usageFocusToken,
    usageFocusProviderId,
    usageFocusProviderName,
    usageFocusDay,
    syncingTools,
    copiedKeyId,
    navigateToProviders,
    navigateToModelPools,
    allTemplatesExpanded,
  } = data;
  const {
    handleToggleService,
    handleCopyAddress,
    handleCopyAnthropicAddress,
    handleToggleProviderEnabled,
    handleReenableProviderModel,
    handleReenableProviderModels,
    handleReenableProviderKey,
    handleSaveProvider,
    handleDeleteProvider,
    handleOpenNewPool,
    handleTogglePoolEnabled,
    handleOpenEditPool,
    handleSavePool,
    handleDeletePool,
    handleOpenPoolLogs,
    handleOpenProviderDay,
    handleReturnToProviderHistory,
    handleClearProviderDayFocus,
    handleDeleteProviderModel,
    handleRestoreProviderModel,
    handleSaveKey,
    handleDeleteKey,
    handleToggleKeyEnabled,
    handleSetDefaultKey,
    handleConfigureTool,
    handleSyncTool,
    handleSyncTemplate,
    handleCreateProviderFromTemplate,
    handleUpsertTemplate,
    handleDeleteTemplate,
    handleCopyKey,
    defaultKeyId,
    availableProviderTags,
  } = actions;

  if (!config) {
    return (
      <div className="h-full overflow-y-auto">
        <div className="mx-auto max-w-7xl p-6">
          {loadError ? (
            <div className="flex flex-col items-center justify-center rounded-2xl border border-destructive/20 bg-destructive/5 p-8 text-center">
              <div className="rounded-full bg-destructive/10 p-3 text-destructive">
                <Network className="h-6 w-6" />
              </div>
              <h3 className="mt-3 text-base font-semibold text-foreground">
                {t("aiGatewayLoadFailed", "Failed to load AI Gateway configuration.")}
              </h3>
              <p className="mt-1 max-w-md text-xs text-muted-foreground">{loadError}</p>
              <button
                type="button"
                onClick={() => void load()}
                className="mt-4 inline-flex items-center rounded-lg bg-primary px-4 py-2 text-xs font-medium text-primary-foreground shadow-sm hover:bg-primary/90"
              >
                {t("retry", "Retry")}
              </button>
            </div>
          ) : (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <span className="h-4 w-4 animate-spin rounded-full border-2 border-primary border-t-transparent" />
              <span>{t("loading", "Loading...")}</span>
            </div>
          )}
        </div>
      </div>
    );
  }

  const pendingSyncCount = (targets ?? []).filter(
    (target) => target.pending_sync,
  ).length;
  const syncedTargetsCount = (targets ?? []).filter(
    (target) => target.synced,
  ).length;
  const autoDisabledCount = status?.auto_disabled_count ?? 0;

  const tabs: Array<{
    id: AiGatewayTab;
    label: string;
    icon: typeof Server;
    count?: number | string;
    hasAlert?: boolean;
  }> = [
    {
      id: "providers",
      label: t("aiGatewayProviders", "Upstream providers"),
      icon: Server,
      count: config?.providers?.length ?? 0,
      hasAlert: autoDisabledCount > 0,
    },
    {
      id: "model-pools",
      label: t("aiGatewayModelPoolsTab", "Model pools"),
      icon: Layers,
      count: modelPoolsForView.length,
    },
    {
      id: "models",
      label: t("aiGatewayModelListTab", "Model list"),
      icon: Sparkles,
      count: modelListTabCount,
    },
    {
      id: "keys",
      label: t("aiGatewayKeys", "API Keys"),
      icon: KeyRound,
      count: config?.keys?.length ?? 0,
    },
    {
      id: "terminals",
      label: t("aiGatewayTerminalSync", "AI terminal integration"),
      icon: TerminalSquare,
      count:
        (targets ?? []).length > 0
          ? `${syncedTargetsCount}/${targets.length}`
          : undefined,
      hasAlert: pendingSyncCount > 0,
    },
    {
      id: "usage",
      label: t("aiGatewayUsageAndLogsTab", "Usage & Logs"),
      icon: BarChart3,
    },
  ];

  // 传给详情弹窗的最新运行时快照：取已加载 config 中与编辑目标同 id 的服务商对象，
  // 供弹窗只合并运行时字段（未找到或新建服务商时为 null）。
  const runtimeProvider = editingProvider?.id
    ? config?.providers.find((provider) => provider.id === editingProvider.id) ?? null
    : null;

  return (
    <div className="h-full overflow-y-auto" data-testid="ai-gateway-console">
      <div className="mx-auto max-w-7xl space-y-4 p-6">
        {/* 头部标题与简介（对齐 AiEnvironments 规范） */}
        <header className="flex items-start gap-3">
          <div className={`rounded-lg p-2 ${iconClassName}`}>
            <ToolIcon className="h-5 w-5" />
          </div>
          <div className="space-y-0.5">
            <h1 className="text-xl font-bold tracking-tight text-foreground">
              {t("aiGateway", "AI Gateway")}
            </h1>
            <p className="max-w-3xl text-xs text-muted-foreground">
              {t(
                "aiGatewayWorkspaceDesc",
                "Manage upstream providers and key pools with OpenAI / Anthropic dual-protocol relay, intelligent routing, and failover, syncing seamlessly to OpenCode, Codex, and Claude Code.",
              )}
            </p>
          </div>
        </header>

        {/* 常驻运行时服务状态卡片 */}
        <RuntimeStatusCard
          status={status}
          config={config}
          busy={busy}
          addressCopied={addressCopied}
          anthropicAddressCopied={anthropicAddressCopied}
          todayStats={todayStats}
          todayFailedRequests={todayFailedRequests}
          refreshing={refreshingToday}
          onSelectTab={selectTab}
          onStart={handleToggleService}
          onStop={handleToggleService}
          onCopyAddress={() => void handleCopyAddress()}
          onCopyAnthropicAddress={() => void handleCopyAnthropicAddress()}
          onRefresh={() => void handleManualRefresh()}
        />

        {/* 工作区 Tabs 标签页导航 */}
        <div
          role="tablist"
          aria-label={t("aiGatewayWorkspaceTabs", "AI Gateway tabs")}
          className="flex flex-wrap items-center gap-1.5 rounded-xl border bg-muted/50 p-1.5 shadow-xs"
        >
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                type="button"
                role="tab"
                aria-selected={isActive}
                onClick={() => selectTab(tab.id)}
                className={`group inline-flex h-9 items-center gap-2 rounded-lg px-3.5 text-xs font-medium transition-all ${
                  isActive
                    ? "bg-background text-foreground font-semibold shadow-xs ring-1 ring-border/80 dark:ring-border"
                    : "text-muted-foreground hover:bg-background/60 hover:text-foreground"
                }`}
              >
                <Icon
                  className={`h-4 w-4 transition-colors ${
                    isActive
                      ? "text-primary"
                      : "text-muted-foreground/70 group-hover:text-foreground"
                  }`}
                />
                <span>{tab.label}</span>
                {tab.count !== undefined ? (
                  <span
                    className={`rounded-full px-2 py-0.5 text-[11px] font-semibold transition-colors ${
                      isActive
                        ? "bg-primary/10 text-primary"
                        : "bg-muted text-muted-foreground group-hover:bg-muted/80 group-hover:text-foreground"
                    }`}
                  >
                    {tab.count}
                  </span>
                ) : null}
                {tab.hasAlert ? (
                  <span
                    className="relative flex h-2 w-2"
                    title={t("aiGatewayHasPendingItems", "Has items needing attention")}
                  >
                    <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-amber-400 opacity-75" />
                    <span className="relative inline-flex h-2 w-2 rounded-full bg-amber-500" />
                  </span>
                ) : null}
              </button>
            );
          })}
        </div>

        {/* Tab 1: 上游服务商 */}
        <div
          role="tabpanel"
          aria-label={t("aiGatewayProviders", "Upstream providers")}
          className={activeTab === "providers" ? "block" : "hidden"}
        >
          <UpstreamProviderList
            providers={config.providers}
            templates={templates}
            selectedProviderId={selectedProviderId}
            busy={busy}
            onSelect={(providerId) => {
              setSelectedProviderId(providerId);
              const found =
                config.providers.find((provider) => provider.id === providerId) ?? null;
              setEditingProvider(found);
              setIsDialogOpen(true);
            }}
            onToggleEnabled={(provider, enabled) =>
              void handleToggleProviderEnabled(provider, enabled)
            }
            onAdd={() => setIsTemplatePickerOpen(true)}
            onManageTemplates={() => setIsTemplateManageOpen(true)}
            onReenableKey={(providerId, keyId) =>
              void handleReenableProviderKey(providerId, keyId)
            }
            onReenableModel={handleReenableProviderModel}
          />
        </div>

        {/* Tab 2: 模型池 */}
        <div
          role="tabpanel"
          aria-label={t("aiGatewayModelPoolsTab", "Model pools")}
          className={activeTab === "model-pools" ? "block" : "hidden"}
        >
          {visitedTabs.has("model-pools") ? (
            <ModelPoolListPanel
              modelPools={modelPoolsForView}
              providers={config.providers}
              poolStatuses={poolStatuses}
              busy={busy}
              isActive={activeTab === "model-pools"}
              onAddPool={() => handleOpenNewPool()}
              onTogglePoolEnabled={(poolId, enabled) => {
                const pool = modelPoolsForView.find(
                  (item) => item.id === poolId,
                );
                if (pool) void handleTogglePoolEnabled(pool, enabled);
              }}
              onEditPool={(pool) => handleOpenEditPool(pool)}
              onOpenPoolLogs={(model) => handleOpenPoolLogs(model)}
            />
          ) : null}
        </div>

        {/* Tab 2: 模型列表（首次访问后才挂载以保留搜索状态） */}
        <div
          role="tabpanel"
          aria-label={t("aiGatewayModelListTab", "Model list")}
          className={activeTab === "models" ? "block" : "hidden"}
        >
          {visitedTabs.has("models") ? (
            <ModelListPanel
              providers={config.providers}
              modelPools={modelPoolsForView}
              port={config.port}
              isActive={activeTab === "models"}
              onNavigateProviders={navigateToProviders}
              onNavigatePools={navigateToModelPools}
            />
          ) : null}
        </div>

        {/* Tab 3: 本地密钥 */}
        <div
          role="tabpanel"
          aria-label={t("aiGatewayKeys", "API Keys")}
          className={activeTab === "keys" ? "block" : "hidden"}
        >
          <LocalKeyList
            keys={config.keys}
            defaultKeyId={defaultKeyId}
            busy={busy}
            copiedKeyId={copiedKeyId}
            onAdd={() => setIsKeyDialogOpen(true)}
            onDelete={(keyId) => void handleDeleteKey(keyId)}
            onSetDefault={(keyId) => void handleSetDefaultKey(keyId)}
            onToggleEnabled={(key, enabled) => void handleToggleKeyEnabled(key, enabled)}
            onCopy={(key) => void handleCopyKey(key)}
          />
        </div>

        {/* Tab 4: AI 终端集成 */}
        <div
          role="tabpanel"
          aria-label={t("aiGatewayTerminalSync", "AI terminal integration")}
          className={activeTab === "terminals" ? "block" : "hidden"}
        >
          {visitedTabs.has("terminals") ? (
            <TerminalSyncPanel
              targets={targets}
              config={config}
              gatewayRunning={Boolean(status?.running)}
              onStartGateway={handleToggleService}
              startingGateway={busy}
              syncingTools={syncingTools}
              isActive={activeTab === "terminals"}
              onConfigureTool={(tool, claudeModels) =>
                handleConfigureTool(tool, claudeModels)
              }
              onSyncTool={(tool, claudeModels) =>
                handleSyncTool(tool, claudeModels)
              }
            />
          ) : null}
        </div>

        {/* Tab 5: 用量与日志 */}
        <div
          role="tabpanel"
          aria-label={t("aiGatewayUsageAndLogsTab", "Usage & Logs")}
          className={activeTab === "usage" ? "space-y-4 block" : "hidden"}
        >
          {/* 顶部标题与二级子标签切换 */}
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between pb-1 border-b border-border/40">
            <div className="flex items-center gap-2.5">
              <div className="rounded-lg p-2 bg-primary/10 text-primary">
                <BarChart3 className="h-5 w-5" />
              </div>
              <div className="space-y-0.5">
                <div className="flex items-center gap-2">
                  <h2 className="text-base font-semibold text-foreground">
                    {t("aiGatewayUsageAndLogsTab", "Usage & Logs")}
                  </h2>
                </div>
                <p className="text-xs text-muted-foreground">
                  {t(
                    "aiGatewayUsageAndLogsDesc",
                    "View token usage statistics, cost breakdowns, and granular request logs.",
                  )}
                </p>
              </div>
            </div>

            {/* 二级子标签切换（用量统计 / 请求日志） */}
            <div
              role="tablist"
              aria-label={t("aiGatewayUsageSubTabs", "Usage and logs subtabs")}
              className="inline-flex items-center rounded-lg border bg-muted/40 p-1 text-xs shrink-0"
            >
              <button
                type="button"
                role="tab"
                aria-selected={usageSubTab === "stats"}
                onClick={() => selectUsageSubTab("stats")}
                data-testid="ai-gateway-subtab-usage-stats"
                className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 font-medium transition-all ${
                  usageSubTab === "stats"
                    ? "bg-background text-foreground font-semibold shadow-xs"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                <BarChart3 className="h-3.5 w-3.5" />
                <span>{t("aiGatewayUsageStatsSubTab", "Usage stats")}</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={usageSubTab === "logs"}
                onClick={() => selectUsageSubTab("logs")}
                data-testid="ai-gateway-subtab-usage-logs"
                className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 font-medium transition-all ${
                  usageSubTab === "logs"
                    ? "bg-background text-foreground font-semibold shadow-xs"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                <ScrollText className="h-3.5 w-3.5" />
                <span>{t("aiGatewayUsageLogsSubTab", "Request logs")}</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={usageSubTab === "history"}
                onClick={() => selectUsageSubTab("history")}
                data-testid="ai-gateway-subtab-usage-history"
                className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 font-medium transition-all ${
                  usageSubTab === "history"
                    ? "bg-background text-foreground font-semibold shadow-xs"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                <HistoryIcon className="h-3.5 w-3.5" />
                <span>
                  {t("aiGatewayProviderHistorySubTab", "Provider history")}
                </span>
              </button>
            </div>
          </div>

          <div className={usageSubTab === "stats" ? "block" : "hidden"}>
            {visitedUsageSubTabs.has("stats") ||
            (activeTab === "usage" && usageSubTab === "stats") ? (
              <UsageStatsPanel
                isActive={activeTab === "usage" && usageSubTab === "stats"}
                providers={config.providers}
              />
            ) : null}
          </div>

          <div className={usageSubTab === "logs" ? "block" : "hidden"}>
            {visitedUsageSubTabs.has("logs") ||
            (activeTab === "usage" && usageSubTab === "logs") ? (
            <UsageLogsPanel
              isActive={activeTab === "usage" && usageSubTab === "logs"}
              focusModel={usageFocusModel}
              focusToken={usageFocusToken}
              focusProviderId={usageFocusProviderId}
              focusProviderName={usageFocusProviderName}
              focusDay={usageFocusDay}
              onReturnToProviderHistory={handleReturnToProviderHistory}
              onClearProviderDayFocus={handleClearProviderDayFocus}
            />
            ) : null}
          </div>

          <div className={usageSubTab === "history" ? "block" : "hidden"}>
            {visitedUsageSubTabs.has("history") ||
            (activeTab === "usage" && usageSubTab === "history") ? (
            <ProviderUsageHistoryPanel
              isActive={activeTab === "usage" && usageSubTab === "history"}
              onOpenDay={handleOpenProviderDay}
              providers={config.providers}
              templates={templates}
            />
            ) : null}
          </div>
        </div>

        {/* 服务商新增与编辑模态弹窗 */}
        <ProviderDetailDialog
          open={isDialogOpen}
          onOpenChange={(open) => {
            setIsDialogOpen(open);
            if (!open) {
              setSelectedProviderId(null);
              setEditingProvider(null);
            }
          }}
          provider={editingProvider}
          runtimeProvider={runtimeProvider}
          availableTags={availableProviderTags}
          prices={config.model_prices ?? []}
          busy={busy}
          onSave={(draft, prices) => handleSaveProvider(draft, prices)}
          onDelete={(providerId) => handleDeleteProvider(providerId)}
          templates={templates}
          onDeleteModel={(providerId, upstreamModel) =>
            void handleDeleteProviderModel(providerId, upstreamModel)
          }
          onRestoreModel={(providerId, upstreamModel) =>
            void handleRestoreProviderModel(providerId, upstreamModel)
          }
          onReenableModel={(providerId, localModel, upstreamModel) =>
            void handleReenableProviderModel(
              providerId,
              localModel,
              upstreamModel,
            )
          }
          onReenableModels={(providerId) =>
            void handleReenableProviderModels(providerId)
          }
          onReenableKey={(providerId, keyId) =>
            void handleReenableProviderKey(providerId, keyId)
          }
        />

        {/* 本地密钥新增模态弹窗 */}
        <LocalKeyDialog
          open={isKeyDialogOpen}
          onOpenChange={setIsKeyDialogOpen}
          busy={busy}
          onSave={handleSaveKey}
        />


        {/* 预设服务商模板选择器 */}
        <ProviderTemplatePickerDialog
          open={isTemplatePickerOpen}
          onOpenChange={setIsTemplatePickerOpen}
          templates={templates}
          busy={busy}
          onSelectBlank={() => {
            setIsTemplatePickerOpen(false);
            setSelectedProviderId(null);
            setEditingProvider(emptyProvider());
            setIsDialogOpen(true);
          }}
          onSelectTemplate={(tpl) => {
            setIsTemplatePickerOpen(false);
            setCreatingTemplate(tpl);
          }}
          onNewTemplate={() => {
            setEditingTemplate(null);
            setIsTemplateEditOpen(true);
          }}
        />

        {/* 模型池创建与编辑模态弹窗 */}
        <ModelPoolDialog
          key={poolDialogKey}
          open={isPoolDialogOpen}
          onOpenChange={(open) => {
            setIsPoolDialogOpen(open);
            if (!open) setEditingPool(null);
          }}
          providers={config.providers}
          aliases={[
            ...(config?.model_pools ?? []).map((item) => item.model),
            ...(config.model_pool_reserved_models ?? []),
          ]}
          templates={templates}
          pool={editingPool}
          poolStatus={
            poolStatuses.find((status) => status.id === editingPool?.id) ?? null
          }
          busy={busy}
          onSave={handleSavePool}
          onDelete={handleDeletePool}
        />

        {/* 服务商模板维护与编辑模态弹窗 */}
        <ProviderTemplateEditDialog
          open={isTemplateEditOpen}
          onOpenChange={setIsTemplateEditOpen}
          template={editingTemplate}
          providers={config.providers}
          busy={busy}
          onSave={handleUpsertTemplate}
          onDelete={handleDeleteTemplate}
        />

        {/* 从模板创建上游服务商模态弹窗 */}
        <TemplateCreateDialog
          open={Boolean(creatingTemplate)}
          onOpenChange={(open) => {
            if (!open) setCreatingTemplate(null);
          }}
          template={creatingTemplate}
          busy={busy}
          onConfirm={async (req) => {
            const ok = await handleCreateProviderFromTemplate(req);
            if (ok) setCreatingTemplate(null);
            return ok;
          }}
        />

        {/* 服务商模板管理模态弹窗 */}
        <Dialog open={isTemplateManageOpen} onOpenChange={setIsTemplateManageOpen}>
          <DialogContent
            className="max-h-[90vh] w-full sm:max-w-5xl lg:max-w-6xl overflow-hidden flex flex-col sm:rounded-2xl p-0 gap-0"
            data-testid="ai-gateway-templates-dialog"
          >
            <DialogHeader className="pl-6 pr-14 py-4 border-b bg-card/80 backdrop-blur-sm shrink-0">
              <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <div className="flex items-center gap-3 min-w-0">
                  <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-2xs">
                    <Sparkles className="h-4.5 w-4.5" />
                  </div>
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <DialogTitle className="truncate text-base font-semibold leading-5 text-foreground">
                        {t("aiGatewayTemplateTab", "Provider Templates")}
                      </DialogTitle>
                      <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-semibold text-primary">
                        {t("aiGatewayAvailableTemplatesCount", {
                          count: templates.length,
                          defaultValue: `共 ${templates.length} 个可用模板`,
                        })}
                      </span>
                    </div>
                    <DialogDescription className="text-xs text-muted-foreground mt-0.5 line-clamp-1">
                      {t(
                        "aiGatewayTemplatesDialogDesc",
                        "Manage official and custom templates. Sync official models and add upstream providers with one click.",
                      )}
                    </DialogDescription>
                  </div>
                </div>

                {/* 顶部操作栏常驻按钮 */}
                <div className="flex items-center gap-2 shrink-0">
                  {templates.length > 0 && (
                    <button
                      type="button"
                      data-testid="template-section-toggle-all-btn"
                      onClick={handleToggleAllTemplates}
                      disabled={busy}
                      className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-border/80 bg-background px-3 text-xs font-medium text-foreground shadow-2xs transition hover:bg-muted active:scale-98 disabled:opacity-50"
                    >
                      <ChevronsUpDown className="h-3.5 w-3.5 text-muted-foreground" />
                      <span>
                        {allTemplatesExpanded
                          ? t("aiGatewayTemplateCollapseAll", "Collapse all")
                          : t("aiGatewayTemplateExpandAll", "Expand all")}
                      </span>
                    </button>
                  )}

                  <button
                    type="button"
                    data-testid="template-section-new-btn"
                    onClick={() => {
                      setEditingTemplate(null);
                      setIsTemplateEditOpen(true);
                    }}
                    disabled={busy}
                    className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-2xs transition hover:bg-primary/90 active:scale-98 disabled:opacity-50"
                  >
                    <Plus className="h-3.5 w-3.5" />
                    <span>{t("aiGatewayNewTemplate", "New template")}</span>
                  </button>
                </div>
              </div>
            </DialogHeader>
            {templatesLoadError ? (
              <div
                data-testid="ai-gateway-templates-load-error"
                title={templatesLoadError}
                className="mx-6 mt-4 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3.5 py-2.5 text-xs text-amber-700 dark:text-amber-400"
              >
                {t(
                  "aiGatewayTemplatesLoadFailed",
                  "Failed to load provider templates.",
                )}
              </div>
            ) : null}
            <div className="flex-1 overflow-y-auto p-6">
              <ProviderTemplateSection
                templates={templates}
                hideHeader
                expandedIds={templateExpandedIds}
                onToggleExpand={handleToggleTemplateExpand}
                busy={busy}
                syncingTemplateIds={syncingTemplates}
                onSync={handleSyncTemplate}
                onCreateProvider={async (req) => {
                  const ok = await handleCreateProviderFromTemplate(req);
                  if (ok) {
                    setIsTemplateManageOpen(false);
                  }
                  return ok;
                }}
                onEditTemplate={(tpl) => {
                  setEditingTemplate(tpl);
                  setIsTemplateEditOpen(true);
                }}
                onNewTemplate={() => {
                  setEditingTemplate(null);
                  setIsTemplateEditOpen(true);
                }}
              />
            </div>
          </DialogContent>
        </Dialog>
      </div>
    </div>
  );
}
