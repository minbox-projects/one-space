import { useState, type ReactNode } from "react";
import {
  Blocks,
  BookOpen,
  Brain,
  FolderOpen,
  Globe,
  NotebookPen,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { LucideIcon } from "lucide-react";
import type { McpServerCardItem } from "@/lib/assistantMcpDisplay";

interface CapabilityBadgesProps {
  knowledgeBaseCount: number;
  knowledgeBaseIds: string[];
  mcpServerCount: number;
  mcpServerIds: string[];
  mcpServerLabels?: string[];
  mcpServerCards?: McpServerCardItem[];
  workspaceReadEnabled: boolean;
  onWorkspaceReadToggle?: () => void;
  notesSearchEnabled: boolean;
  onNotesSearchToggle?: () => void;
  memoryEnabled: boolean;
  onMemoryToggle?: () => void;
  webSearchEnabled: boolean;
  onWebSearchToggle?: () => void;
}

function connectionStatusClass(status: string) {
  switch (status) {
    case "ready":
      return "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400";
    case "failed":
      return "border-destructive/30 bg-destructive/5 text-destructive";
    default:
      return "border-muted bg-muted/40 text-muted-foreground";
  }
}

function IconButton({
  active,
  ariaLabel,
  count,
  icon: Icon,
  onClick,
  title,
}: {
  active: boolean;
  ariaLabel: string;
  count?: number;
  icon: LucideIcon;
  onClick?: () => void;
  title: string;
}) {
  const interactive = Boolean(onClick);

  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      aria-label={ariaLabel}
      aria-pressed={active}
      className={`relative inline-flex h-8 w-8 items-center justify-center rounded-lg border transition-colors ${
        active
          ? "border-primary bg-primary/5 text-primary"
          : "border-muted text-muted-foreground"
      } ${interactive ? "hover:bg-muted hover:text-foreground" : "cursor-default"}`}
    >
      <Icon className="h-4 w-4" />
      {typeof count === "number" && count > 0 ? (
        <span className="absolute -right-1 -top-1 min-w-[16px] rounded-full bg-primary px-1 text-[10px] font-medium leading-4 text-primary-foreground">
          {count > 9 ? "9+" : count}
        </span>
      ) : null}
    </button>
  );
}

function BadgePopover({
  open,
  onClose,
  title,
  items,
  content,
  icon,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  items: string[];
  content?: ReactNode;
  icon: ReactNode;
}) {
  const { t } = useTranslation();
  if (!open) return null;

  return (
    <div className="absolute bottom-full left-0 z-50 mb-2 w-[420px] max-w-[min(90vw,520px)] rounded-xl border bg-card shadow-lg">
      <div className="flex items-center justify-between border-b px-3 py-2">
        <div className="flex items-center gap-2 text-sm font-medium">
          {icon}
          <span>{title}</span>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="rounded-lg p-1 hover:bg-muted"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
      <div className="max-h-[280px] overflow-y-auto p-2">
        {content ? (
          content
        ) : items.length > 0 ? (
          items.map((item, index) => (
            <div
              key={`${item}-${index}`}
              className="rounded-lg px-3 py-2 text-sm"
            >
              {item}
            </div>
          ))
        ) : (
          <div className="px-3 py-2 text-sm text-muted-foreground">
            {t("noneLabel", "None")}
          </div>
        )}
      </div>
    </div>
  );
}

