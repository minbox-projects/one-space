import { useCallback, useMemo } from "react";
import { emit } from "@tauri-apps/api/event";
import { errorToMessage } from "@/lib/messages";
import {
  AI_GATEWAY_DEFAULT_PORT,
  AI_GATEWAY_STATUS_UPDATED_EVENT,
  aiGatewayConfigureTerminal,
  aiGatewayCreateProviderFromTemplate,
  aiGatewayDeleteKey,
  aiGatewayDeleteProvider,
  aiGatewayDeleteProviderModel,
  aiGatewayDeleteProviderTemplate,
  aiGatewayDeleteModelPool,
  aiGatewayGetConfig,
  aiGatewayModelPoolsStatus,
  aiGatewayReenableProviderKey,
  aiGatewayReenableProviderModel,
  aiGatewayReenableProviderModels,
  aiGatewayRestoreProviderModel,
  aiGatewaySetDefaultKey,
  aiGatewaySetModelPoolEnabled,
  aiGatewaySetProviderEnabled,
  aiGatewayStart,
  aiGatewayStatus,
  aiGatewayStop,
  aiGatewaySyncProviderTemplate,
  aiGatewaySyncTerminal,
  aiGatewayTerminalTargets,
  aiGatewayUpsertKey,
  aiGatewayUpsertModelPool,
  aiGatewayUpsertProvider,
  aiGatewayUpsertProviderTemplate,
  localAnthropicMessagesUrl,
  localBaseUrl,
  providerKeyPool,
  resolveDefaultKeyId,
  type ClaudeModelSelection,
  type CreateProviderFromTemplateRequest,
  type GatewayConfig,
  type GatewayKey,
  type GatewayModelPool,
  type GatewayProviderTemplate,
  type GatewayUpstreamProvider,
  type GatewayUpstreamProviderWithKeys,
  type ModelPrice,
} from "@/lib/aiGateway";
import {
  findMappingLabel as findMappingLabelInConfig,
  findProviderName as findProviderNameInConfig,
  formatMappingLabel as formatMappingLabelValue,
} from "../helpers/labels";
import type { AiGatewayPageData } from "./useAiGatewayPageData";

