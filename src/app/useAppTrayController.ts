import { useCallback, useEffect } from "react";
import type { Dispatch, SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { TFunction } from "i18next";
import {
  aiGatewayStart,
  aiGatewayStop,
  AI_GATEWAY_DEFAULT_PORT,
  AI_GATEWAY_STATUS_UPDATED_EVENT,
} from "@/lib/aiGateway";
import type { GatewayStatus } from "@/lib/aiGateway";
import { protocolRouterStart, protocolRouterStop } from "@/lib/protocolRouter";
import type { ProtocolRouterStatus } from "@/lib/protocolRouter";
import { fileSharingStop } from "@/lib/fileSharing";
import type { FileSharingSnapshot } from "@/lib/fileSharing";
import {
  sshTunnelsConnectAll,
  sshTunnelsDisconnectAll,
} from "@/lib/sshTunnels";
import type { SshTunnelBatchOperationResult, SshTunnelsSnapshot } from "@/components/sshTunnels/types";
import {
  publishRuntimeStatus,
  refreshRuntimeStatus,
  setNativeWindowVisible,
} from "@/lib/runtimeStatus";
import {
  BLE_LOCK_STATUS_EVENT,
  BleLockCommandError,
  bleLockLockNow,
  bleLockSaveConfig,
} from "@/lib/bleLock";
import type { BleLockConfig } from "@/lib/bleLock";
import {
  applyTrayMenu,
  buildTrayMenuModel,
} from "@/lib/trayMenu";
import type { TrayMenuState, TrayTranslate } from "@/lib/trayMenu";
import {
  AI_WORKFLOW_PROFILE_UPDATED_EVENT,
  SUPPORTED_TOOLS,
  getActiveModels,
} from "@/lib/aiWorkflowProfiles";
import { TUNNEL_STATUS_CHANGED_EVENT } from "@/lib/portTunnel";
import { errorToMessage } from "@/lib/messages";
import type { AppI18n, TrayShortcutConfig } from "./types";
import { BLE_LOCK_TRAY_TOGGLES, TRAY_NAVIGATION_IDS } from "./constants";

type PushToast = (toast: {
  title: string;
  description?: string;
  kind?: "info" | "success" | "warning" | "error" | "loading";
  durationMs?: number;
  action?: { label: string; onClick: () => void };
}) => string;

type UseAppTrayControllerParams = {
  isTauri: boolean;
  isQuickAiView: boolean;
  trayState: TrayMenuState;
  setTrayState: Dispatch<SetStateAction<TrayMenuState>>;
  trayStateRef: { current: TrayMenuState };
  trayMenuApplyRef: { current: Promise<void> };
  trayActionRef: { current: (id: string) => Promise<void> };
  gatewayBaseUrlRef: { current: string | null };
  t: TFunction;
  i18n: AppI18n;
  pushToast: PushToast;
  navigateFromTray: (target: string) => void;
  setAboutAutoCheck: (value: boolean) => void;
  setAboutOpen: (open: boolean) => void;
};

export function useAppTrayController({
  isTauri,
  isQuickAiView,
  trayState,
  setTrayState,
  trayStateRef,
  trayMenuApplyRef,
  trayActionRef,
  gatewayBaseUrlRef,
  t,
  i18n,
  pushToast,
  navigateFromTray,
  setAboutAutoCheck,
  setAboutOpen,
}: UseAppTrayControllerParams) {
  // Tray menu controller: seed state, apply the native menu and keep it in sync.
  useEffect(() => {
    trayStateRef.current = trayState;
  }, [trayState]);

  const applyTrayModel = useCallback(
    (state: TrayMenuState) => {
      if (!isTauri || isQuickAiView) return;
      const model = buildTrayMenuModel(state, t as TrayTranslate);
      // A slow native submenu build must not overwrite a newer menu.
      trayMenuApplyRef.current = trayMenuApplyRef.current.then(async () => {
        await applyTrayMenu(model, (id) => trayActionRef.current(id));
      });
    },
    [isQuickAiView, isTauri, t],
  );

  useEffect(() => {
    applyTrayModel(trayState);
  }, [applyTrayModel, trayState]);

  const refreshTrayGateway = useCallback(async () => {
    await refreshRuntimeStatus<GatewayStatus>("gateway", { force: true });
  }, []);

  const refreshTrayRouter = useCallback(async () => {
    await refreshRuntimeStatus<ProtocolRouterStatus>("router", { force: true });
  }, []);

  const refreshTrayTunnels = useCallback(async () => {
    await refreshRuntimeStatus<SshTunnelsSnapshot>("ssh-tunnels", {
      force: true,
    });
  }, []);

  const refreshTraySharing = useCallback(async () => {
    await refreshRuntimeStatus<FileSharingSnapshot>("file-sharing", {
      force: true,
    });
  }, []);

  const refreshTrayVisibility = useCallback(async () => {
    try {
      const visible = await getCurrentWindow().isVisible();
      if (typeof visible !== "boolean") return;
      setNativeWindowVisible(visible);
      setTrayState((prev) =>
        prev.windowVisible === visible
          ? prev
          : { ...prev, windowVisible: visible },
      );
    } catch {
      // Keep the last known state when the visibility query fails.
    }
  }, []);

  const handleTrayAction = useCallback(
    async (id: string) => {
      if (id === "toggle-window") {
        if (trayStateRef.current.windowVisible) {
          await invoke("hide_window");
          setTrayState((prev) => ({ ...prev, windowVisible: false }));
        } else {
          await invoke("show_main_window");
          setTrayState((prev) => ({ ...prev, windowVisible: true }));
        }
        return;
      }
      if (id === "quick-ai") {
        await invoke("toggle_quick_ai_window");
        return;
      }
      if (id === "check-for-updates") {
        setAboutAutoCheck(true);
        setAboutOpen(true);
        return;
      }
      if (id === "about") {
        setAboutAutoCheck(false);
        setAboutOpen(true);
        return;
      }
      if (id === "gateway") {
        const service = trayStateRef.current.gateway;
        try {
          if (service.running) {
            await aiGatewayStop();
          } else {
            await aiGatewayStart();
          }
        } catch (error) {
          const message =
            error instanceof Error ? error.message : errorToMessage(error);
          pushToast({
            title: service.running
              ? t("error", { message })
              : t("tray.action.gatewayStartFailed", {
                  port: service.port ?? AI_GATEWAY_DEFAULT_PORT,
                  error: message,
                }),
            kind: "error",
          });
        }
        await refreshTrayGateway();
        return;
      }
      if (id === "router") {
        const service = trayStateRef.current.router;
        try {
          if (service.running) {
            await protocolRouterStop();
          } else {
            await protocolRouterStart();
          }
        } catch (error) {
          const message =
            error instanceof Error ? error.message : errorToMessage(error);
          pushToast({ title: t("error", { message }), kind: "error" });
        }
        await refreshTrayRouter();
        return;
      }
      if (id === "connect-all" || id === "disconnect-all") {
        try {
          const result =
            id === "connect-all"
              ? await sshTunnelsConnectAll<SshTunnelBatchOperationResult>()
              : await sshTunnelsDisconnectAll<SshTunnelBatchOperationResult>();
          if (result.failed_count > 0) {
            pushToast({
              title: t("tray.action.sshBatchPartial", {
                success: result.success_count,
                failed: result.failed_count,
              }),
              kind: "error",
            });
          }
        } catch (error) {
          const message =
            error instanceof Error ? error.message : errorToMessage(error);
          pushToast({ title: t("error", { message }), kind: "error" });
        }
        await refreshTrayTunnels();
        return;
      }
      if (id === "stop-sharing") {
        try {
          await fileSharingStop();
        } catch (error) {
          const message =
            error instanceof Error ? error.message : errorToMessage(error);
          pushToast({ title: t("error", { message }), kind: "error" });
        }
        await refreshTraySharing();
        return;
      }
      if (id === "sync") {
        try {
          await invoke("sync_run_now");
        } catch (error) {
          const message =
            error instanceof Error ? error.message : errorToMessage(error);
          pushToast({ title: t("error", { message }), kind: "error" });
        }
        return;
      }
      if (id === "copy-address") {
        const url = gatewayBaseUrlRef.current;
        if (url) {
          await navigator.clipboard.writeText(url);
        }
        return;
      }
      if (id === "quit") {
        await invoke("quit_app");
        return;
      }
      if (id === "settings") {
        await invoke("show_main_window");
        navigateFromTray("settings");
        return;
      }
      if (id === "ble-lock-set-device") {
        await invoke("show_main_window");
        navigateFromTray("ble-lock");
        return;
      }
      if (id === "ble-lock-lock-now") {
        try {
          const result = await bleLockLockNow();
          publishRuntimeStatus("ble-lock", result);
        } catch (error) {
          const title =
            error instanceof BleLockCommandError && error.code
              ? t(`bleLockError_${error.code}`)
              : errorToMessage(error);
          pushToast({ title, kind: "error" });
        }
        return;
      }
      if (id.startsWith("ble-lock-")) {
        const config = trayStateRef.current.bleLock?.config;
        if (!config) return;

        const applyBleLockConfig = async (next: BleLockConfig) => {
          try {
            const result = await bleLockSaveConfig(next);
            publishRuntimeStatus("ble-lock", result);
          } catch (error) {
            const title =
              error instanceof BleLockCommandError && error.code
                ? t(`bleLockError_${error.code}`)
                : errorToMessage(error);
            pushToast({ title, kind: "error" });
          }
        };

        if (
          id.startsWith("ble-lock-unlock-rssi") ||
          id.startsWith("ble-lock-lock-rssi")
        ) {
          const prefix = id.startsWith("ble-lock-unlock-rssi")
            ? "ble-lock-unlock-rssi"
            : "ble-lock-lock-rssi";
          const raw = id.slice(prefix.length + 1);
          const value = raw === "disable" ? null : Number(raw);
          if (value !== null && !Number.isInteger(value)) return;
          const field =
            prefix === "ble-lock-unlock-rssi" ? "unlock_rssi" : "lock_rssi";
          await applyBleLockConfig({ ...config, [field]: value });
          return;
        }

        const toggle = BLE_LOCK_TRAY_TOGGLES[id];
        if (toggle) {
          const next: BleLockConfig = { ...config, [toggle]: !config[toggle] };
          // Screensaver lock and display-off are mutually exclusive.
          if (toggle === "use_screensaver_to_lock" && next.use_screensaver_to_lock) {
            next.sleep_display = false;
          }
          if (toggle === "sleep_display" && next.sleep_display) {
            next.use_screensaver_to_lock = false;
          }
          await applyBleLockConfig(next);
        }
        return;
      }
      if (TRAY_NAVIGATION_IDS.has(id)) {
        await invoke("show_main_window");
        navigateFromTray(id);
      }
    },
    [
      navigateFromTray,
      pushToast,
      refreshTrayGateway,
      refreshTrayRouter,
      refreshTraySharing,
      refreshTrayTunnels,
      t,
    ],
  );

  useEffect(() => {
    trayActionRef.current = handleTrayAction;
  }, [handleTrayAction]);

  useEffect(() => {
    if (!isTauri || isQuickAiView) return;

    let disposed = false;
    let aiWorkflowRequest = 0;
    const refreshTrayAiWorkflow = async () => {
      const request = ++aiWorkflowRequest;
      try {
        const profile = await getActiveModels();
        if (disposed || request !== aiWorkflowRequest) return;
        setTrayState((prev) => {
          const previous = prev.aiWorkflow?.profile;
          const unchanged = previous === profile || (
            previous && profile &&
            previous.name === profile.name &&
            previous.rows.length === profile.rows.length &&
            previous.rows.every((row, index) => {
              const next = profile.rows[index];
              return row.role === next.role && SUPPORTED_TOOLS.every((tool) =>
                row[tool]?.model === next[tool]?.model &&
                row[tool]?.reasoning_effort === next[tool]?.reasoning_effort,
              );
            })
          );
          return unchanged && !prev.aiWorkflow?.unavailable
            ? prev
            : { ...prev, aiWorkflow: { profile } };
        });
      } catch {
        if (disposed || request !== aiWorkflowRequest) return;
        setTrayState((prev) => prev.aiWorkflow?.unavailable
          ? prev
          : { ...prev, aiWorkflow: { profile: null, unavailable: true } },
        );
      }
    };

    const unlistenFns: Array<() => void> = [];
    const addListener = (
      eventName: string,
      handler: (event: { payload?: unknown }) => void,
    ) => {
      listen(eventName, handler)
        .then((fn) => {
          if (disposed) fn();
          else unlistenFns.push(fn);
        })
        .catch((e) => {
          console.error(`Failed to subscribe to ${eventName}`, e);
        });
    };

    addListener("main-window-visibility-changed", (event) => {
      void refreshTrayAiWorkflow();
      const payload = event.payload;
      if (typeof payload === "boolean") {
        setNativeWindowVisible(payload);
        setTrayState((prev) =>
          prev.windowVisible === payload
            ? prev
            : { ...prev, windowVisible: payload },
        );
      } else {
        void refreshTrayVisibility();
      }
    });
    // One shared event bridge per service: publish the backend payload into the
    // runtime-status store. Valid snapshots are consumed with zero queries; the
    // store coalesces one fallback pull for missing/unit payloads.
    addListener(AI_GATEWAY_STATUS_UPDATED_EVENT, (event) => {
      publishRuntimeStatus("gateway", event.payload);
    });
    addListener("protocol-router-status-update", (event) => {
      publishRuntimeStatus("router", event.payload);
    });
    addListener("ssh-tunnels-updated", (event) => {
      publishRuntimeStatus("ssh-tunnels", event.payload);
    });
    addListener("file-sharing-updated", (event) => {
      publishRuntimeStatus("file-sharing", event.payload);
    });
    addListener(BLE_LOCK_STATUS_EVENT, (event) => {
      publishRuntimeStatus("ble-lock", event.payload);
    });
    addListener(TUNNEL_STATUS_CHANGED_EVENT, () => {
      void refreshRuntimeStatus("port-tunnel");
    });
    addListener("tray-shortcuts-updated", (event) => {
      const payload = (event.payload ?? {}) as {
        main?: string | null;
        quick?: string | null;
      };
      setTrayState((prev) => ({
        ...prev,
        shortcuts: { main: payload.main ?? null, quick: payload.quick ?? null },
      }));
    });

    const handleLanguageChanged = () => {
      applyTrayModel(trayStateRef.current);
    };
    i18n.on("languageChanged", handleLanguageChanged);

    window.addEventListener(AI_WORKFLOW_PROFILE_UPDATED_EVENT, refreshTrayAiWorkflow);
    const aiWorkflowTimer = window.setInterval(refreshTrayAiWorkflow, 60_000);
    void refreshTrayAiWorkflow();
    void refreshTrayVisibility();
    invoke<TrayShortcutConfig>("get_storage_config")
      .then((cfg) => {
        if (!cfg) return;
        setTrayState((prev) => ({
          ...prev,
          shortcuts: {
            main: cfg.main_shortcut ?? null,
            quick: cfg.quick_ai_shortcut ?? null,
          },
        }));
      })
      .catch(() => {});

    return () => {
      disposed = true;
      window.clearInterval(aiWorkflowTimer);
      window.removeEventListener(AI_WORKFLOW_PROFILE_UPDATED_EVENT, refreshTrayAiWorkflow);
      unlistenFns.forEach((fn) => fn());
      i18n.off("languageChanged", handleLanguageChanged);
    };
  }, [
    applyTrayModel,
    i18n,
    isQuickAiView,
    isTauri,
    refreshTrayGateway,
    refreshTrayRouter,
    refreshTraySharing,
    refreshTrayTunnels,
    refreshTrayVisibility,
  ]);
}
