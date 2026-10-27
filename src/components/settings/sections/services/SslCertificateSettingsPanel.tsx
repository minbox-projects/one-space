import type { Dispatch, SetStateAction } from "react";
import type { TFunction } from "i18next";
import { ShieldCheck, Clock, X, Plus } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { SettingCard } from "../../common/SettingCard";
import type { SslConfig } from "@/lib/sslCertificate";

interface SslCertificateSettingsPanelProps {
  t: TFunction;
  sslConfig: SslConfig;
  setSslConfig: Dispatch<SetStateAction<SslConfig>>;
  newCheckTimeInput: string;
  setNewCheckTimeInput: (val: string) => void;
  sslAlertDaysInput: string;
  setSslAlertDaysInput: (val: string) => void;
  onShowToast?: (type: "success" | "error", text: string) => void;
}

export function SslCertificateSettingsPanel({
  t,
  sslConfig,
  setSslConfig,
  newCheckTimeInput,
  setNewCheckTimeInput,
  sslAlertDaysInput,
  setSslAlertDaysInput,
  onShowToast,
}: SslCertificateSettingsPanelProps) {
  return (
    <div className="space-y-6">
      <SettingCard
        id="section-ssl-certificate"
        icon={ShieldCheck}
        title={t(
          "sslCertificateSettingsTitle",
          "SSL 证书巡检与告警设置",
        )}
        description={t(
          "sslCertificateSettingsDesc",
          "配置域名 SSL 证书的每日自动巡检时间节点与临期告警阈值。",
        )}
      >
        {/* Auto check switch */}
        <div className="flex items-center justify-between">
          <div>
            <h4 className="text-sm font-semibold text-foreground">
              {t("sslAutoCheckEnabled", "启用每日自动巡检")}
            </h4>
            <p className="text-xs text-muted-foreground mt-0.5">
              {t(
                "sslAutoCheckEnabledDesc",
                "开启后，系统将在设定的时间节点自动对关注列表中的域名进行 TLS 握手探测。",
              )}
            </p>
          </div>
          <Switch
            checked={sslConfig.autoCheckEnabled}
            onCheckedChange={(checked) =>
              setSslConfig((prev) => ({
                ...prev,
                autoCheckEnabled: checked,
              }))
            }
          />
        </div>

        <hr className="border-border/50" />

        {/* Check times */}
        <div className="space-y-3">
          <div>
            <h4 className="text-sm font-semibold text-foreground">
              {t("sslCheckTimesTitle", "每日自动巡检时间节点")}
            </h4>
            <p className="text-xs text-muted-foreground mt-0.5">
              {t(
                "sslCheckTimesDesc",
                "支持配置多个 24 小时制“时:分”时间节点（例如 09:00、18:00）。",
              )}
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            {sslConfig.checkTimes.map((timeStr) => (
              <span
                key={timeStr}
                className="inline-flex items-center gap-1.5 rounded-lg border bg-muted/60 px-3 py-1 text-xs font-mono font-medium text-foreground"
              >
                <Clock className="w-3.5 h-3.5 text-muted-foreground" />
                {timeStr}
                {sslConfig.checkTimes.length > 1 && (
                  <button
                    type="button"
                    onClick={() =>
                      setSslConfig((prev) => ({
                        ...prev,
                        checkTimes: prev.checkTimes.filter(
                          (t) => t !== timeStr,
                        ),
                      }))
                    }
                    className="ml-1 rounded p-0.5 hover:bg-muted text-muted-foreground hover:text-rose-500 transition-colors"
                    title={t("delete", "删除")}
                  >
                    <X className="w-3 h-3" />
                  </button>
                )}
              </span>
            ))}
          </div>

          <div className="flex items-center gap-2 pt-1">
            <input
              type="time"
              value={newCheckTimeInput}
              onChange={(e) => setNewCheckTimeInput(e.target.value)}
              className="rounded-lg border bg-background px-3 py-1.5 text-xs font-mono text-foreground focus:outline-none focus:ring-2 focus:ring-primary/20"
            />
            <button
              type="button"
              onClick={() => {
                const trimmed = newCheckTimeInput.trim();
                if (!/^([01]\d|2[0-3]):([0-5]\d)$/.test(trimmed)) {
                  onShowToast?.(
                    "error",
                    t("invalidTimeFormat", "时间格式无效，请输入 HH:mm 格式"),
                  );
                  return;
                }
                if (sslConfig.checkTimes.includes(trimmed)) {
                  onShowToast?.("error", t("duplicateTimeNode", "该时间节点已存在"));
                  return;
                }
                const updated = [...sslConfig.checkTimes, trimmed].sort();
                setSslConfig((prev) => ({
                  ...prev,
                  checkTimes: updated,
                }));
              }}
              className="inline-flex items-center gap-1 rounded-lg border bg-background px-3 py-1.5 text-xs font-medium text-foreground hover:bg-muted transition-colors"
            >
              <Plus className="w-3.5 h-3.5" />
              <span>{t("addCheckTime", "添加时间节点")}</span>
            </button>
          </div>
        </div>

        <hr className="border-border/50" />

        {/* Alert days threshold */}
        <div className="space-y-3">
          <div>
            <h4 className="text-sm font-semibold text-foreground">
              {t("sslAlertThresholdTitle", "临期告警阈值（天）")}
            </h4>
            <p className="text-xs text-muted-foreground mt-0.5">
              {t(
                "sslAlertThresholdDesc",
                "当证书剩余天数小于等于该天数时，自动标记为临期并生成消息中心通知。",
              )}
            </p>
          </div>

          <div className="flex items-center gap-3">
            <input
              type="number"
              min={1}
              max={365}
              value={sslAlertDaysInput}
              onChange={(e) => setSslAlertDaysInput(e.target.value)}
              className="w-32 rounded-xl border bg-background px-3.5 py-2 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-primary/20"
            />
            <span className="text-xs text-muted-foreground">天</span>
          </div>

          <div className="rounded-xl bg-amber-500/10 border border-amber-500/20 p-3.5 text-xs text-amber-700 dark:text-amber-400">
            <p className="font-semibold">
              {t("sslDailyReminderNotice", "⚡ 每日提醒机制说明：")}
            </p>
            <p className="mt-1 text-[11px] leading-relaxed">
              {t(
                "sslDailyReminderNoticeDetail",
                "当证书剩余天数 <= 7 天且未续期时，系统每天都会在自动巡检后提醒并加入消息中心，直到证书续期恢复正常。",
              )}
            </p>
          </div>
        </div>
      </SettingCard>
    </div>
  );
}
