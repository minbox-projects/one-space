import type { Dispatch, SetStateAction } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";
import { errorToMessage } from "@/lib/messages";
import type { ActionContext } from "@/lib/userActions";
import { protocolRouterRotateToken } from "@/lib/protocolRouter";
import type {
  ProxyConfig,
  ProxyStatus,
  ProtocolRouterConfig,
  SkillsSyncState,
  SubagentSourceDiagnoseResult,
} from "../types";
import { generateRandomMd5String } from "../helpers";
import type { SettingsMessage } from "./types";

export interface SettingsActionsContext {
  t: TFunction;
  pushToast: ActionContext["pushToast"];
  proxyConfig: ProxyConfig;
  setProxyStatus: Dispatch<SetStateAction<ProxyStatus | null>>;
  setTestingProxy: Dispatch<SetStateAction<boolean>>;
  setProtocolRouterBusy: Dispatch<SetStateAction<boolean>>;
  setProtocolRouterConfig: Dispatch<SetStateAction<ProtocolRouterConfig>>;
  setNewPass: Dispatch<SetStateAction<string>>;
  setConfirmNewPass: Dispatch<SetStateAction<string>>;
  setShowNewPass: Dispatch<SetStateAction<boolean>>;
  setShowConfirmNewPass: Dispatch<SetStateAction<boolean>>;
  setMessage: Dispatch<SetStateAction<SettingsMessage>>;
  masterPassword: string;
  newPass: string;
  confirmNewPass: string;
  setLoading: Dispatch<SetStateAction<boolean>>;
  setChangingPass: Dispatch<SetStateAction<boolean>>;
  setMasterPassword: Dispatch<SetStateAction<string>>;
  setSkillsSyncNowLoading: Dispatch<SetStateAction<boolean>>;
  setSkillsSyncState: Dispatch<SetStateAction<SkillsSyncState | null>>;
  setSubagentsSyncNowLoading: Dispatch<SetStateAction<boolean>>;
  setSubagentsSyncState: Dispatch<SetStateAction<SkillsSyncState | null>>;
  setSubagentSourceDiagnosing: Dispatch<
    SetStateAction<Record<string, boolean>>
  >;
  setSubagentSourceDiagnostics: Dispatch<
    SetStateAction<Record<string, SubagentSourceDiagnoseResult>>
  >;
}

