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
  FolderCheck,
  Check,
  Search,
  Sparkles,
  Layers,
  AlertCircle,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { TelegramIcon } from "@/components/common/TelegramIcon";
import { refreshRuntimeStatus } from "@/lib/runtimeStatus";
import { SettingCard } from "../../common/SettingCard";
import type { ApiResp, WorkspaceView } from "@/components/Workspaces/types";
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
    success: boolean;
    output: string;
  } | null>(null);
  const [newWorkspaceDir, setNewWorkspaceDir] = useState("");

  // 系统工作空间联动状态
  const [systemWorkspaces, setSystemWorkspaces] = useState<WorkspaceView[]>([]);
  const [wsSearchQuery, setWsSearchQuery] = useState("");

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
        const resp = await invoke<ApiResp<WorkspaceView[]>>("workspaces_list");
        if (resp && Array.isArray(resp.data)) {
          // 按最近活跃时间倒序排列
          const sorted = [...resp.data].sort(
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

  const handleProbeCli = async () => {
    if (!config.opencode_bin_path) {
      onShowToast?.("error", "请先选择一个具体的 OpenCode 版本");
      return;
    }
    try {
      setProbingCli(true);
      setCliProbeResult(null);
      const res = await remoteBotProbeOpencode(config.opencode_bin_path);
      setCliProbeResult({ success: true, output: res.trim() });
      onShowToast?.("success", `探测成功: ${res.split("\n")[0] || res}`);
    } catch (e: any) {
      const errStr = e?.message || String(e);
      setCliProbeResult({ success: false, output: errStr });
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
    onShowToast?.("success", `已将最近活跃的 ${Math.min(5, top5Paths.length)} 个项目同步至手机端常用列表`);
  };

  // 过滤系统工作空间列表
  const filteredSystemWorkspaces = useMemo(() => {
    const query = wsSearchQuery.trim().toLowerCase();
    if (!query) {
      // 默认展示最近活跃的前 6 个供快速挑选
      return systemWorkspaces.slice(0, 6);
    }
    return systemWorkspaces.filter(
      (w) =>
        w.workspace.name.toLowerCase().includes(query) ||
        w.workspace.root_path.toLowerCase().includes(query) ||
        (w.workspace.tags && w.workspace.tags.some((t) => t.toLowerCase().includes(query)))
    );
  }, [systemWorkspaces, wsSearchQuery]);

  return (
    <SettingCard
      id="section-remote-bot"
      icon={TelegramIcon}
      title={t("remoteBotSettings", "Telegram 远程工作台")}
      description={t(
        "remoteBotSettingsDesc",
        "通过 Telegram 手机客户端远程唤起 Planning 需求规划、Ask Question 交互决策以及 OpenCode CLI 实施与测试。"
      )}
    >
      {/* 状态看板 */}
      <div className="grid gap-3 grid-cols-3">
        <div className="rounded-xl border bg-muted/20 p-3.5">
          <div className="text-xs uppercase tracking-wider text-muted-foreground font-semibold">
            {t("status", "运行状态")}
          </div>
          <div className="mt-1 text-base font-semibold flex items-center gap-2">
            {status?.running ? (
              <>
                <CheckCircle2 className="w-4 h-4 text-emerald-500" />
                <span className="text-emerald-600 dark:text-emerald-400">已在线轮询</span>
              </>
            ) : (
              <>
                <XCircle className="w-4 h-4 text-muted-foreground" />
                <span className="text-muted-foreground">已停止</span>
              </>
            )}
          </div>
        </div>

        <div className="rounded-xl border bg-muted/20 p-3.5">
          <div className="text-xs uppercase tracking-wider text-muted-foreground font-semibold">
            机器人账户
          </div>
          <div className="mt-1 text-base font-semibold truncate">
            {status?.bot_username ? `@${status.bot_username}` : "未连接"}
          </div>
        </div>

        <div className="rounded-xl border bg-muted/20 p-3.5">
          <div className="text-xs uppercase tracking-wider text-muted-foreground font-semibold">
            已授权管理员
          </div>
          <div className="mt-1 text-base font-semibold">
            {config.allowed_chat_ids.length > 0 ? `${config.allowed_chat_ids.length} 个账号` : "未绑定"}
          </div>
        </div>
      </div>

      {status?.last_error && (
        <div className="p-3 rounded-xl bg-destructive/10 text-destructive text-sm flex items-center gap-2">
          <ShieldAlert className="w-4 h-4 flex-shrink-0" />
          <span className="truncate">最后异常: {status.last_error}</span>
        </div>
      )}

      {/* 统一网络代理联动提示 */}
      <div className="p-3 rounded-xl bg-muted/40 border border-border/40 text-xs text-muted-foreground flex items-center justify-between">
        <span>🌐 当前网络若无法直连 Telegram，系统将自动经由「服务 -&gt; 网络代理」的统一代理策略转发请求。</span>
      </div>

      {/* 启用开关 */}
      <div className="flex items-center justify-between border-t pt-4">
        <div>
          <div className="text-sm font-medium">启用 Telegram 远程控制</div>
          <div className="text-xs text-muted-foreground">
            基于出站长轮询连接，无需公网 IP 或开通端口
          </div>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={config.enabled}
          onClick={() => {
            if (!config.enabled && !config.opencode_bin_path?.trim()) {
              onShowToast?.("error", "必须选择一个具体的 OpenCode 版本后才能启用 Telegram 远程工作台");
              return;
            }
            handleSave({ ...config, enabled: !config.enabled });
          }}
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
            config.enabled ? "bg-primary" : "bg-muted"
          }`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
              config.enabled ? "translate-x-6" : "translate-x-1"
            }`}
          />
        </button>
      </div>

      {/* Token 输入 */}
      <div className="space-y-2 border-t pt-4">
        <label className="text-sm font-medium flex items-center gap-2">
          <KeyRound className="w-4 h-4 text-muted-foreground" />
          Telegram Bot Token
        </label>
        <div className="flex gap-2">
          <input
            type={showToken ? "text" : "password"}
            value={config.bot_token}
            onChange={(e) => setConfig({ ...config, bot_token: e.target.value })}
            placeholder="例如: 123456789:ABCdefGHIjklMNOpqrsTUVwxyz"
            className="flex-1 px-3 py-1.5 text-sm rounded-lg border bg-background font-mono"
          />
          <button
            type="button"
            onClick={() => setShowToken(!showToken)}
            className="px-3 py-1.5 text-xs font-medium rounded-lg border hover:bg-muted"
          >
            {showToken ? "隐藏" : "显示"}
          </button>
          <button
            type="button"
            disabled={testing || !config.bot_token.trim()}
            onClick={handleTestToken}
            className="px-3 py-1.5 text-xs font-medium rounded-lg bg-primary/10 text-primary hover:bg-primary/20 disabled:opacity-50"
          >
            {testing ? "测试中..." : "测试连接"}
          </button>
          <button
            type="button"
            onClick={() => handleSave(config)}
            className="px-3 py-1.5 text-xs font-medium rounded-lg bg-primary text-primary-foreground hover:bg-primary/90"
          >
            保存
          </button>
        </div>
        <p className="text-xs text-muted-foreground">
          在 Telegram 中私聊 @BotFather 发送 /newbot 即可快速申请 Bot Token。
        </p>
      </div>

      {/* 白名单 Chat IDs */}
      <div className="space-y-3 border-t pt-4">
        <div className="flex items-center justify-between">
          <div>
            <div className="text-sm font-medium">授权管理员白名单 (Chat ID)</div>
            <div className="text-xs text-muted-foreground">
              仅允许白名单内的用户向机器人发送控制与审批指令
            </div>
          </div>
          <div className="flex gap-2">
            <input
              type="text"
              value={newChatId}
              onChange={(e) => setNewChatId(e.target.value)}
              placeholder="输入纯数字 Chat ID"
              className="w-44 px-2.5 py-1 text-xs rounded-lg border bg-background font-mono"
            />
            <button
              type="button"
              onClick={handleAddChatId}
              className="px-2.5 py-1 text-xs font-medium rounded-lg bg-primary/10 text-primary hover:bg-primary/20 flex items-center gap-1"
            >
              <Plus className="w-3.5 h-3.5" />
              添加
            </button>
          </div>
        </div>

        {config.allowed_chat_ids.length === 0 ? (
          <div className="p-3 rounded-lg border border-dashed text-xs text-muted-foreground text-center">
            暂无绑定的管理员。可在 Telegram 私聊你的机器人发送 <code className="font-mono text-primary">/bind</code> 快速完成初始绑定。
          </div>
        ) : (
          <div className="space-y-1.5">
            {config.allowed_chat_ids.map((id) => (
              <div
                key={id}
                className="flex items-center justify-between px-3 py-2 rounded-lg border bg-muted/20 text-xs font-mono"
              >
                <span>ID: {id}</span>
                <button
                  type="button"
                  onClick={() => handleRemoveChatId(id)}
                  className="text-muted-foreground hover:text-destructive transition-colors"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* 自然语言自由对话开关 */}
      <div className="flex items-center justify-between border-t pt-4">
        <div>
          <div className="text-sm font-medium">允许空闲状态下的自然语言自由对话</div>
          <div className="text-xs text-muted-foreground">
            开启后直接发送一段文字即可与 OpenCode 自由对话（遵循其标准流程，自主选择 coding 实施或 planning 规划）；关闭后空闲状态仅响应 <code>/planning</code>、<code>/help</code> 等显式指令，防止移动端误触打字意外触发任务。
          </div>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={config.enable_natural_dialogue}
          onClick={() =>
            handleSave({
              ...config,
              enable_natural_dialogue: !config.enable_natural_dialogue,
            })
          }
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
            config.enable_natural_dialogue ? "bg-primary" : "bg-muted"
          }`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
              config.enable_natural_dialogue ? "translate-x-6" : "translate-x-1"
            }`}
          />
        </button>
      </div>

      {/* OpenCode CLI 版本与执行路径配置（必选） */}
      <div className="space-y-3 border-t pt-4">
        <div className="flex items-center justify-between">
          <div>
            <label className="text-sm font-medium flex items-center gap-2">
              <Terminal className="w-4 h-4 text-muted-foreground" />
              OpenCode CLI 版本选择 (必选)
            </label>
            <div className="text-xs text-muted-foreground">
              Telegram 远程通讯逻辑同时支持 OpenCode v1 与 v2 版本。请在本机已安装的 OpenCode 中选择一个具体的生效版本。
            </div>
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              disabled={scanningVersions}
              onClick={handleScanVersions}
              className="px-2.5 py-1 text-xs font-medium rounded-lg border hover:bg-muted flex items-center gap-1.5 transition-colors disabled:opacity-50"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${scanningVersions ? "animate-spin" : ""}`} />
              {scanningVersions ? "正在扫描..." : "重新扫描"}
            </button>
            <button
              type="button"
              onClick={handleBrowseCli}
              className="px-2.5 py-1 text-xs font-medium rounded-lg border hover:bg-muted flex items-center gap-1.5 transition-colors"
            >
              <FolderOpen className="w-3.5 h-3.5" />
              浏览指定文件
            </button>
          </div>
        </div>

        {/* 版本卡片列表 */}
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
                      ? "bg-primary/5 border-primary shadow-sm"
                      : "bg-muted/10 hover:bg-muted/30 border-border/60"
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
                      {isSelected && <Check className="w-2.5 h-2.5" />}
                    </div>

                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="font-semibold text-sm">
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
                          <span className="px-1.5 py-0.5 text-[10px] font-medium rounded bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20">
                            当前生效
                          </span>
                        )}
                      </div>
                      <div className="text-xs text-muted-foreground font-mono truncate mt-0.5" title={item.path}>
                        {item.path}
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center gap-2 flex-shrink-0">
                    <button
                      type="button"
                      disabled={probingCli}
                      onClick={(e) => {
                        e.stopPropagation();
                        void handleProbeCli();
                      }}
                      className="px-2 py-1 text-xs rounded border hover:bg-muted text-muted-foreground"
                    >
                      测试可用性
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <div className="p-3.5 rounded-xl border border-dashed bg-muted/20 text-center space-y-1.5">
            <div className="text-sm font-medium text-muted-foreground flex items-center justify-center gap-1.5">
              <AlertCircle className="w-4 h-4 text-amber-500" />
              未检测到本机已安装的 OpenCode
            </div>
            <div className="text-xs text-muted-foreground">
              请确认终端是否已安装 OpenCode CLI（支持 v1 或 v2），或点击右上角「浏览指定文件」手动选取。
            </div>
          </div>
        )}

        {/* 必选警告提示 */}
        {!config.opencode_bin_path && (
          <div className="p-2.5 rounded-lg text-xs border border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-400 flex items-center gap-2">
            <AlertCircle className="w-4 h-4 flex-shrink-0" />
            <span>⚠️ 必须选择一个具体的 OpenCode 版本，方可启用 Telegram 远程控制。</span>
          </div>
        )}

        {cliProbeResult && (
          <div
            className={`p-2.5 rounded-lg text-xs font-mono border flex items-start gap-2 ${
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

      {/* 联动 OneSpace 系统工作空间与常用项目候选 */}
      <div className="space-y-4 border-t pt-4">
        <div className="flex items-center justify-between">
          <div>
            <div className="text-sm font-medium flex items-center gap-2">
              <FolderCheck className="w-4 h-4 text-muted-foreground" />
              Telegram 常用工作区精选列表
            </div>
            <div className="text-xs text-muted-foreground mt-0.5">
              为防止移动端刷屏，仅精选常驻项目（推荐 3~8 个）推送至手机端 <code>/projects</code> 按钮；其他项目可在手机端随时输入 <code>/projects &lt;名称&gt;</code> 全量搜索切换。
            </div>
          </div>
          {systemWorkspaces.length > 0 && (
            <button
              type="button"
              onClick={handleImportTop5Recent}
              className="px-2.5 py-1 text-xs font-medium rounded-lg bg-primary/10 text-primary hover:bg-primary/20 flex items-center gap-1 shrink-0"
              title="一键将最近活跃的 5 个系统工作空间加入手机精选列表"
            >
              <Sparkles className="w-3.5 h-3.5 text-amber-500" />
              选入活跃 Top 5
            </button>
          )}
        </div>

        {/* 已选精选项目列表 */}
        {(!config.workspace_directories || config.workspace_directories.length === 0) ? (
          <div className="p-3 rounded-lg border border-dashed text-xs text-muted-foreground text-center">
            暂无精选项目。可从下方系统工作空间勾选加入，或直接点击上方「选入活跃 Top 5」。
          </div>
        ) : (
          <div className="space-y-1.5">
            {config.workspace_directories.map((dir) => {
              const isActive = config.active_workspace === dir;
              // 匹配系统工作空间获取友好名称
              const sysWs = systemWorkspaces.find((w) => w.workspace.root_path === dir);
              const displayName = sysWs ? sysWs.workspace.name : (dir.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || dir);
              return (
                <div
                  key={dir}
                  className={`flex items-center justify-between px-3 py-2 rounded-lg border text-xs transition-colors ${
                    isActive
                      ? "bg-primary/5 border-primary/40 font-medium"
                      : "bg-muted/20 hover:bg-muted/30"
                  }`}
                >
                  <div className="flex items-center gap-2.5 min-w-0">
                    <button
                      type="button"
                      onClick={() => handleSetActiveWorkspace(dir)}
                      title={isActive ? "当前 Telegram 目标项目" : "设为目标项目"}
                      className={`flex items-center justify-center w-4 h-4 rounded-full border transition-colors ${
                        isActive
                          ? "bg-primary border-primary text-primary-foreground"
                          : "border-muted-foreground/40 hover:border-primary"
                      }`}
                    >
                      {isActive && <Check className="w-2.5 h-2.5 stroke-[3]" />}
                    </button>
                    <span className="font-semibold text-foreground shrink-0">{displayName}</span>
                    {sysWs && (
                      <span className="px-1.5 py-0.2 rounded text-[10px] bg-blue-500/10 text-blue-600 dark:text-blue-400 shrink-0">
                        系统工作区
                      </span>
                    )}
                    <span className="font-mono text-muted-foreground truncate">{dir}</span>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    {isActive ? (
                      <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-primary/10 text-primary">
                        当前活跃
                      </span>
                    ) : (
                      <button
                        type="button"
                        onClick={() => handleSetActiveWorkspace(dir)}
                        className="text-xs text-muted-foreground hover:text-foreground transition-colors"
                      >
                        设为活跃
                      </button>
                    )}
                    <button
                      type="button"
                      onClick={() => handleRemoveWorkspaceDir(dir)}
                      className="text-muted-foreground hover:text-destructive transition-colors ml-1"
                      title="移出精选常用"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}

        {/* 从 OneSpace 系统工作空间中快速勾选添加 */}
        {systemWorkspaces.length > 0 && (
          <div className="p-3 rounded-xl border bg-muted/10 space-y-3">
            <div className="flex items-center justify-between gap-2">
              <div className="text-xs font-semibold text-foreground flex items-center gap-1.5">
                <Layers className="w-3.5 h-3.5 text-primary" />
                从 OneSpace 系统工作空间添加 (共 {systemWorkspaces.length} 个)
              </div>
              <div className="relative w-48">
                <Search className="w-3.5 h-3.5 absolute left-2 top-2 text-muted-foreground" />
                <input
                  type="text"
                  value={wsSearchQuery}
                  onChange={(e) => setWsSearchQuery(e.target.value)}
                  placeholder="搜索项目名称或标签..."
                  className="w-full pl-7 pr-2 py-1 text-xs rounded-md border bg-background"
                />
              </div>
            </div>

            <div className="grid gap-1.5 max-h-48 overflow-y-auto pr-1">
              {filteredSystemWorkspaces.length === 0 ? (
                <div className="text-xs text-muted-foreground text-center py-2">未找到匹配的工作空间</div>
              ) : (
                filteredSystemWorkspaces.map((w) => {
                  const root = w.workspace.root_path;
                  const isSelected = (config.workspace_directories || []).includes(root);
                  return (
                    <div
                      key={w.workspace.id}
                      className="flex items-center justify-between px-2.5 py-1.5 rounded-lg border bg-background/50 hover:bg-background text-xs"
                    >
                      <div className="min-w-0 flex-1 mr-2">
                        <div className="flex items-center gap-2">
                          <span className="font-medium text-foreground truncate">{w.workspace.name}</span>
                          {w.workspace.tags && w.workspace.tags.length > 0 && (
                            <span className="text-[10px] text-muted-foreground truncate">
                              [{w.workspace.tags.slice(0, 2).join(", ")}]
                            </span>
                          )}
                        </div>
                        <div className="text-[11px] text-muted-foreground truncate font-mono">{root}</div>
                      </div>
                      {isSelected ? (
                        <button
                          type="button"
                          onClick={() => handleRemoveWorkspaceDir(root)}
                          className="px-2 py-0.5 text-[11px] font-medium rounded text-emerald-600 bg-emerald-500/10 hover:bg-destructive/10 hover:text-destructive transition-colors shrink-0 flex items-center gap-1"
                        >
                          <Check className="w-3 h-3" />
                          已在精选
                        </button>
                      ) : (
                        <button
                          type="button"
                          onClick={() => {
                            const current = config.workspace_directories || [];
                            const updated = {
                              ...config,
                              workspace_directories: [...current, root],
                              active_workspace: config.active_workspace || root,
                            };
                            handleSave(updated);
                          }}
                          className="px-2 py-0.5 text-[11px] font-medium rounded border hover:bg-primary hover:text-primary-foreground transition-colors shrink-0 flex items-center gap-1"
                        >
                          <Plus className="w-3 h-3" />
                          加入精选
                        </button>
                      )}
                    </div>
                  );
                })
              )}
            </div>
          </div>
        )}

        {/* 自定义非系统工作区目录添加 */}
        <div className="flex items-center justify-between pt-1">
          <div className="text-xs text-muted-foreground">
            若需添加未在 OneSpace 中录入的独立目录：
          </div>
          <div className="flex gap-2">
            <input
              type="text"
              value={newWorkspaceDir}
              onChange={(e) => setNewWorkspaceDir(e.target.value)}
              placeholder="输入目录绝对路径"
              className="w-52 px-2.5 py-1 text-xs rounded-lg border bg-background font-mono"
            />
            <button
              type="button"
              onClick={handleBrowseWorkspace}
              className="px-2.5 py-1 text-xs font-medium rounded-lg border hover:bg-muted flex items-center gap-1"
            >
              <FolderOpen className="w-3.5 h-3.5" />
              浏览
            </button>
            <button
              type="button"
              onClick={handleAddWorkspaceDir}
              className="px-2.5 py-1 text-xs font-medium rounded-lg bg-primary/10 text-primary hover:bg-primary/20 flex items-center gap-1"
            >
              <Plus className="w-3.5 h-3.5" />
              添加
            </button>
          </div>
        </div>
      </div>

      {/* 权限自动审批设置 */}
      <div className="flex items-center justify-between border-t pt-4">
        <div>
          <div className="text-sm font-medium">Coding 阶段常规权限自动通过</div>
          <div className="text-xs text-muted-foreground">
            开启后 OpenCode 读写代码、运行测试将自动授权，仅高危操作抛出手机端卡片审批
          </div>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={config.auto_approve_permissions}
          onClick={() =>
            handleSave({
              ...config,
              auto_approve_permissions: !config.auto_approve_permissions,
            })
          }
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
            config.auto_approve_permissions ? "bg-primary" : "bg-muted"
          }`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
              config.auto_approve_permissions ? "translate-x-6" : "translate-x-1"
            }`}
          />
        </button>
      </div>

      {/* 底部重试/刷新 */}
      <div className="flex justify-end pt-2">
        <button
          type="button"
          disabled={loading}
          onClick={handleRestart}
          className="px-3 py-1.5 text-xs font-medium rounded-lg border hover:bg-muted flex items-center gap-1.5"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
          重启轮询服务
        </button>
      </div>
    </SettingCard>
  );
}
