import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Hash, Settings } from "lucide-react";
import type { SshCommonPortView } from "./types";

type CommonPortSelectMenuProps = {
  ports: SshCommonPortView[];
  onSelect: (portItem: SshCommonPortView) => void;
  onManage: () => void;
  className?: string;
  buttonLabel?: string;
};

export function CommonPortSelectMenu({
  ports,
  onSelect,
  onManage,
  className = "",
  buttonLabel,
}: CommonPortSelectMenuProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (event: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(event.target as Node)
      ) {
        setOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  return (
    <div ref={containerRef} className={`relative inline-block ${className}`}>
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        aria-haspopup="listbox"
        aria-expanded={open}
        title={t("sshTunnelSelectCommonPort", "Select from common ports")}
        className="inline-flex h-9 items-center gap-1.5 rounded-lg border border-input bg-background px-3 py-1.5 text-xs font-medium text-muted-foreground shadow-sm transition-colors hover:bg-muted hover:text-foreground shrink-0"
      >
        <Hash className="h-3.5 w-3.5 text-primary" />
        <span>{buttonLabel || t("sshTunnelCommonPorts", "Common Ports")}</span>
        <ChevronDown
          className={`h-3.5 w-3.5 transition-transform duration-150 ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>

      {open && (
        <div className="absolute right-0 top-full z-50 mt-1 w-72 rounded-xl border bg-popover p-1 shadow-lg ring-1 ring-black/5 animate-in fade-in-0 zoom-in-95">
          <div className="px-2.5 py-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground border-b mb-1">
            {t("sshTunnelCommonPorts", "Common Ports")}
          </div>

          <div className="max-h-64 overflow-y-auto space-y-0.5">
            {ports.length === 0 ? (
              <div className="px-3 py-3 text-xs text-center text-muted-foreground">
                {t("sshTunnelNoCommonPortsAvailable", "No common ports configured.")}
              </div>
            ) : (
              ports.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => {
                    onSelect(item);
                    setOpen(false);
                  }}
                  className="flex w-full items-center justify-between gap-2 rounded-lg px-2.5 py-2 text-left text-xs transition-colors hover:bg-muted focus-visible:bg-muted focus-visible:outline-none"
                >
                  <div className="flex flex-col min-w-0 pr-1">
                    <span className="font-medium text-foreground truncate">
                      {item.name}
                    </span>
                    {item.description && (
                      <span className="text-[11px] text-muted-foreground truncate">
                        {item.description}
                      </span>
                    )}
                  </div>
                  <span className="font-mono text-xs font-semibold text-primary shrink-0 bg-primary/10 rounded px-1.5 py-0.5">
                    {item.localPort} → {item.remotePort}
                  </span>
                </button>
              ))
            )}
          </div>

          <div className="border-t my-1" />

          <button
            type="button"
            onClick={() => {
              setOpen(false);
              onManage();
            }}
            className="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs font-medium text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          >
            <Settings className="h-3.5 w-3.5" />
            <span>{t("sshTunnelManageCommonPorts", "Manage Common Ports")}</span>
          </button>
        </div>
      )}
    </div>
  );
}
