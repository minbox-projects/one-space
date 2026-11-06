import { useState, useEffect } from "react";
import type { TFunction } from "i18next";
import { CheckCircle2, XCircle, RefreshCw, KeyRound, ShieldAlert, Plus, Trash2 } from "lucide-react";
import { TelegramIcon } from "@/components/common/TelegramIcon";
import { refreshRuntimeStatus } from "@/lib/runtimeStatus";
import { SettingCard } from "../../common/SettingCard";
import {
  type RemoteBotConfig,
  type RemoteBotStatus,
  remoteBotGetConfig,
  remoteBotSaveConfig,
  remoteBotGetStatus,
  remoteBotTestConnection,
  remoteBotRestart,
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
    updated_at: Date.now(),
  });
  const [status, setStatus] = useState<RemoteBotStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [testing, setTesting] = useState(false);
  const [newChatId, setNewChatId] = useState("");
  const [showToken, setShowToken] = useState(false);

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    try {
      setLoading(true);
      const [cfg, st] = await Promise.all([
        remoteBotGetConfig(),
        remoteBotGetStatus(),
      ]);
      setConfig(cfg);
      setStatus(st);
    } catch (e: any) {
      onShowToast?.("error", `加载配置失败: ${e?.message || e}`);
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async (updated: RemoteBotConfig) => {
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
          onClick={() => handleSave({ ...config, enabled: !config.enabled })}
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