export function useAiGatewayPageActions(data: AiGatewayPageData) {
  const {
    t,
    pushToast,
    confirmDialog,
    config,
    status,
    setStatus,
    setTargets,
    setSyncingTools,
    setAddressCopied,
    setAnthropicAddressCopied,
    setCopiedKeyId,
    setEditingPool,
    setIsPoolDialogOpen,
    setPoolDialogKey,
    setEditingProvider,
    setSelectedProviderId,
    setIsDialogOpen,
    setTemplates,
    setSyncingTemplates,
    setPoolStatuses,
    setUsageFocusModel,
    setUsageFocusProviderId,
    setUsageFocusProviderName,
    setUsageFocusDay,
    setUsageFocusToken,
    templates,
    targets,
    loadConfigScoped,
    selectTab,
    selectUsageSubTab,
    applyConfig,
    applyConfigMutation,
    runAction,
    refreshTodayUsage,
  } = data;

  const formatMappingLabel = (localModel: string, upstreamModel: string) =>
    formatMappingLabelValue(localModel, upstreamModel);

  const findProviderName = (providerId: string) =>
    findProviderNameInConfig(config, providerId);

  const findMappingLabel = (providerId: string, upstreamModel: string) =>
    findMappingLabelInConfig(config, providerId, upstreamModel);

  const handleToggleService = () => {
    const running = Boolean(status?.running);
    const address = localBaseUrl(
      config?.port ?? status?.port ?? AI_GATEWAY_DEFAULT_PORT,
    );
    return runAction(
      async () => {
        const runningNow = Boolean(status?.running);
        const nextStatus = runningNow
          ? await aiGatewayStop()
          : await aiGatewayStart();
        setStatus(nextStatus);
        applyConfigMutation(await aiGatewayGetConfig());
        await emit(AI_GATEWAY_STATUS_UPDATED_EVENT).catch(() => {});
        void refreshTodayUsage(true);
      },
      running
        ? t("aiGatewayServiceStopped", "Gateway service stopped")
        : t("aiGatewayServiceStarted", "Gateway service started"),
      running
        ? t("aiGatewayServiceStopFailed", "Failed to stop gateway service")
        : t("aiGatewayServiceStartFailed", "Failed to start gateway service"),
      running ? undefined : address,
    );
  };

  const handleToggleProviderEnabled = (provider: GatewayUpstreamProvider, enabled: boolean) =>
    runAction(
      async () => {
        await applyConfig(await aiGatewaySetProviderEnabled(provider.id, enabled));
      },
      enabled
        ? t("aiGatewayProviderEnabled", "Provider enabled: {{name}}", {
            name: provider.name,
          })
        : t("aiGatewayProviderDisabled", "Provider disabled: {{name}}", {
            name: provider.name,
          }),
      t("aiGatewayProviderOperationFailed", "Upstream provider operation failed"),
    );

  const handleReenableProviderModel = (
    providerId: string,
    localModel: string,
    upstreamModel: string,
  ) =>
    runAction(
      async () => {
        const next = await aiGatewayReenableProviderModel(
          providerId,
          localModel,
          upstreamModel,
        );
        await applyConfig(next);
        setEditingProvider(
          next.providers.find((provider) => provider.id === providerId) ?? null,
        );
      },
      t("aiGatewayMappingReenabled", "Mapping re-enabled: {{mapping}}", {
        mapping: formatMappingLabel(localModel, upstreamModel),
      }),
      t("aiGatewayMappingOperationFailed", "Model mapping operation failed"),
      findProviderName(providerId),
    );

  const handleReenableProviderModels = (providerId: string) => {
    const provider =
      config?.providers.find((item) => item.id === providerId) ?? null;
    const providerName = provider?.name ?? providerId;
    const autoDisabledCount =
      provider?.mappings.filter((mapping) => mapping.auto_disabled).length ?? 0;
    return runAction(
      async () => {
        const next = await aiGatewayReenableProviderModels(providerId);
        await applyConfig(next);
        setEditingProvider(
          next.providers.find((item) => item.id === providerId) ?? null,
        );
      },
      t(
        "aiGatewayMappingsReenabledAll",
        "All auto-disabled mappings re-enabled: {{provider}} ({{count}})",
        { provider: providerName, count: autoDisabledCount },
      ),
      t("aiGatewayMappingOperationFailed", "Model mapping operation failed"),
    );
  };

  const handleReenableProviderKey = (providerId: string, keyId: string) => {
    const provider =
      config?.providers.find((item) => item.id === providerId) ?? null;
    const keyName =
      providerKeyPool(provider).find((key) => key.id === keyId)?.name ?? keyId;
    return runAction(
      async () => {
        const next = await aiGatewayReenableProviderKey(providerId, keyId);
        await applyConfig(next);
        setEditingProvider(
          next.providers.find((item) => item.id === providerId) ?? null,
        );
      },
      t("aiGatewayProviderKeyReenabled", "Upstream key re-enabled: {{key}}", {
        key: keyName,
      }),
      t("aiGatewayProviderKeyOperationFailed", "Upstream key operation failed"),
      provider?.name,
    );
  };

  const handleSaveProvider = (
    draft: GatewayUpstreamProviderWithKeys,
    prices: ModelPrice[],
  ) => {
    const isNew = !draft.id;
    const providerName = draft.name.trim() || draft.id;
    return runAction(
      async () => {
        const next = await aiGatewayUpsertProvider(draft, prices);
        applyConfigMutation(next);
        const saved = draft.id
          ? next.providers.find((provider) => provider.id === draft.id) ?? null
          : null;
        setEditingProvider(saved);
        setSelectedProviderId(saved?.id ?? null);
        const [nextStatus, nextTargets] = await Promise.all([
          aiGatewayStatus(),
          aiGatewayTerminalTargets(),
        ]);
        setStatus(nextStatus);
        setTargets(nextTargets);
      },
      isNew
        ? t("aiGatewayProviderCreated", "Provider created: {{name}}", {
            name: providerName,
          })
        : t("aiGatewayProviderSaved", "Provider saved: {{name}}", {
            name: providerName,
          }),
      t("aiGatewayProviderOperationFailed", "Upstream provider operation failed"),
    );
  };

  const handleDeleteProvider = (providerId: string) => {
    const providerName = findProviderName(providerId);
    return runAction(
      async () => {
        await applyConfig(await aiGatewayDeleteProvider(providerId));
        setEditingProvider(null);
        setSelectedProviderId(null);
        setIsDialogOpen(false);
      },
      t("aiGatewayProviderDeleted", "Provider deleted: {{name}}", {
        name: providerName,
      }),
      t("aiGatewayProviderOperationFailed", "Upstream provider operation failed"),
    );
  };

  const refreshPoolStatuses = useCallback(async () => {
    try {
      setPoolStatuses(await aiGatewayModelPoolsStatus());
    } catch {
      // Nonsecret status refresh failure must never block configuration edits.
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleTogglePoolEnabled = (pool: GatewayModelPool, enabled: boolean) =>
    runAction(
      async () => {
        await applyConfig(await aiGatewaySetModelPoolEnabled(pool.id, enabled));
        await refreshPoolStatuses();
      },
      enabled
        ? t("aiGatewayModelPoolEnabled", "Model pool enabled: {{name}}", {
            name: pool.name,
          })
        : t("aiGatewayModelPoolDisabled", "Model pool disabled: {{name}}", {
            name: pool.name,
          }),
      t("aiGatewayModelPoolOperationFailed", "Model pool operation failed"),
    );

  const handleOpenNewPool = () => {
    setEditingPool(null);
    setPoolDialogKey((key) => key + 1);
    setIsPoolDialogOpen(true);
  };

  const handleOpenEditPool = (pool: GatewayModelPool) => {
    setEditingPool(pool);
    setPoolDialogKey((key) => key + 1);
    setIsPoolDialogOpen(true);
  };

  const handleSavePool = async (pool: GatewayModelPool) => {
    // A rejected write propagates to the dialog, which keeps the draft visible.
    const next = await aiGatewayUpsertModelPool(pool);
    applyConfigMutation(next);
    await refreshPoolStatuses();
    pushToast({
      title: t("aiGatewayModelPoolSaved", "Model pool saved: {{name}}", {
        name: pool.name,
      }),
      kind: "success",
    });
  };

  const handleDeletePool = (poolId: string) => {
    const name =
      (config?.model_pools ?? []).find((pool) => pool.id === poolId)?.name ?? poolId;
    return runAction(
      async () => {
        await applyConfig(await aiGatewayDeleteModelPool(poolId));
        await refreshPoolStatuses();
      },
      t("aiGatewayModelPoolDeleted", "Model pool deleted: {{name}}", { name }),
      t("aiGatewayModelPoolOperationFailed", "Model pool operation failed"),
    );
  };

  const handleOpenPoolLogs = (model: string) => {
    // Reuse the existing request-log query state: switch to the logs subtab and
    // seed the alias filter; no new trace-ID or log format is introduced. A pool
    // alias shortcut clears any exact provider/day focus.
    setUsageFocusProviderId(null);
    setUsageFocusProviderName(null);
    setUsageFocusDay(null);
    setUsageFocusModel(model);
    setUsageFocusToken((token) => token + 1);
    selectUsageSubTab("logs");
    selectTab("usage");
  };

  // A Provider history daily row opens the existing request-log view focused on
  // one exact provider ID and UTC+8 day; the alias focus is cleared so the
  // recorded-name/model/status selections cannot compete.
  const handleOpenProviderDay = useCallback(
    (request: { providerId: string; providerName?: string; day: string }) => {
      setUsageFocusModel(null);
      setUsageFocusProviderId(request.providerId);
      setUsageFocusProviderName(request.providerName ?? null);
      setUsageFocusDay(request.day);
      setUsageFocusToken((token) => token + 1);
      selectUsageSubTab("logs");
      selectTab("usage");
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [selectTab, selectUsageSubTab],
  );

  // Returning restores the Provider history subtab; its mounted selection is
  // retained, and the one-shot provider/day focus is released so later ordinary
  // request-log browsing starts from the quick range again.
  const handleReturnToProviderHistory = useCallback(() => {
    setUsageFocusModel(null);
    setUsageFocusProviderId(null);
    setUsageFocusProviderName(null);
    setUsageFocusDay(null);
    setUsageFocusToken((token) => token + 1);
    selectUsageSubTab("history");
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectUsageSubTab]);

  // Clearing the exact provider/day focus stays in the request-log view and
  // releases the one-shot focus (and any stale alias), so the retained quick
  // range applies again on the next ordinary request.
  const handleClearProviderDayFocus = useCallback(() => {
    setUsageFocusModel(null);
    setUsageFocusProviderId(null);
    setUsageFocusProviderName(null);
    setUsageFocusDay(null);
    setUsageFocusToken((token) => token + 1);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const syncEditingProvider = (
    next: GatewayConfig,
    providerId: string,
  ) => {
    const updated =
      next.providers.find((provider) => provider.id === providerId) ?? null;
    setEditingProvider((prev) =>
      prev && prev.id === providerId ? updated : prev,
    );
  };

  const handleDeleteProviderModel = (providerId: string, upstreamModel: string) =>
    runAction(
      async () => {
        const next = await aiGatewayDeleteProviderModel(providerId, upstreamModel);
        await applyConfig(next);
        syncEditingProvider(next, providerId);
      },
      t("aiGatewayMappingDeleted", "Mapping deleted: {{mapping}}", {
        mapping: findMappingLabel(providerId, upstreamModel),
      }),
      t("aiGatewayMappingOperationFailed", "Model mapping operation failed"),
      findProviderName(providerId),
    );

  const handleRestoreProviderModel = (providerId: string, upstreamModel: string) =>
    runAction(
      async () => {
        const next = await aiGatewayRestoreProviderModel(providerId, upstreamModel);
        await applyConfig(next);
        syncEditingProvider(next, providerId);
      },
      t("aiGatewayModelRestored", "Mapping restored: {{mapping}}", {
        mapping: findMappingLabel(providerId, upstreamModel),
      }),
      t("aiGatewayMappingOperationFailed", "Model mapping operation failed"),
      findProviderName(providerId),
    );

  const handleSaveKey = (key: GatewayKey): Promise<boolean> =>
    runAction(
      async () => {
        await applyConfig(await aiGatewayUpsertKey(key));
      },
      t("aiGatewayLocalKeyCreated", "Local key created: {{label}}", {
        label: key.label,
      }),
      t("aiGatewayLocalKeyOperationFailed", "Local key operation failed"),
    );

  const handleDeleteKey = (keyId: string) => {
    const label = config?.keys.find((key) => key.id === keyId)?.label ?? keyId;
    // 本地密钥删除不可撤销：先经全局二次确认框确认，与服务商/模板删除保持一致。
    void (async () => {
      const confirmed = await confirmDialog(
        t(
          "aiGatewayDeleteKeyConfirm",
          'Are you sure you want to delete local key "{{label}}"? This action cannot be undone.',
          { label },
        ),
        { title: t("aiGatewayDeleteKeyTitle", "Delete local key") },
      );
      if (!confirmed) return;
      await runAction(
        async () => {
          await applyConfig(await aiGatewayDeleteKey(keyId));
        },
        t("aiGatewayLocalKeyDeleted", "Local key deleted: {{label}}", { label }),
        t("aiGatewayLocalKeyOperationFailed", "Local key operation failed"),
      );
    })();
  };

  const handleToggleKeyEnabled = (key: GatewayKey, enabled: boolean) =>
    runAction(
      async () => {
        await applyConfig(await aiGatewayUpsertKey({ ...key, enabled }));
      },
      enabled
        ? t("aiGatewayLocalKeyEnabled", "Local key enabled: {{label}}", {
            label: key.label,
          })
        : t("aiGatewayLocalKeyDisabled", "Local key disabled: {{label}}", {
            label: key.label,
          }),
      t("aiGatewayLocalKeyOperationFailed", "Local key operation failed"),
    );

  const handleSetDefaultKey = (keyId: string) => {
    const label = config?.keys.find((key) => key.id === keyId)?.label ?? keyId;
    return runAction(
      async () => {
        await applyConfig(await aiGatewaySetDefaultKey(keyId));
      },
      t("aiGatewayDefaultKeyUpdated", "Default key set to {{label}}", { label }),
      t("aiGatewayLocalKeyOperationFailed", "Local key operation failed"),
    );
  };

  const runTerminalAction = useCallback(
    async (
      tool: string,
      action: () => Promise<void>,
      successTitle: string,
      failureTitle: string,
    ) => {
      setSyncingTools((prev) => ({ ...prev, [tool]: true }));
      try {
        await action();
        pushToast({ title: successTitle, kind: "success" });
      } catch (err) {
        pushToast({
          title: failureTitle,
          description: errorToMessage(err),
          kind: "error",
        });
      } finally {
        setSyncingTools((prev) => {
          const next = { ...prev };
          delete next[tool];
          return next;
        });
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [pushToast],
  );

  const toolDisplayName = (tool: string) =>
    targets.find((target) => target.tool === tool)?.name ?? tool;

  const handleConfigureTool = (
    tool: string,
    claudeModels?: ClaudeModelSelection,
  ) =>
    void runTerminalAction(
      tool,
      async () => {
        await aiGatewayConfigureTerminal([tool], claudeModels);
        await applyConfig(await aiGatewayGetConfig());
      },
      t("aiGatewayConfigureSuccess", "Terminal provider added for {{tool}}.", {
        tool: toolDisplayName(tool),
      }),
      t(
        "aiGatewayTerminalOperationFailed",
        "Terminal integration operation failed for {{tool}}",
        { tool: toolDisplayName(tool) },
      ),
    );

  const handleSyncTool = (tool: string, claudeModels?: ClaudeModelSelection) =>
    void runTerminalAction(
      tool,
      async () => {
        await aiGatewaySyncTerminal([tool], claudeModels);
        await applyConfig(await aiGatewayGetConfig());
      },
      t("aiGatewaySyncSuccess", "Terminal provider synced for {{tool}}.", {
        tool: toolDisplayName(tool),
      }),
      t(
        "aiGatewayTerminalOperationFailed",
        "Terminal integration operation failed for {{tool}}",
        { tool: toolDisplayName(tool) },
      ),
    );

  const handleSyncTemplate = useCallback(
    (templateId: string) => {
      const templateName =
        templates.find((view) => view.template.id === templateId)?.template.name ??
        templateId;
      setSyncingTemplates((prev) => ({ ...prev, [templateId]: true }));
      void (async () => {
        try {
          const updated = await aiGatewaySyncProviderTemplate(templateId);
          setTemplates((prev) =>
            prev.map((view) =>
              view.template.id === templateId ? updated : view,
            ),
          );
          await applyConfig(await aiGatewayGetConfig());
          pushToast({
            title: t(
              "aiGatewayTemplateSyncSuccess",
              "Provider template synced: {{name}}.",
              { name: templateName },
            ),
            kind: "success",
          });
        } catch (err) {
          pushToast({
            title: t(
              "aiGatewayTemplateOperationFailed",
              "Provider template operation failed: {{name}}",
              { name: templateName },
            ),
            description: errorToMessage(err),
            kind: "error",
          });
        } finally {
          setSyncingTemplates((prev) => {
            const next = { ...prev };
            delete next[templateId];
            return next;
          });
        }
      })();
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [applyConfig, pushToast, t, templates],
  );

  const handleCreateProviderFromTemplate = useCallback(
    async (request: CreateProviderFromTemplateRequest): Promise<boolean> => {
      const templateName =
        templates.find((view) => view.template.id === request.templateId)
          ?.template.name ?? request.templateId;
      try {
        const created = await aiGatewayCreateProviderFromTemplate(request);
        const existingIds = new Set(
          (config?.providers ?? []).map((provider) => provider.id),
        );
        const newProvider =
          created.providers.find((provider) => !existingIds.has(provider.id)) ?? null;
        await applyConfig(await aiGatewayGetConfig());
        if (newProvider) {
          setSelectedProviderId(newProvider.id);
          setEditingProvider(newProvider);
          setIsDialogOpen(true);
        }
        pushToast({
          title: t(
            "aiGatewayTemplateProviderCreated",
            "Provider {{provider}} created from template {{template}}.",
            {
              provider: newProvider?.name ?? request.name,
              template: templateName,
            },
          ),
          kind: "success",
        });
        return true;
      } catch (err) {
        pushToast({
          title: t(
            "aiGatewayTemplateCreateFailed",
            "Failed to create provider from template {{template}}.",
            { template: templateName },
          ),
          description: errorToMessage(err),
          kind: "error",
        });
        return false;
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [applyConfig, config, pushToast, t, templates],
  );

  const handleUpsertTemplate = async (
    template: GatewayProviderTemplate,
  ): Promise<boolean> => {
    try {
      const nextViews = await aiGatewayUpsertProviderTemplate(template);
      setTemplates(nextViews);
      // A manual template save can rewrite bound mappings and their projected
      // terminal configuration, so reload configuration and target summaries
      // rather than showing stale inherited values.
      void loadConfigScoped();
      pushToast({
        title: t("aiGatewayTemplateSaved", "Template saved: {{name}}.", {
          name: template.name,
        }),
        kind: "success",
      });
      return true;
    } catch (err) {
      pushToast({
        title: t(
          "aiGatewayTemplateOperationFailed",
          "Provider template operation failed: {{name}}",
          { name: template.name },
        ),
        description: errorToMessage(err),
        kind: "error",
      });
      return false;
    }
  };

  const handleDeleteTemplate = async (templateId: string): Promise<boolean> => {
    const templateName =
      templates.find((view) => view.template.id === templateId)?.template.name ??
      templateId;
    try {
      const nextViews = await aiGatewayDeleteProviderTemplate(templateId);
      setTemplates(nextViews);
      pushToast({
        title: t("aiGatewayTemplateDeleted", "Template deleted: {{name}}.", {
          name: templateName,
        }),
        kind: "success",
      });
      return true;
    } catch (err) {
      pushToast({
        title: t(
          "aiGatewayTemplateOperationFailed",
          "Provider template operation failed: {{name}}",
          { name: templateName },
        ),
        description: errorToMessage(err),
        kind: "error",
      });
      return false;
    }
  };

  const handleCopyAddress = async () => {
    if (!config) return;
    const address = localBaseUrl(config.port);
    try {
      await navigator.clipboard.writeText(address);
      setAddressCopied(true);
      setTimeout(() => setAddressCopied(false), 2000);
      pushToast({
        title: t("aiGatewayAddressCopied", "Local API address copied"),
        description: address,
        kind: "success",
      });
    } catch (err) {
      pushToast({
        title: t(
          "aiGatewayAddressCopyFailed",
          "Failed to copy local API address",
        ),
        description: errorToMessage(err),
        kind: "error",
      });
    }
  };

  const handleCopyAnthropicAddress = async () => {
    if (!config) return;
    const address = localAnthropicMessagesUrl(config.port);
    try {
      await navigator.clipboard.writeText(address);
      setAnthropicAddressCopied(true);
      setTimeout(() => setAnthropicAddressCopied(false), 2000);
      pushToast({
        title: t(
          "aiGatewayAnthropicAddressCopied",
          "Anthropic Messages API address copied",
        ),
        description: address,
        kind: "success",
      });
    } catch (err) {
      pushToast({
        title: t(
          "aiGatewayAddressCopyFailed",
          "Failed to copy local API address",
        ),
        description: errorToMessage(err),
        kind: "error",
      });
    }
  };


  const handleCopyKey = async (key: GatewayKey) => {
    try {
      await navigator.clipboard.writeText(key.value);
      setCopiedKeyId(key.id);
      pushToast({
        title: t("aiGatewayKeyCopied", "API key copied: {{label}}", {
          label: key.label,
        }),
        kind: "success",
      });
    } catch (err) {
      pushToast({
        title: t("aiGatewayKeyCopyFailed", "Failed to copy API key"),
        description: errorToMessage(err),
        kind: "error",
      });
    }
  };

  const defaultKeyId = config
    ? resolveDefaultKeyId(config.keys, config.default_key_id)
    : null;

  // 全部服务商已使用的标签，作为编辑弹窗「推荐标签」的候选，让用户能复用其他
  // 服务商已有的标签，而不是只能手输。
  const availableProviderTags = useMemo(() => {
    const set = new Set<string>();
    for (const provider of config?.providers ?? []) {
      for (const tag of provider.tags ?? []) {
        const trimmed = tag.trim();
        if (trimmed) set.add(trimmed);
      }
    }
    return Array.from(set).sort();
  }, [config?.providers]);
  return {
    handleToggleService,
    handleToggleProviderEnabled,
    handleReenableProviderModel,
    handleReenableProviderModels,
    handleReenableProviderKey,
    handleSaveProvider,
    handleDeleteProvider,
    handleTogglePoolEnabled,
    handleOpenNewPool,
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
    handleCopyAddress,
    handleCopyAnthropicAddress,
    handleCopyKey,
    defaultKeyId,
    availableProviderTags,
  };
}
