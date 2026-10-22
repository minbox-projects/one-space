import { useTranslation } from "react-i18next";
import {
  HardDrive,
  ShieldCheck,
  Lock,
  Copy,
  Eye,
  EyeOff,
  Sparkles,
  RefreshCw,
  FolderOpen,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { Switch } from "@/components/ui/switch";
import { SettingCard } from "../common/SettingCard";
import { SubTabNavigation, type SubTabItem } from "../common/SubTabNavigation";
import type {
  StorageConfig,
  SyncPolicy,
  LegacySettingsSection,
} from "../types";
import { normalizeSyncPolicyForUi } from "../helpers";

export interface DataSecuritySectionProps {
  subTab: LegacySettingsSection;
  onSubTabChange: (tab: LegacySettingsSection) => void;
  config: StorageConfig;
  setConfig: React.Dispatch<React.SetStateAction<StorageConfig>>;
  masterPassword: string;
  showPass: boolean;
  setShowPass: (show: boolean) => void;
  newPass: string;
  setNewPass: (pass: string) => void;
  confirmNewPass: string;
  setConfirmNewPass: (pass: string) => void;
  showNewPass: boolean;
  setShowNewPass: (show: boolean) => void;
  showConfirmNewPass: boolean;
  setShowConfirmNewPass: (show: boolean) => void;
  changingPass: boolean;
  setChangingPass: (changing: boolean) => void;
  onGenerateMd5Password: () => void;
  onChangeMasterPassword: () => Promise<void>;
  loading?: boolean;
  onShowToast?: (type: "success" | "error", text: string) => void;
  subTabDirtyMap?: Record<string, boolean>;
}

export function DataSecuritySection({
  subTab,
  onSubTabChange,
  config,
  setConfig,
  masterPassword,
  showPass,
  setShowPass,
  newPass,
  setNewPass,
  confirmNewPass,
  setConfirmNewPass,
  showNewPass,
  setShowNewPass,
  showConfirmNewPass,
  setShowConfirmNewPass,
  changingPass,
  setChangingPass,
  onGenerateMd5Password,
  onChangeMasterPassword,
  loading = false,
  onShowToast,
  subTabDirtyMap = {},
}: DataSecuritySectionProps) {
  const { t } = useTranslation();

  const subTabItems: SubTabItem[] = [
    {
      id: "storage",
      titleKey: "dataStorageMenu",
      fallbackTitle: "存储与多端同步",
      icon: HardDrive,
      dirty: subTabDirtyMap["storage"],
    },
    {
      id: "security",
      titleKey: "security",
      fallbackTitle: "主密码与加密",
      icon: ShieldCheck,
      dirty: subTabDirtyMap["security"],
    },
  ];

  const syncScopeConfigurable = config.storage_type !== "local";

  const syncScopeItems: {
    key: keyof SyncPolicy;
    titleKey: string;
    titleFallback: string;
    descKey: string;
    descFallback: string;
  }[] = [
    {
      key: "providers",
      titleKey: "syncScopeProviders",
      titleFallback: "AI Terminal Service Providers",
      descKey: "syncScopeProvidersDesc",
      descFallback:
        "Sync provider profiles, active selections, runtime policy, and profile history. API keys and other sensitive fields remain local.",
    },
    {
      key: "mcp",
      titleKey: "syncScopeMcp",
      titleFallback: "MCP Servers",
      descKey: "syncScopeMcpDesc",
      descFallback:
        "Sync MCP server definitions and provider links. Sensitive env/header values are synced as placeholders, and local model switch state is excluded.",
    },
    {
      key: "content",
      titleKey: "syncScopeContent",
      titleFallback: "Content Data",
      descKey: "syncScopeContentDesc",
      descFallback: "Sync encrypted notes, bookmarks, and snippets only.",
    },
    {
      key: "skills_sources",
      titleKey: "syncScopeSkillsSources",
      titleFallback: "Skills Sources",
      descKey: "syncScopeSkillsSourcesDesc",
      descFallback:
        "Sync shared-profile data for Skills sources. This file is shared with Subagents sources and sync settings.",
    },
    {
      key: "skills_repository",
      titleKey: "syncScopeSkillsRepository",
      titleFallback: "Skills Repository",
      descKey: "syncScopeSkillsRepositoryDesc",
      descFallback:
        "Sync data/skills repository snapshots and metadata, excluding local install records and remote cache.",
    },
    {
      key: "subagents_sources",
      titleKey: "syncScopeSubagentsSources",
      titleFallback: "Subagents Sources",
      descKey: "syncScopeSubagentsSourcesDesc",
      descFallback:
        "Sync shared-profile data for Subagents sources. This file is shared with Skills sources and sync settings.",
    },
    {
      key: "subagents_repository",
      titleKey: "syncScopeSubagentsRepository",
      titleFallback: "Subagents Repository",
      descKey: "syncScopeSubagentsRepositoryDesc",
      descFallback:
        "Sync data/subagents repository snapshots and metadata, excluding local install records and remote cache.",
    },
    {
      key: "ai_news",
      titleKey: "syncScopeAiNews",
      titleFallback: "AI News",
      descKey: "syncScopeAiNewsDesc",
      descFallback:
        "When enabled, OneSpace syncs plaintext AI news records across devices only after an RSS fetch adds new items.",
    },
  ];

  const updateSyncPolicy = (key: keyof SyncPolicy, checked: boolean) => {
    setConfig((prev) => ({
      ...prev,
      sync_policy: {
        ...normalizeSyncPolicyForUi(prev.sync_policy),
        [key]: checked,
      },
    }));
  };

  const handleSelectLocalStoragePath = async () => {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (selected && typeof selected === "string") {
        setConfig((prev) => ({ ...prev, local_storage_path: selected }));
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleSelectICloudPath = async () => {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (selected && typeof selected === "string") {
        setConfig((prev) => ({ ...prev, icloud_storage_path: selected }));
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleSelectSshKey = async () => {
    try {
      const selected = await open({ multiple: false });
      if (selected && typeof selected === "string") {
        setConfig((prev) => ({ ...prev, ssh_key_path: selected }));
      }
    } catch (e) {
      console.error(e);
    }
  };

  return (
    <div className="space-y-6">
      <SubTabNavigation
        items={subTabItems}
        activeSubTab={subTab}
        onChange={onSubTabChange}
      />

      {/* 1. Storage Backend and Sync Policy */}
      {subTab === "storage" && (
        <SettingCard
          id="section-storage"
          icon={HardDrive}
          title={t("dataStorageMenu", "Data Storage")}
          description={t(
            "dataStorageDesc",
            "Configure where OneSpace data is saved and synced.",
          )}
        >
          {/* Storage Type */}
          <div className="space-y-2">
            <label className="text-sm font-medium text-muted-foreground">
              {t("storageType", "Storage Type")}
            </label>
            <div className="grid grid-cols-3 gap-2 p-1 bg-muted rounded-xl border">
              <button
                type="button"
                onClick={() =>
                  setConfig((prev) => ({ ...prev, storage_type: "local" }))
                }
                className={`py-2 px-4 rounded-lg text-sm font-medium transition-all ${
                  config.storage_type === "local"
                    ? "bg-background shadow-sm text-foreground"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {t("local", "Local")}
              </button>
              <button
                type="button"
                onClick={() =>
                  setConfig((prev) => ({ ...prev, storage_type: "icloud" }))
                }
                className={`py-2 px-4 rounded-lg text-sm font-medium transition-all ${
                  config.storage_type === "icloud"
                    ? "bg-background shadow-sm text-foreground"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {t("icloud", "iCloud Drive")}
              </button>
              <button
                type="button"
                onClick={() =>
                  setConfig((prev) => ({ ...prev, storage_type: "git" }))
                }
                className={`py-2 px-4 rounded-lg text-sm font-medium transition-all ${
                  config.storage_type === "git"
                    ? "bg-background shadow-sm text-foreground"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {t("gitRepo", "Git Repository")}
              </button>
            </div>
          </div>

          <hr className="border-border/50" />

          {/* Sync Data Scope */}
          <div className="space-y-3">
            <div className="space-y-1">
              <h3 className="text-sm font-medium">
                {t("syncScopeTitle", "Sync Data Scope")}
              </h3>
              <p className="text-xs text-muted-foreground">
                {t(
                  "syncScopeDesc",
                  "Choose which data domains should be synchronized across storage backends.",
                )}
              </p>
            </div>

            {syncScopeConfigurable ? (
              <div className="space-y-3">
                {syncScopeItems.map((item) => (
                  <div
                    key={item.key}
                    className="flex items-center justify-between gap-4 rounded-xl border bg-muted/20 px-4 py-3"
                  >
                    <div className="space-y-0.5">
                      <h4 className="text-sm font-medium">
                        {t(item.titleKey, item.titleFallback)}
                      </h4>
                      <p className="text-xs text-muted-foreground">
                        {t(item.descKey, item.descFallback)}
                      </p>
                    </div>
                    <Switch
                      checked={
                        !!normalizeSyncPolicyForUi(config.sync_policy)[item.key]
                      }
                      onCheckedChange={(checked) =>
                        updateSyncPolicy(item.key, checked)
                      }
                    />
                  </div>
                ))}
              </div>
            ) : (
              <div className="rounded-xl border border-dashed bg-muted/20 px-4 py-3 text-xs text-muted-foreground">
                {t(
                  "syncScopeLocalHint",
                  "Local mode stores data directly on this device. Sync scope is available when using iCloud or Git storage.",
                )}
              </div>
            )}
          </div>

          {/* iCloud storage path */}
          {config.storage_type === "icloud" && (
            <div className="space-y-4 pt-4 border-t animate-in fade-in zoom-in-95">
              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t("icloudStoragePath", "iCloud Storage Path")}
                </label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    placeholder={t(
                      "icloudStoragePathPlaceholder",
                      "~/Library/Mobile Documents/com~apple~CloudDocs/onespace",
                    )}
                    className="flex-1 bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all font-mono"
                    value={config.icloud_storage_path || ""}
                    onChange={(e) =>
                      setConfig((prev) => ({
                        ...prev,
                        icloud_storage_path: e.target.value,
                      }))
                    }
                  />
                  <button
                    type="button"
                    onClick={handleSelectICloudPath}
                    className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95"
                  >
                    <FolderOpen className="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* Local storage path */}
          {config.storage_type === "local" && (
            <div className="space-y-4 pt-4 border-t animate-in fade-in zoom-in-95">
              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t("localStoragePath", "Local Storage Path")}
                </label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    placeholder={t(
                      "localStoragePathPlaceholder",
                      "~/.config/onespace/data",
                    )}
                    className="flex-1 bg-background border rounded-xl px-4 py-2.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all font-mono"
                    value={config.local_storage_path || ""}
                    onChange={(e) =>
                      setConfig((prev) => ({
                        ...prev,
                        local_storage_path: e.target.value,
                      }))
                    }
                  />
                  <button
                    type="button"
                    onClick={handleSelectLocalStoragePath}
                    className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95"
                  >
                    <FolderOpen className="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* Git storage configuration */}
          {config.storage_type === "git" && (
            <div className="space-y-4 pt-4 border-t animate-in fade-in zoom-in-95">
              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t("gitUrl", "Git Repository URL")}
                </label>
                <input
                  type="text"
                  placeholder="https://github.com/user/repo.git or git@github.com:user/repo.git"
                  className="w-full bg-background border rounded-xl px-4 py-2.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                  value={config.git_url || ""}
                  onChange={(e) =>
                    setConfig((prev) => ({ ...prev, git_url: e.target.value }))
                  }
                />
              </div>

              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t("authMethod", "Authentication Method")}
                </label>
                <div className="grid grid-cols-2 gap-2 p-1 bg-muted rounded-xl border">
                  <button
                    type="button"
                    onClick={() =>
                      setConfig((prev) => ({ ...prev, auth_method: "http" }))
                    }
                    className={`py-2 px-4 rounded-lg text-sm font-medium transition-all ${
                      (config.auth_method || "http") === "http"
                        ? "bg-background shadow-sm text-foreground"
                        : "text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    HTTP (Token)
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      setConfig((prev) => ({ ...prev, auth_method: "ssh" }))
                    }
                    className={`py-2 px-4 rounded-lg text-sm font-medium transition-all ${
                      config.auth_method === "ssh"
                        ? "bg-background shadow-sm text-foreground"
                        : "text-muted-foreground hover:text-foreground"
                    }`}
                  >
                    SSH Key
                  </button>
                </div>
              </div>

              {(config.auth_method || "http") === "http" ? (
                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  <div className="space-y-1">
                    <label className="text-xs text-muted-foreground">
                      {t("httpUsername", "Username")}
                    </label>
                    <input
                      type="text"
                      className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                      value={config.http_username || ""}
                      onChange={(e) =>
                        setConfig((prev) => ({
                          ...prev,
                          http_username: e.target.value,
                        }))
                      }
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs text-muted-foreground">
                      {t("httpToken", "Access Token")}
                    </label>
                    <input
                      type="password"
                      className="w-full bg-background border rounded-xl px-3.5 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/20"
                      value={config.http_token || ""}
                      onChange={(e) =>
                        setConfig((prev) => ({
                          ...prev,
                          http_token: e.target.value,
                        }))
                      }
                    />
                  </div>
                </div>
              ) : (
                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground">
                    {t("sshKeyPath", "SSH Private Key Path")}
                  </label>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      placeholder="~/.ssh/id_rsa"
                      className="flex-1 bg-background border rounded-xl px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
                      value={config.ssh_key_path || ""}
                      onChange={(e) =>
                        setConfig((prev) => ({
                          ...prev,
                          ssh_key_path: e.target.value,
                        }))
                      }
                    />
                    <button
                      type="button"
                      onClick={handleSelectSshKey}
                      className="px-4 py-2 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80"
                    >
                      <FolderOpen className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}
        </SettingCard>
      )}

      {/* 2. Security & Master Password */}
      {subTab === "security" && (
        <SettingCard
          id="section-security"
          icon={ShieldCheck}
          title={t("security", "Data Security")}
          description={t(
            "securityDesc",
            "Manage your master password used for encrypting sensitive data.",
          )}
        >
          {/* Master Password Box */}
          <div className="bg-muted/30 p-5 rounded-2xl border border-dashed space-y-4">
            <div className="flex items-center justify-between">
              <label className="text-sm font-medium text-muted-foreground">
                {t("currentMasterPassword", "Current Master Password")}
              </label>
              <ShieldCheck className="w-5 h-5 text-primary opacity-50" />
            </div>

            <div className="relative">
              <Lock className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
              <input
                type={showPass ? "text" : "password"}
                readOnly
                className="w-full bg-background border rounded-xl pl-10 pr-20 py-3 text-sm font-mono tracking-widest cursor-default"
                value={masterPassword}
              />
              <button
                type="button"
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(masterPassword);
                    onShowToast?.("success", t("copiedToClipboard", "Copied to clipboard"));
                  } catch (e: any) {
                    onShowToast?.("error", e?.toString() || "Copy failed");
                  }
                }}
                title={t("copyToClipboard", "Copy to clipboard")}
                className="absolute right-10 top-3 text-muted-foreground hover:text-foreground transition-colors"
              >
                <Copy className="w-4 h-4" />
              </button>
              <button
                type="button"
                onClick={() => setShowPass(!showPass)}
                className="absolute right-3.5 top-3 text-muted-foreground hover:text-foreground transition-colors"
              >
                {showPass ? (
                  <EyeOff className="w-4 h-4" />
                ) : (
                  <Eye className="w-4 h-4" />
                )}
              </button>
            </div>
            <p className="text-[10px] text-muted-foreground leading-relaxed">
              {t(
                "defaultPassNotice",
                "Note: This key is used for encrypting Git credentials and other sensitive data locally.",
              )}
            </p>
          </div>

          {!changingPass ? (
            <button
              type="button"
              onClick={() => {
                setChangingPass(true);
                setShowNewPass(true);
                setShowConfirmNewPass(true);
              }}
              className="w-full py-3 border border-primary/20 bg-primary/5 text-primary rounded-xl text-sm font-semibold hover:bg-primary/10 transition-all"
            >
              {t("changeMasterPassword", "Change Master Password")}
            </button>
          ) : (
            <div className="space-y-4 pt-4 border-t animate-in fade-in slide-in-from-top-2">
              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={onGenerateMd5Password}
                  disabled={loading}
                  className="inline-flex items-center gap-2 px-3 py-1.5 border rounded-lg text-xs font-medium hover:bg-muted transition-colors disabled:opacity-50"
                >
                  <Sparkles className="w-3.5 h-3.5" />
                  <span>{t("generateMd5Password", "Generate MD5 Password")}</span>
                </button>
              </div>

              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t("newPassword", "New Password")}
                </label>
                <div className="relative">
                  <Lock className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                  <input
                    type={showNewPass ? "text" : "password"}
                    className="w-full bg-background border rounded-xl pl-10 pr-12 py-3 text-sm font-mono tracking-widest focus:outline-none focus:ring-2 focus:ring-primary/20"
                    value={newPass}
                    onChange={(e) => setNewPass(e.target.value)}
                  />
                  <button
                    type="button"
                    onClick={() => setShowNewPass(!showNewPass)}
                    className="absolute right-3.5 top-3 text-muted-foreground hover:text-foreground transition-colors"
                  >
                    {showNewPass ? (
                      <EyeOff className="w-4 h-4" />
                    ) : (
                      <Eye className="w-4 h-4" />
                    )}
                  </button>
                </div>
              </div>

              <div className="space-y-2">
                <label className="text-sm font-medium text-muted-foreground">
                  {t("confirmPassword", "Confirm Password")}
                </label>
                <div className="relative">
                  <Lock className="absolute left-3.5 top-3 w-4 h-4 text-muted-foreground" />
                  <input
                    type={showConfirmNewPass ? "text" : "password"}
                    className="w-full bg-background border rounded-xl pl-10 pr-12 py-3 text-sm font-mono tracking-widest focus:outline-none focus:ring-2 focus:ring-primary/20"
                    value={confirmNewPass}
                    onChange={(e) => setConfirmNewPass(e.target.value)}
                  />
                  <button
                    type="button"
                    onClick={() => setShowConfirmNewPass(!showConfirmNewPass)}
                    className="absolute right-3.5 top-3 text-muted-foreground hover:text-foreground transition-colors"
                  >
                    {showConfirmNewPass ? (
                      <EyeOff className="w-4 h-4" />
                    ) : (
                      <Eye className="w-4 h-4" />
                    )}
                  </button>
                </div>
              </div>

              <div className="flex gap-2 pt-2">
                <button
                  type="button"
                  onClick={onChangeMasterPassword}
                  disabled={
                    !newPass ||
                    !confirmNewPass ||
                    newPass !== confirmNewPass ||
                    loading ||
                    !masterPassword
                  }
                  className="flex-1 bg-primary text-primary-foreground py-2.5 rounded-xl text-sm font-semibold hover:bg-primary/90 disabled:opacity-50"
                >
                  {loading ? (
                    <RefreshCw className="w-4 h-4 animate-spin mx-auto" />
                  ) : (
                    t("confirmChange", "Update Password")
                  )}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setChangingPass(false);
                    setNewPass("");
                    setConfirmNewPass("");
                    setShowNewPass(true);
                    setShowConfirmNewPass(true);
                  }}
                  className="px-6 py-2.5 border rounded-xl text-sm font-medium hover:bg-muted transition-all"
                >
                  {t("cancel", "Cancel")}
                </button>
              </div>
            </div>
          )}
        </SettingCard>
      )}
    </div>
  );
}