export function CapabilityBadges({
  knowledgeBaseCount,
  knowledgeBaseIds,
  mcpServerCount,
  mcpServerIds,
  mcpServerLabels = [],
  mcpServerCards = [],
  workspaceReadEnabled,
  onWorkspaceReadToggle,
  notesSearchEnabled,
  onNotesSearchToggle,
  memoryEnabled,
  onMemoryToggle,
  webSearchEnabled,
  onWebSearchToggle,
}: CapabilityBadgesProps) {
  const { t } = useTranslation();
  const [kbPopoverOpen, setKbPopoverOpen] = useState(false);
  const [mcpPopoverOpen, setMcpPopoverOpen] = useState(false);

  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="relative">
        <IconButton
          active={knowledgeBaseCount > 0}
          ariaLabel={t("knowledgeBasesLabel", "Knowledge Bases")}
          count={knowledgeBaseCount}
          icon={BookOpen}
          onClick={() => setKbPopoverOpen((open) => !open)}
          title={t("knowledgeBasesLabel", "Knowledge Bases")}
        />
        <BadgePopover
          open={kbPopoverOpen}
          onClose={() => setKbPopoverOpen(false)}
          title={t("knowledgeBasesLabel", "Knowledge Bases")}
          items={knowledgeBaseIds}
          icon={<BookOpen className="h-3.5 w-3.5" />}
        />
      </div>

      <div className="relative">
        <IconButton
          active={mcpServerCount > 0}
          ariaLabel={t("mcpServersLabel", "MCP Servers")}
          count={mcpServerCount}
          icon={Blocks}
          onClick={() => setMcpPopoverOpen((open) => !open)}
          title={t("mcpServersLabel", "MCP Servers")}
        />
        <BadgePopover
          open={mcpPopoverOpen}
          onClose={() => setMcpPopoverOpen(false)}
          title={t("mcpServersLabel", "MCP Servers")}
          items={mcpServerLabels.length > 0 ? mcpServerLabels : mcpServerIds}
          content={
            mcpServerCards.length > 0 ? (
              <div className="space-y-2">
                {mcpServerCards.map((item) => (
                  <div
                    key={item.serverId}
                    className="w-full rounded-xl border bg-background px-4 py-3"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="text-sm font-medium">{item.name}</div>
                        <div className="mt-1 text-[11px] uppercase tracking-[0.16em] text-muted-foreground">
                          {item.meta}
                        </div>
                      </div>
                      <span
                        className={`shrink-0 rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] ${connectionStatusClass(item.connectionStatus)}`}
                      >
                        {item.connectionLabel}
                      </span>
                    </div>
                    <div className="mt-2 text-xs text-foreground">
                      {t("mcpConnectionStatusLabel", "Connection")}: {item.connectionLabel}
                    </div>
                    <div className="mt-2 text-xs leading-5 text-muted-foreground">
                      {item.summary}
                    </div>
                    <div className="mt-2 text-xs text-foreground">
                      {item.previewSummary}
                    </div>
                    {item.impactLabels.length > 0 ? (
                      <div className="mt-2 flex flex-wrap gap-2">
                        {item.impactLabels.map((label) => (
                          <span
                            key={`${item.serverId}-${label}`}
                            className="rounded-full border border-dashed px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] text-muted-foreground"
                          >
                            {label}
                          </span>
                        ))}
                      </div>
                    ) : null}
                    {item.toolNames.length > 0 ? (
                      <div className="mt-2 flex flex-wrap gap-2">
                        {item.toolNames.map((toolName) => (
                          <span
                            key={`${item.serverId}-${toolName}`}
                            className="rounded-full border bg-muted/40 px-2 py-0.5 text-[11px] text-muted-foreground"
                          >
                            {toolName}
                          </span>
                        ))}
                      </div>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : undefined
          }
          icon={<Blocks className="h-3.5 w-3.5" />}
        />
      </div>

      <IconButton
        active={workspaceReadEnabled}
        ariaLabel={t("workspaceReadLabel", "Workspace Read")}
        icon={FolderOpen}
        onClick={onWorkspaceReadToggle}
        title={t("workspaceReadLabel", "Workspace Read")}
      />

      <IconButton
        active={notesSearchEnabled}
        ariaLabel={t("notesSearchLabel", "Notes Search")}
        icon={NotebookPen}
        onClick={onNotesSearchToggle}
        title={t("notesSearchLabel", "Notes Search")}
      />

      <IconButton
        active={memoryEnabled}
        ariaLabel={t("memoryLabel", "Memory")}
        icon={Brain}
        onClick={onMemoryToggle}
        title={t("memoryLabel", "Memory")}
      />

      <IconButton
        active={webSearchEnabled}
        ariaLabel={t("networkRetrievalToggle", "Toggle web search")}
        icon={Globe}
        onClick={onWebSearchToggle}
        title={t("networkRetrieval", "Web Search")}
      />
    </div>
  );
}
