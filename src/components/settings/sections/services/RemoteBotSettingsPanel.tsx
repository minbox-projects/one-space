import { useState, useEffect, useMemo } from "react";
import type { TFunction } from "i18next";
import {
  CheckCircle2,
  XCircle,
  RefreshCw,
  KeyRound,
  ShieldAlert,
  Plus,
  Trash2,
  FolderOpen,
  Terminal,
  Check,
  Search,
  Sparkles,
  Layers,
  AlertCircle,
  Eye,
  EyeOff,
  Star,
  User,
  ShieldCheck,
  Globe,
  Bot,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { TelegramIcon } from "@/components/common/TelegramIcon";
import { refreshRuntimeStatus } from "@/lib/runtimeStatus";
import { SettingCard } from "../../common/SettingCard";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import type { ApiResp, WorkspaceView } from "@/components/Workspaces/types";
import { normalizeWorkspaceView } from "@/components/Workspaces/helpers/workspaceHelpers";
import {
  type RemoteBotConfig,
  type RemoteBotStatus,
  type OpenCodeInstalledVersion,
  remoteBotGetConfig,
  remoteBotSaveConfig,
  remoteBotGetStatus,
  remoteBotTestConnection,
  remoteBotRestart,
  remoteBotProbeOpencode,
  remoteBotListInstalledOpencode,
} from "@/lib/remoteBot";

interface RemoteBotSettingsPanelProps {
  t: TFunction;
  onShowToast?: (type: "success" | "error", text: string) => void;
}

export function RemoteBotSettingsPanel({ t, onShowToast }: RemoteBotSettingsPanelProps) {
  const [config, setConfig] = useState<RemoteBotConfig>({
    enabled: false,
    bot_token: "",
    allowed_chat_ids: [],
    active_workspace: null,
    auto_approve_permissions: true,
    enable_natural_dialogue: true,
    opencode_bin_path: null,
    workspace_directories: [],
    updated_at: Date.now(),
  });
  const [status, setStatus] = useState<RemoteBotStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [testing, setTesting] = useState(false);
  const [newChatId, setNewChatId] = useState("");
  const [showToken, setShowToken] = useState(false);

  // OpenCode CLI 版本与检测状态
  const [installedVersions, setInstalledVersions] = useState<OpenCodeInstalledVersion[]>([]);
  const [scanningVersions, setScanningVersions] = useState(false);
  const [probingCli, setProbingCli] = useState(false);
  const [cliProbeResult, setCliProbeResult] = useState<{
    path: string;
    success: boolean;
    output: string;
  } | null>(null);

  // 工作空间管理状态
  const [systemWorkspaces, setSystemWorkspaces] = useState<WorkspaceView[]>([]);
  const [isAddWorkspaceOpen, setIsAddWorkspaceOpen] = useState(false);
  const [wsSearchQuery, setWsSearchQuery] = useState("");
  const [newWorkspaceDir, setNewWorkspaceDir] = useState("");

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    try {
      setLoading(true);
      const [cfg, st, versions] = await Promise.all([
        remoteBotGetConfig(),
        remoteBotGetStatus(),
        remoteBotListInstalledOpencode().catch(() => [] as OpenCodeInstalledVersion[]),
      ]);
      setConfig({
        ...cfg,
        workspace_directories: cfg.workspace_directories || [],
        enable_natural_dialogue: cfg.enable_natural_dialogue ?? true,
      });
      setStatus(st);
      setInstalledVersions(versions);

      // 如果当前已配置路径但不在扫描列表中，自动尝试探针并追加为自定义项
      if (cfg.opencode_bin_path && !versions.some((v) => v.path === cfg.opencode_bin_path)) {
        try {
          const probeText = await remoteBotProbeOpencode(cfg.opencode_bin_path);
          const verMatch = probeText.match(/v?(\d+\.\d+\.\d+)/i);
          const ver = verMatch ? verMatch[1] : probeText.split("\n")[0] || "custom";
          const major = ver.startsWith("2") ? 2 : 1;
          const verType = major >= 2 ? "v2" : "v1";
          setInstalledVersions((prev) => [
            {
              path: cfg.opencode_bin_path!,
              version: ver,
              major_version: major,
              version_type: verType,
              is_active: true,
            },
            ...prev,
          ]);
        } catch {
          // 忽略探测失败
        }
      }

      // 加载 OneSpace 系统工作空间列表
      try {
        const resp = await invoke<ApiResp<unknown[]>>("workspaces_list");
        if (resp && Array.isArray(resp.data)) {
          const sorted = resp.data
            .map((item) => normalizeWorkspaceView(item))
            .sort(
              (a, b) => (b.workspace.last_activity_at || 0) - (a.workspace.last_activity_at || 0)
            );
          setSystemWorkspaces(sorted);
        }
      } catch (wsErr) {
        console.warn("加载 OneSpace 系统工作空间失败:", wsErr);
      }
    } catch (e: any) {
      onShowToast?.("error", `加载配置失败: ${e?.message || e}`);
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async (updated: RemoteBotConfig) => {
    if (updated.enabled && !updated.opencode_bin_path?.trim()) {
      onShowToast?.("error", "启用 Telegram 远程控制前，必须选择一个具体的 OpenCode 版本");
      return;
    }
    try {
      setConfig(updated);
      await remoteBotSaveConfig(updated);
      onShowToast?.("success", t("remoteBotSaved", "设置已保存"));
      const newStatus = await remoteBotGetStatus();
      setStatus(newStatus);
      void refreshRuntimeStatus("remote-bot");
    } catch (e: any) {
      onShowToast?.("error", `保存失败: ${e?.message || e}`);
    }
  };

  const handleTestToken = async () => {
    if (!config.bot_token.trim()) {
      onShowToast?.("error", "请先输入 Telegram Bot Token");
      return;
    }
    try {
      setTesting(true);
      const user = await remoteBotTestConnection(config.bot_token.trim());
      onShowToast?.("success", `连接成功！机器人: @${user.username || user.first_name}`);
    } catch (e: any) {
      onShowToast?.("error", `测试连接失败: ${e?.message || e}`);
    } finally {
      setTesting(false);
    }
  };

  const handleRestart = async () => {
    try {
      setLoading(true);
      const newStatus = await remoteBotRestart();
      setStatus(newStatus);
      void refreshRuntimeStatus("remote-bot");
      onShowToast?.("success", "已重启 Telegram 轮询服务");
    } catch (e: any) {
      onShowToast?.("error", `重启失败: ${e?.message || e}`);
    } finally {
      setLoading(false);
    }
  };

  const handleAddChatId = () => {
    const parsed = parseInt(newChatId.trim(), 10);
    if (isNaN(parsed)) {
      onShowToast?.("error", "请输入合法的纯数字 Chat ID");
      return;
    }
    if (config.allowed_chat_ids.includes(parsed)) {
      onShowToast?.("error", "该 Chat ID 已在白名单中");
      return;
    }
    const updated = {
      ...config,
      allowed_chat_ids: [...config.allowed_chat_ids, parsed],
    };
    setNewChatId("");
    handleSave(updated);
  };

  const handleRemoveChatId = (id: number) => {
    const updated = {
      ...config,
      allowed_chat_ids: config.allowed_chat_ids.filter((c) => c !== id),
    };
    handleSave(updated);
  };

  // OpenCode CLI 路径探测与选择
  const handleScanVersions = async () => {
    try {
      setScanningVersions(true);
      const list = await remoteBotListInstalledOpencode();
      setInstalledVersions(list);
      if (list.length === 0) {
        onShowToast?.("error", "未在本机检测到已安装的 OpenCode，请通过终端安装或手动指定路径");
      } else {
        onShowToast?.("success", `已扫描到 ${list.length} 个本机 OpenCode 版本`);
      }
    } catch (err: any) {
      onShowToast?.("error", `扫描版本失败: ${err?.message || err}`);
    } finally {
      setScanningVersions(false);
    }
  };

  const handleSelectVersion = async (item: OpenCodeInstalledVersion) => {
    const updated: RemoteBotConfig = {
      ...config,
      opencode_bin_path: item.path,
      opencode_version: item.version,
      opencode_version_type: item.version_type,
    };
    setConfig(updated);
    await handleSave(updated);
  };

  const handleBrowseCli = async () => {
    try {
      const selected = await openDialog({
        directory: false,
        multiple: false,
        title: "选择 OpenCode CLI 执行文件",
      });
      if (selected && typeof selected === "string") {
        try {
          const probeRes = await remoteBotProbeOpencode(selected);
          const verMatch = probeRes.match(/v?(\d+\.\d+\.\d+)/i);
          const ver = verMatch ? verMatch[1] : probeRes.split("\n")[0] || "custom";
          const major = ver.startsWith("2") ? 2 : 1;
          const verType = major >= 2 ? "v2" : "v1";

          const newItem: OpenCodeInstalledVersion = {
            path: selected,
            version: ver,
            major_version: major,
            version_type: verType,
            is_active: true,
          };

          setInstalledVersions((prev) => {
            const rest = prev.filter((p) => p.path !== selected);
            return [newItem, ...rest];
          });

          const updated: RemoteBotConfig = {
            ...config,
            opencode_bin_path: selected,
            opencode_version: ver,
            opencode_version_type: verType,
          };
          setConfig(updated);
          await handleSave(updated);
          onShowToast?.("success", `已选择并验证 OpenCode (${verType} ${ver})`);
        } catch (probeErr: any) {
          onShowToast?.("error", `该文件不是有效的 OpenCode 可执行程序: ${probeErr?.message || probeErr}`);
        }
      }
    } catch (e: any) {
      onShowToast?.("error", `选择文件失败: ${e?.message || e}`);
    }
  };

  const handleProbeCli = async (path: string) => {
    try {
      setProbingCli(true);
      setCliProbeResult(null);
      const res = await remoteBotProbeOpencode(path);
      setCliProbeResult({ path, success: true, output: res.trim() });
      onShowToast?.("success", `探测成功: ${res.split("\n")[0] || res}`);
    } catch (e: any) {
      const errStr = e?.message || String(e);
      setCliProbeResult({ path, success: false, output: errStr });
      onShowToast?.("error", `CLI 探测失败: ${errStr}`);
    } finally {
      setProbingCli(false);
    }
  };

  // 工作区项目管理
  const handleBrowseWorkspace = async () => {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: "选择项目工作区根目录",
      });
      if (selected && typeof selected === "string") {
        const currentList = config.workspace_directories || [];
        if (!currentList.includes(selected)) {
          const updated = {
            ...config,
            workspace_directories: [...currentList, selected],
            active_workspace: config.active_workspace || selected,
          };
          await handleSave(updated);
          setNewWorkspaceDir("");
        } else {
          onShowToast?.("error", "该工作区目录已在常用列表中");
        }
      }
    } catch (e: any) {
      onShowToast?.("error", `选择目录失败: ${e?.message || e}`);
    }
  };

  const handleAddWorkspaceDir = () => {
    const trimmed = newWorkspaceDir.trim();
    if (!trimmed) {
      onShowToast?.("error", "请输入有效的工作区绝对路径");
      return;
    }
    const currentList = config.workspace_directories || [];
    if (currentList.includes(trimmed)) {
      onShowToast?.("error", "该工作区目录已在常用列表中");
      return;
    }
    const updated = {
      ...config,
      workspace_directories: [...currentList, trimmed],
      active_workspace: config.active_workspace || trimmed,
    };
    setNewWorkspaceDir("");
    handleSave(updated);
  };

  const handleRemoveWorkspaceDir = (dir: string) => {
    const currentList = config.workspace_directories || [];
    const updatedList = currentList.filter((d) => d !== dir);
    const updated = {
      ...config,
      workspace_directories: updatedList,
      active_workspace: config.active_workspace === dir ? (updatedList[0] || null) : config.active_workspace,
    };
    handleSave(updated);
  };

  const handleSetActiveWorkspace = (dir: string) => {
    const updated = {
      ...config,
      active_workspace: dir,
    };
    handleSave(updated);
  };

  // 一键选入最近活跃 Top 5
  const handleImportTop5Recent = () => {
    if (systemWorkspaces.length === 0) {
      onShowToast?.("error", "暂无可用的系统工作空间");
      return;
    }
    const top5Paths = systemWorkspaces.slice(0, 5).map((w) => w.workspace.root_path);
    const currentSet = new Set(config.workspace_directories || []);
    for (const p of top5Paths) {
      currentSet.add(p);
    }
    const updatedList = Array.from(currentSet);
    const updated = {
      ...config,
      workspace_directories: updatedList,
      active_workspace: config.active_workspace || updatedList[0] || null,
    };
    handleSave(updated);
    onShowToast?.("success", `已将最近活跃的 ${Math.min(5, top5Paths.length)} 个项目同步至精选列表`);
  };

  // 过滤系统工作空间列表
  const filteredSystemWorkspaces = useMemo(() => {
    const query = wsSearchQuery.trim().toLowerCase();
    if (!query) {
      return systemWorkspaces;
    }
    return systemWorkspaces.filter(
      (w) =>
        w.workspace.name.toLowerCase().includes(query) ||
        w.workspace.root_path.toLowerCase().includes(query) ||
        (w.workspace.tags && w.workspace.tags.some((t) => t.toLowerCase().includes(query)))
    );
  }, [systemWorkspaces, wsSearchQuery]);

  return (
    <div className="space-y-6">
      {/* 1. 服务状态与连接凭据 */}
      <SettingCard
        id="section-remote-bot"
        icon={TelegramIcon}
        title={t("remoteBotSettings", "Telegram 远程工作台")}
        description={t(
          "remoteBotSettingsDesc",
          "通过 Telegram 手机客户端远程唤起 Planning 需求规划、Ask Question 交互决策以及 OpenCode CLI 实施与测试。"
        )}
        headerAction={
          <div className="flex items-center gap-3">
            <Tooltip content="重启 Telegram 轮询服务">
              <Button
                variant="ghost"
                size="icon"
                disabled={loading}
                onClick={handleRestart}
                className="h-8 w-8 text-muted-foreground hover:text-foreground"
              >
                <RefreshCw className={`w-4 h-4 ${loading ? "animate-spin" : ""}`} />
              </Button>
            </Tooltip>
            <div className="h-4 w-px bg-border/60" />
            <Switch
              checked={config.enabled}
              onCheckedChange={(checked) => {
                if (checked && !config.opencode_bin_path?.trim()) {
                  onShowToast?.("error", "必须选择一个具体的 OpenCode 版本后才能启用 Telegram 远程工作台");
                  return;
                }
                handleSave({ ...config, enabled: checked });
              }}
            />
          </div>
        }
      >
        {/* 一体式紧凑状态看板 */}
        <div className="flex flex-wrap items-center justify-between gap-3 p-3.5 rounded-xl border bg-muted/20">
          <div className="flex flex-wrap items-center gap-4">
            {/* 状态徽章 */}
            <div className="flex items-center gap-2">
              {status?.running ? (
                <Badge variant="success" className="gap-1.5 py-1 px-2.5">
                  <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
                  已在线轮询
                </Badge>
              ) : (
                <Badge variant="secondary" className="gap-1.5 py-1 px-2.5 text-muted-foreground">
                  <span className="w-2 h-2 rounded-full bg-muted-foreground/50" />
                  已停止
                </Badge>
              )}
            </div>

            {/* 机器人账号 */}
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Bot className="w-3.5 h-3.5" />
              <span>机器人:</span>
              <span className="font-semibold text-foreground font-mono">
                {status?.bot_username ? `@${status.bot_username}` : "未连接"}
              </span>
            </div>

            {/* 管理员数量 */}
            <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <User className="w-3.5 h-3.5" />
              <span>已授权管理员:</span>
              <span className="font-semibold text-foreground">
                {config.allowed_chat_ids.length > 0 ? `${config.allowed_chat_ids.length} 人` : "未绑定"}
              </span>
            </div>
          </div>

          {/* 统一网络代理联动提示 */}
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground bg-background/60 px-2.5 py-1 rounded-lg border border-border/40">
            <Globe className="w-3.5 h-3.5 text-primary" />
            <span>网络代理已联动</span>
          </div>
        </div>

        {/* 异常警示 */}
        {status?.last_error && (
          <div className="p-3 rounded-xl bg-destructive/10 border border-destructive/20 text-destructive text-xs flex items-center gap-2">
            <ShieldAlert className="w-4 h-4 flex-shrink-0" />
            <span className="truncate">最后异常: {status.last_error}</span>
          </div>
        )}

        {/* Token 输入区域 */}
        <div className="space-y-2 pt-1">
          <div className="flex items-center justify-between">
            <label className="text-sm font-semibold text-foreground flex items-center gap-1.5">
              <KeyRound className="w-4 h-4 text-primary" />
              Telegram Bot Token
            </label>
            <span className="text-xs text-muted-foreground">
              在 Telegram 私聊 <code className="text-primary font-mono">@BotFather</code> 发送 <code className="text-primary font-mono">/newbot</code> 申请
            </span>
          </div>

          <div className="flex gap-2">
            <div className="relative flex-1">
              <input
                type={showToken ? "text" : "password"}
                value={config.bot_token}
                onChange={(e) => setConfig({ ...config, bot_token: e.target.value })}
                placeholder="例如: 123456789:ABCdefGHIjklMNOpqrsTUVwxyz"
                className="w-full rounded-xl border bg-background pl-3.5 pr-10 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
              />
              <button
                type="button"
                onClick={() => setShowToken(!showToken)}
                className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                title={showToken ? "隐藏 Token" : "显示 Token"}
              >
                {showToken ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
              </button>
            </div>
            <Button
              type="button"
              variant="outline"
              disabled={testing || !config.bot_token.trim()}
              onClick={handleTestToken}
              className="rounded-xl px-4 text-xs font-medium"
            >
              {testing ? "测试中..." : "测试连接"}
            </Button>
            <Button
              type="button"
              onClick={() => handleSave(config)}
              className="rounded-xl px-4 text-xs font-medium"
            >
              保存凭据
            </Button>
          </div>
        </div>
      </SettingCard>

      {/* 2. OpenCode 执行引擎与策略 */}
      <SettingCard
        id="section-remote-bot-engine"
        icon={Terminal}
        title="OpenCode 执行引擎与策略"
        description="选择 Telegram 远程会话所绑定的 OpenCode CLI 版本，并定制空闲交互模式与权限自动授权策略。"
        headerAction={
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={scanningVersions}
              onClick={handleScanVersions}
              className="h-8 rounded-lg text-xs gap-1.5"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${scanningVersions ? "animate-spin" : ""}`} />
              {scanningVersions ? "正在扫描..." : "重新扫描"}
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleBrowseCli}
              className="h-8 rounded-lg text-xs gap-1.5"
            >
              <FolderOpen className="w-3.5 h-3.5" />
              浏览选取
            </Button>
          </div>
        }
      >
        {/* OpenCode 版本单选卡片网格 */}
        <div className="space-y-2.5">
          <div className="text-xs font-medium text-muted-foreground flex items-center justify-between">
            <span>已安装 OpenCode CLI 版本 (必选生效一项)</span>
            {!config.opencode_bin_path && (
              <span className="text-amber-500 flex items-center gap-1 font-normal">
                <AlertCircle className="w-3.5 h-3.5" /> 必须选择版本后方可启用远程控制
              </span>
            )}
          </div>

          {installedVersions.length > 0 ? (
            <div className="space-y-2">
              {installedVersions.map((item) => {
                const isSelected = config.opencode_bin_path === item.path;
                const isV2 = item.version_type === "v2" || item.major_version >= 2;

                return (
                  <div
                    key={item.path}
                    onClick={() => handleSelectVersion(item)}
                    className={`p-3 rounded-xl border transition-all cursor-pointer flex items-center justify-between gap-3 ${
                      isSelected
                        ? "bg-primary/5 border-primary/50 shadow-sm"
                        : "bg-muted/10 hover:bg-muted/20 border-border/60"
                    }`}
                  >
                    <div className="flex items-center gap-3 min-w-0 flex-1">
                      <div
                        className={`w-4 h-4 rounded-full border flex items-center justify-center flex-shrink-0 transition-colors ${
                          isSelected
                            ? "border-primary bg-primary text-primary-foreground"
                            : "border-muted-foreground/40 bg-background"
                        }`}
                      >
                        {isSelected && <Check className="w-2.5 h-2.5 stroke-[3]" />}
                      </div>

                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="font-semibold text-sm text-foreground">
                            OpenCode {item.version}
                          </span>
                          <span
                            className={`px-1.5 py-0.5 text-[10px] font-bold rounded uppercase tracking-wider ${
                              isV2
                                ? "bg-purple-500/10 text-purple-600 dark:text-purple-400 border border-purple-500/20"
                                : "bg-sky-500/10 text-sky-600 dark:text-sky-400 border border-sky-500/20"
                            }`}
                          >
                            {isV2 ? "OpenCode v2" : "OpenCode v1"}
                          </span>
                          {isSelected && (
                            <Badge variant="success" className="text-[10px] py-0 px-1.5 h-4 font-normal">
                              当前生效
                            </Badge>
                          )}
                        </div>
                        <Tooltip content={item.path}>
                          <div className="text-xs text-muted-foreground font-mono truncate mt-0.5 max-w-md">
                            {item.path}
                          </div>
                        </Tooltip>
                      </div>
                    </div>

                    <div className="flex items-center gap-2 flex-shrink-0">
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={probingCli}
                        onClick={(e) => {
                          e.stopPropagation();
                          void handleProbeCli(item.path);
                        }}
                        className="h-7 px-2.5 text-xs text-muted-foreground hover:text-foreground"
                      >
                        测试可用性
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="p-4 rounded-xl border border-dashed bg-muted/10 text-center space-y-1">
              <div className="text-sm font-medium text-muted-foreground flex items-center justify-center gap-1.5">
                <AlertCircle className="w-4 h-4 text-amber-500" />
                未在本机检测到已安装的 OpenCode CLI
              </div>
              <div className="text-xs text-muted-foreground">
                请确认终端是否已安装 OpenCode（支持 v1/v2），或点击右上角「浏览选取」手动指定执行文件路径。
              </div>
            </div>
          )}

          {/* 探测结果浮层 */}
          {cliProbeResult && (
            <div
              className={`p-2.5 rounded-xl text-xs font-mono border flex items-start gap-2 ${
                cliProbeResult.success
                  ? "bg-emerald-500/10 border-emerald-500/20 text-emerald-700 dark:text-emerald-300"
                  : "bg-destructive/10 border-destructive/20 text-destructive"
              }`}
            >
              {cliProbeResult.success ? (
                <CheckCircle2 className="w-4 h-4 flex-shrink-0 mt-0.5 text-emerald-500" />
              ) : (
                <XCircle className="w-4 h-4 flex-shrink-0 mt-0.5 text-destructive" />
              )}
              <div className="flex-1 break-all whitespace-pre-wrap">{cliProbeResult.output}</div>
            </div>
          )}
        </div>

        <hr className="border-border/50" />

        {/* 策略设置项 1: 自然语言自由对话 */}
        <div className="flex items-center justify-between gap-4">
          <div className="space-y-0.5">
            <h4 className="text-sm font-semibold text-foreground">
              允许空闲状态下的自然语言自由对话
            </h4>
            <p className="text-xs text-muted-foreground leading-relaxed">
              开启后，直接发送一段文字即可与 OpenCode 自由对话；关闭后空闲时仅响应 <code className="text-primary font-mono">/planning</code>、<code className="text-primary font-mono">/help</code> 等显式指令，防止移动端误触打字意外触发任务。
            </p>
          </div>
          <Switch
            checked={config.enable_natural_dialogue}
            onCheckedChange={(checked) =>
              handleSave({
                ...config,
                enable_natural_dialogue: checked,
              })
            }
          />
        </div>

        <hr className="border-border/50" />

        {/* 策略设置项 2: Coding 阶段常规权限自动通过 */}
        <div className="flex items-center justify-between gap-4">
          <div className="space-y-0.5">
            <h4 className="text-sm font-semibold text-foreground">
              Coding 阶段常规权限自动通过
            </h4>
            <p className="text-xs text-muted-foreground leading-relaxed">
              开启后 OpenCode 读写代码、运行测试将自动授权，仅高危操作抛出手机端卡片审批；大幅提升远程免确认流畅度。
            </p>
          </div>
          <Switch
            checked={config.auto_approve_permissions}
            onCheckedChange={(checked) =>
              handleSave({
                ...config,
                auto_approve_permissions: checked,
              })
            }
          />
        </div>
      </SettingCard>

      {/* 3. 常用工作区与管理员白名单 */}
      <SettingCard
        id="section-remote-bot-workspaces"
        icon={Layers}
        title="常用工作区与管理员白名单"
        description="管理推送至 Telegram /projects 菜单的精选工作区，以及拥有机器人家长控制权限的管理员 Chat ID。"
        headerAction={
          <div className="flex items-center gap-2">
            {systemWorkspaces.length > 0 && (
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handleImportTop5Recent}
                className="h-8 rounded-lg text-xs gap-1.5"
                title="一键将最近活跃的 5 个系统工作空间加入精选列表"
              >
                <Sparkles className="w-3.5 h-3.5 text-amber-500" />
                选入活跃 Top 5
              </Button>
            )}
            <Button
              type="button"
              size="sm"
              onClick={() => setIsAddWorkspaceOpen(true)}
              className="h-8 rounded-lg text-xs gap-1.5"
            >
              <Plus className="w-3.5 h-3.5" />
              添加工作区
            </Button>
          </div>
        }
      >
        {/* 精选工作区列表 */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <div className="text-sm font-semibold text-foreground flex items-center gap-1.5">
              <span>Telegram 精选工作区</span>
              <span className="text-xs font-normal text-muted-foreground">
                ({config.workspace_directories?.length || 0} 个项目)
              </span>
            </div>
            <span className="text-xs text-muted-foreground">
              手机端输入 <code className="text-primary font-mono">/projects</code> 即可快捷切换
            </span>
          </div>

          {(!config.workspace_directories || config.workspace_directories.length === 0) ? (
            <div className="p-4 rounded-xl border border-dashed bg-muted/10 text-center space-y-1.5">
              <div className="text-xs text-muted-foreground">
                暂无精选常用项目。可点击右上角「选入活跃 Top 5」或「添加工作区」快速加入。
              </div>
            </div>
          ) : (
            <div className="space-y-1.5">
              {config.workspace_directories.map((dir) => {
                const isActive = config.active_workspace === dir;
                const sysWs = systemWorkspaces.find((w) => w.workspace.root_path === dir);
                const displayName = sysWs
                  ? sysWs.workspace.name
                  : (dir.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || dir);

                return (
                  <div
                    key={dir}
                    className={`flex items-center justify-between px-3 py-2.5 rounded-xl border text-xs transition-colors ${
                      isActive
                        ? "bg-primary/5 border-primary/40"
                        : "bg-muted/10 hover:bg-muted/20 border-border/60"
                    }`}
                  >
                    <div className="flex items-center gap-2.5 min-w-0 flex-1">
                      <button
                        type="button"
                        onClick={() => handleSetActiveWorkspace(dir)}
                        title={isActive ? "当前 Telegram 活跃项目" : "设为当前活跃项目"}
                        className={`flex items-center justify-center w-5 h-5 rounded-full border transition-colors ${
                          isActive
                            ? "bg-primary border-primary text-primary-foreground shadow-sm"
                            : "border-muted-foreground/30 hover:border-primary text-transparent hover:text-muted-foreground"
                        }`}
                      >
                        <Star className={`w-3 h-3 ${isActive ? "fill-primary-foreground text-primary-foreground" : ""}`} />
                      </button>

                      <span className="font-semibold text-foreground shrink-0">{displayName}</span>

                      {sysWs && (
                        <span className="px-1.5 py-0.5 rounded text-[10px] bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20 shrink-0">
                          系统工作区
                        </span>
                      )}

                      <Tooltip content={dir}>
                        <span className="font-mono text-muted-foreground truncate max-w-xs">{dir}</span>
                      </Tooltip>
                    </div>

                    <div className="flex items-center gap-2 shrink-0">
                      {isActive ? (
                        <Badge variant="success" className="text-[10px] py-0 px-2 h-5 font-normal">
                          当前活跃
                        </Badge>
                      ) : (
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => handleSetActiveWorkspace(dir)}
                          className="h-6 px-2 text-[11px] text-muted-foreground hover:text-foreground"
                        >
                          设为活跃
                        </Button>
                      )}
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        onClick={() => handleRemoveWorkspaceDir(dir)}
                        className="h-6 w-6 text-muted-foreground hover:text-destructive transition-colors"
                        title="移出常用精选"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <hr className="border-border/50" />

        {/* 管理员安全白名单 (Chat ID) */}
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <div>
              <div className="text-sm font-semibold text-foreground flex items-center gap-1.5">
                <ShieldCheck className="w-4 h-4 text-primary" />
                授权管理员白名单 (Chat ID)
              </div>
              <div className="text-xs text-muted-foreground mt-0.5">
                仅白名单用户可在 Telegram 中向机器人下达执行指令与确认审批
              </div>
            </div>

            <div className="flex items-center gap-2">
              <input
                type="text"
                value={newChatId}
                onChange={(e) => setNewChatId(e.target.value)}
                placeholder="纯数字 Chat ID"
                className="w-36 px-2.5 py-1 text-xs rounded-lg border bg-background font-mono focus:outline-none focus:ring-1 focus:ring-primary/30"
              />
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handleAddChatId}
                className="h-7 px-2.5 text-xs gap-1"
              >
                <Plus className="w-3.5 h-3.5" />
                添加
              </Button>
            </div>
          </div>

          {config.allowed_chat_ids.length === 0 ? (
            <div className="p-3.5 rounded-xl border border-dashed bg-muted/10 text-xs text-muted-foreground text-center">
              暂无管理员。可在 Telegram 私聊你的机器人发送 <code className="font-mono text-primary font-semibold">/bind</code> 快速完成自动绑定。
            </div>
          ) : (
            <div className="flex flex-wrap gap-2">
              {config.allowed_chat_ids.map((id) => (
                <div
                  key={id}
                  className="flex items-center gap-2 px-3 py-1.5 rounded-full border bg-muted/20 text-xs font-mono group hover:border-primary/40 transition-colors"
                >
                  <User className="w-3 h-3 text-muted-foreground group-hover:text-primary transition-colors" />
                  <span className="text-foreground font-medium">{id}</span>
                  <button
                    type="button"
                    onClick={() => handleRemoveChatId(id)}
                    className="text-muted-foreground hover:text-destructive transition-colors ml-0.5"
                    title="移除该管理员"
                  >
                    <Trash2 className="w-3 h-3" />
                  </button>
                </div>
              ))}
            </div>
          )}

          <div className="text-[11px] text-muted-foreground flex items-center gap-1">
            <span>💡 提示: 在 Telegram 中私聊机器人发送 <code className="font-mono text-primary">/bind</code> 可一键完成身份验证与绑定。</span>
          </div>
        </div>
      </SettingCard>

      {/* 添加精选工作区弹窗 (Dialog) */}
      <Dialog open={isAddWorkspaceOpen} onOpenChange={setIsAddWorkspaceOpen}>
        <DialogContent className="max-w-xl max-h-[85vh] flex flex-col p-6 rounded-2xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2 text-base">
              <Layers className="w-4 h-4 text-primary" />
              添加 Telegram 精选工作区
            </DialogTitle>
            <DialogDescription className="text-xs">
              从 OneSpace 已有工作空间中挑选，或添加未录入的本地文件夹目录。
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 py-2 flex-1 overflow-hidden flex flex-col">
            {/* 系统工作空间搜索选择 */}
            <div className="space-y-2 flex-1 flex flex-col min-h-0">
              <div className="flex items-center justify-between">
                <span className="text-xs font-semibold text-foreground">
                  OneSpace 系统工作空间 (共 {systemWorkspaces.length} 个)
                </span>
                <div className="relative w-48">
                  <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
                  <input
                    type="text"
                    value={wsSearchQuery}
                    onChange={(e) => setWsSearchQuery(e.target.value)}
                    placeholder="搜索名称或标签..."
                    className="w-full pl-8 pr-2.5 py-1 text-xs rounded-lg border bg-background focus:outline-none focus:ring-1 focus:ring-primary/30"
                  />
                </div>
              </div>

              <div className="flex-1 overflow-y-auto space-y-1.5 pr-1 border rounded-xl p-2 bg-muted/10 max-h-56">
                {filteredSystemWorkspaces.length === 0 ? (
                  <div className="text-xs text-muted-foreground text-center py-6">未找到匹配的工作空间</div>
                ) : (
                  filteredSystemWorkspaces.map((w) => {
                    const root = w.workspace.root_path;
                    const isSelected = (config.workspace_directories || []).includes(root);
                    return (
                      <div
                        key={w.workspace.id}
                        className="flex items-center justify-between p-2 rounded-lg border bg-background/60 hover:bg-background text-xs transition-colors"
                      >
                        <div className="min-w-0 flex-1 mr-3">
                          <div className="flex items-center gap-2">
                            <span className="font-semibold text-foreground truncate">{w.workspace.name}</span>
                            {w.workspace.tags && w.workspace.tags.length > 0 && (
                              <span className="text-[10px] text-muted-foreground truncate">
                                [{w.workspace.tags.slice(0, 2).join(", ")}]
                              </span>
                            )}
                          </div>
                          <div className="text-[11px] text-muted-foreground truncate font-mono mt-0.5">{root}</div>
                        </div>

                        {isSelected ? (
                          <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => handleRemoveWorkspaceDir(root)}
                            className="h-7 px-2.5 text-[11px] font-medium text-emerald-600 bg-emerald-500/10 hover:bg-destructive/10 hover:text-destructive transition-colors shrink-0 gap-1 rounded-lg"
                          >
                            <Check className="w-3 h-3" />
                            已在精选
                          </Button>
                        ) : (
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => {
                              const current = config.workspace_directories || [];
                              const updated = {
                                ...config,
                                workspace_directories: [...current, root],
                                active_workspace: config.active_workspace || root,
                              };
                              handleSave(updated);
                            }}
                            className="h-7 px-2.5 text-[11px] font-medium transition-colors shrink-0 gap-1 rounded-lg"
                          >
                            <Plus className="w-3 h-3" />
                            加入精选
                          </Button>
                        )}
                      </div>
                    );
                  })
                )}
              </div>
            </div>

            {/* 自定义未录入目录添加 */}
            <div className="pt-2 border-t space-y-2">
              <span className="text-xs font-semibold text-foreground">添加未录入的本地文件夹</span>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={newWorkspaceDir}
                  onChange={(e) => setNewWorkspaceDir(e.target.value)}
                  placeholder="输入目录绝对路径..."
                  className="flex-1 px-3 py-1.5 text-xs rounded-xl border bg-background font-mono focus:outline-none focus:ring-1 focus:ring-primary/30"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={handleBrowseWorkspace}
                  className="rounded-xl text-xs gap-1.5"
                >
                  <FolderOpen className="w-3.5 h-3.5" />
                  浏览
                </Button>
                <Button
                  type="button"
                  size="sm"
                  onClick={handleAddWorkspaceDir}
                  className="rounded-xl text-xs gap-1.5"
                >
                  <Plus className="w-3.5 h-3.5" />
                  添加
                </Button>
              </div>
            </div>
          </div>

          <div className="pt-3 border-t flex justify-end">
            <Button
              type="button"
              onClick={() => setIsAddWorkspaceOpen(false)}
              className="rounded-xl px-5 text-xs"
            >
              完成
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
