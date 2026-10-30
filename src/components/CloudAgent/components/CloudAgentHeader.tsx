import { useState, useEffect } from "react";
import { CloudCog, Server, Settings } from "lucide-react";
import type { TFunction } from "i18next";
import { CustomSelect } from "@/components/common/CustomSelect";
import { sshHostsList } from "@/lib/sshTunnels";

interface CloudAgentHeaderProps {
  t: TFunction;
  selectedHost: string;
  setSelectedHost: (host: string) => void;
}

export function CloudAgentHeader({
  t,
  selectedHost,
  setSelectedHost,
}: CloudAgentHeaderProps) {
  const [hosts, setHosts] = useState<{ id: string; label: string }[]>([]);

  useEffect(() => {
    sshHostsList<{ name: string; host_name: string }[]>()
      .then((res) => {
        if (res && res.length > 0) {
          setHosts(res.map((h) => ({ id: h.name, label: h.name })));
        }
      })
      .catch(() => {});
  }, []);

  return (
    <header className="flex flex-wrap items-center justify-between gap-4">
      {/* 左侧：图标 + 标题与简要描述（对齐 AiGateway 标准） */}
      <div className="flex items-start gap-3">
        <div className="rounded-lg p-2 bg-primary/10 text-primary">
          <CloudCog className="h-5 w-5" />
        </div>
        <div className="space-y-0.5">
          <h1 className="text-xl font-bold tracking-tight text-foreground">
            {t("cloudAgentTitle", "AI 云端智能体 (Cloud Agent)")}
          </h1>
          <p className="max-w-2xl text-xs text-muted-foreground">
            {t(
              "cloudAgentSubtitle",
              "长程编程断网保活 · 本地终端一键直连 · AI 网关双端同步 · Issues 自动实施流水线"
            )}
          </p>
        </div>
      </div>

      {/* 右侧：环境主机切换与设置快捷入口 */}
      <div className="flex items-center gap-2.5 shrink-0">
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Server className="h-3.5 w-3.5" />
          <div className="w-36">
            <CustomSelect
              size="sm"
              value={selectedHost}
              onChange={setSelectedHost}
              placeholder={t("cloudAgentSelectHostPlaceholder", "请选择主机")}
              options={
                hosts.length > 0
                  ? hosts.map((h) => ({ value: h.id, label: h.label }))
                  : selectedHost
                  ? [{ value: selectedHost, label: selectedHost }]
                  : []
              }
            />
          </div>
        </div>

        <button
          type="button"
          onClick={() => {
            const win = window as any;
            win.setSettingsTab?.("cloud-agent");
            win.setSettingsOpen?.(true);
          }}
          title={t("settings", "设置")}
          aria-label={t("settings", "设置")}
          className="flex h-8 w-8 items-center justify-center rounded-lg border border-border/80 bg-background text-muted-foreground hover:bg-muted hover:text-foreground transition-all cursor-pointer shadow-2xs"
        >
          <Settings className="h-4 w-4" />
        </button>
      </div>
    </header>
  );
}