export function createSettingsActions({
  t,
  pushToast,
  proxyConfig,
  setProxyStatus,
  setTestingProxy,
  setProtocolRouterBusy,
  setProtocolRouterConfig,
  setNewPass,
  setConfirmNewPass,
  setShowNewPass,
  setShowConfirmNewPass,
  setMessage,
  masterPassword,
  newPass,
  confirmNewPass,
  setLoading,
  setChangingPass,
  setMasterPassword,
  setSkillsSyncNowLoading,
  setSkillsSyncState,
  setSubagentsSyncNowLoading,
  setSubagentsSyncState,
  setSubagentSourceDiagnosing,
  setSubagentSourceDiagnostics,
}: SettingsActionsContext) {
  const handleTestProxy = async () => {
    setTestingProxy(true);
    setProxyStatus(null);
    try {
      const status = await invoke<ProxyStatus>("test_proxy_connection", {
        config: proxyConfig,
      });
      setProxyStatus(status);
    } catch (err: any) {
      setProxyStatus({
        is_available: false,
        latency_ms: 0,
        message: err.toString(),
        proxy_type: proxyConfig.proxy_type,
        proxy_host: proxyConfig.proxy_host,
      });
    } finally {
      setTestingProxy(false);
    }
  };

  const rotateProtocolToken = async () => {
    setProtocolRouterBusy(true);
    try {
      const nextConfig = await protocolRouterRotateToken();
      setProtocolRouterConfig((prev) => ({ ...prev, token: nextConfig.token }));
      pushToast({
        title: t("tokenRotatedTitle", "Token updated"),
        description: t(
          "tokenRotatedDesc",
          "Generated new router token. Save to apply.",
        ),
        kind: "success",
      });
    } catch (e: any) {
      pushToast({
        title: t("tokenRotateFailedTitle", "Token rotate failed"),
        description: errorToMessage(e),
        kind: "error",
      });
    } finally {
      setProtocolRouterBusy(false);
    }
  };

  const handleGenerateMd5Password = () => {
    const generated = generateRandomMd5String();
    setNewPass(generated);
    setConfirmNewPass(generated);
    setShowNewPass(true);
    setShowConfirmNewPass(true);
    setMessage({
      type: "success",
      text: t(
        "md5PasswordGenerated",
        "Generated and filled into both password fields.",
      ),
    });
    setTimeout(() => setMessage({ type: "", text: "" }), 2000);
  };

  const handleChangeMasterPassword = async () => {
    if (!newPass || newPass !== confirmNewPass) return;
    setLoading(true);
    try {
      await invoke("change_master_password", {
        oldPassword: masterPassword,
        newPassword: newPass,
      });
      setMasterPassword(newPass);
      setChangingPass(false);
      setNewPass("");
      setConfirmNewPass("");
      setMessage({
        type: "success",
        text: t("masterPasswordUpdated", "Master password updated successfully."),
      });
      setTimeout(() => setMessage({ type: "", text: "" }), 3000);
    } catch (e: any) {
      setMessage({ type: "error", text: e.toString() });
    } finally {
      setLoading(false);
    }
  };

  const handleSkillsSyncNow = async () => {
    setSkillsSyncNowLoading(true);
    try {
      await invoke("skills_sync_now");
      const resp = await invoke<{ ok: boolean; data: SkillsSyncState }>(
        "skills_sync_status_get",
      );
      setSkillsSyncState(resp?.data || null);
      setMessage({ type: "success", text: t("syncSuccess", "Sync successful") });
      setTimeout(() => setMessage({ type: "", text: "" }), 3000);
    } catch (e: any) {
      setMessage({ type: "error", text: e.toString() });
    } finally {
      setSkillsSyncNowLoading(false);
    }
  };

  const handleSubagentsSyncNow = async () => {
    setSubagentsSyncNowLoading(true);
    try {
      await invoke("subagents_sync_now");
      const resp = await invoke<{ ok: boolean; data: SkillsSyncState }>(
        "subagents_sync_status_get",
      );
      setSubagentsSyncState(resp?.data || null);
      setMessage({ type: "success", text: t("syncSuccess", "Sync successful") });
      setTimeout(() => setMessage({ type: "", text: "" }), 3000);
    } catch (e: any) {
      setMessage({ type: "error", text: e.toString() });
    } finally {
      setSubagentsSyncNowLoading(false);
    }
  };

  const handleDiagnoseSubagentSource = async (sourceId: string) => {
    if (!sourceId) return;
    setSubagentSourceDiagnosing((prev) => ({ ...prev, [sourceId]: true }));
    try {
      const resp = await invoke<{ ok: boolean; data: SubagentSourceDiagnoseResult }>(
        "subagents_source_diagnose",
        { sourceId },
      );
      if (resp?.data) {
        setSubagentSourceDiagnostics((prev) => ({
          ...prev,
          [sourceId]: resp.data,
        }));
      }
    } catch (e: any) {
      pushToast({
        title: "Diagnose failed",
        description: e.toString(),
        kind: "error",
      });
    } finally {
      setSubagentSourceDiagnosing((prev) => ({ ...prev, [sourceId]: false }));
    }
  };

  return {
    handleTestProxy,
    rotateProtocolToken,
    handleGenerateMd5Password,
    handleChangeMasterPassword,
    handleSkillsSyncNow,
    handleSubagentsSyncNow,
    handleDiagnoseSubagentSource,
  };
}
