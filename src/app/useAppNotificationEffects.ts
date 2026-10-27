import { useEffect } from "react";
import type { TFunction } from "i18next";
import {
  NETWORK_CIRCUIT_EVENT,
  NETWORK_CIRCUIT_MESSAGE,
} from "@/lib/networkCircuitBreaker";
import { notifySystemEvent } from "@/lib/userActions";
import { recordMessage } from "@/lib/messages";

type UseAppNotificationEffectsParams = {
  pushToast: (toast: {
    title: string;
    description?: string;
    kind?: "info" | "success" | "warning" | "error" | "loading";
  }) => string;
  t: TFunction;
  setNetworkCircuitOpen: (open: boolean) => void;
  ignoredUpdateVersion: string | null;
  ignoredUpdateVersionRef: { current: string | null };
  skillsAutoUpdateNotice: string | null;
  setSkillsAutoUpdateNotice: (notice: string | null) => void;
  runtimeError: string | null;
  setRuntimeError: (error: string | null) => void;
  setRuntimeErrorCopied: (copied: boolean) => void;
};

export function useAppNotificationEffects({
  pushToast,
  t,
  setNetworkCircuitOpen,
  ignoredUpdateVersion,
  ignoredUpdateVersionRef,
  skillsAutoUpdateNotice,
  setSkillsAutoUpdateNotice,
  runtimeError,
  setRuntimeError,
  setRuntimeErrorCopied,
}: UseAppNotificationEffectsParams) {
  useEffect(() => {
    const onNetworkCircuitOpen = () => {
      setNetworkCircuitOpen(true);
      void notifySystemEvent(
        { pushToast, recordMessage },
        {
          source: "system",
          category: "network",
          action: "network-circuit-open",
          severity: "error",
          dedupeKey: "system:network-circuit-open",
          target: { tab: "settings", section: "proxy" },
          message: {
            title: t("networkCircuitTitle", "Network connection issue"),
            summary: t("networkCircuitMessage", NETWORK_CIRCUIT_MESSAGE),
            detail: t("networkCircuitMessage", NETWORK_CIRCUIT_MESSAGE),
          },
          toast: true,
          toastKind: "error",
        },
      );
    };
    window.addEventListener(NETWORK_CIRCUIT_EVENT, onNetworkCircuitOpen);
    return () => {
      window.removeEventListener(NETWORK_CIRCUIT_EVENT, onNetworkCircuitOpen);
    };
  }, [pushToast, t]);

  useEffect(() => {
    ignoredUpdateVersionRef.current = ignoredUpdateVersion;
  }, [ignoredUpdateVersion]);

  useEffect(() => {
    if (!skillsAutoUpdateNotice) return;
    const timer = window.setTimeout(() => {
      setSkillsAutoUpdateNotice(null);
    }, 4000);
    return () => window.clearTimeout(timer);
  }, [skillsAutoUpdateNotice]);

  useEffect(() => {
    const recordRuntimeMessage = (kind: "error" | "unhandledrejection", text: string) => {
      const firstLine = text.split("\n").find(Boolean) || "Unknown runtime error";
      void notifySystemEvent(
        { pushToast, recordMessage },
        {
          source: "system",
          category: "runtime",
          action: kind,
          severity: "error",
          dedupeKey: `system:runtime:${kind}:${firstLine}`,
          target: { tab: "launcher" },
          message: {
            title:
              kind === "unhandledrejection"
                ? t("runtimeUnhandledRejectionTitle", "Unhandled promise rejection")
                : t("runtimeErrorTitle", "Runtime error"),
            summary: firstLine,
            detail: text,
          },
          toast: true,
          toastKind: "error",
        },
      );
    };

    const handleWindowError = (event: ErrorEvent) => {
      const message =
        event.error?.stack || event.message || "Unknown runtime error";
      console.error("window error", event.error || event.message);
      setRuntimeError(message);
      setRuntimeErrorCopied(false);
      recordRuntimeMessage("error", message);
    };

    const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
      const reason = event.reason;
      const message =
        reason instanceof Error
          ? reason.stack || reason.message
          : typeof reason === "string"
            ? reason
            : JSON.stringify(reason, null, 2);
      console.error("unhandled rejection", reason);
      setRuntimeError(message || "Unhandled promise rejection");
      setRuntimeErrorCopied(false);
      recordRuntimeMessage(
        "unhandledrejection",
        message || "Unhandled promise rejection",
      );
    };

    window.addEventListener("error", handleWindowError);
    window.addEventListener("unhandledrejection", handleUnhandledRejection);
    return () => {
      window.removeEventListener("error", handleWindowError);
      window.removeEventListener(
        "unhandledrejection",
        handleUnhandledRejection,
      );
    };
  }, [pushToast, t]);

  const copyRuntimeError = async () => {
    if (!runtimeError) return;
    try {
      await navigator.clipboard.writeText(runtimeError);
      setRuntimeErrorCopied(true);
      window.setTimeout(() => setRuntimeErrorCopied(false), 2000);
    } catch (error) {
      console.error("failed to copy runtime error", error);
    }
  };

  return { copyRuntimeError };
}
