import { useState } from "react";
import { Book, Database, FileText, Globe, MemoryStick, X } from "lucide-react";
import { useTranslation } from "react-i18next";

interface CapabilityBadgesProps {
  knowledgeBaseCount: number;
  knowledgeBaseIds: string[];
  mcpServerCount: number;
  mcpServerIds: string[];
  workspaceReadEnabled: boolean;
  onWorkspaceReadToggle: () => void;
  notesSearchEnabled: boolean;
  onNotesSearchToggle: () => void;
  memoryEnabled: boolean;
  onMemoryToggle: () => void;
  webSearchEnabled: boolean;
  onWebSearchToggle: () => void;
}

function BadgePopover({
  open,
  onClose,
  title,
  items,
  icon,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  items: string[];
  icon: React.ReactNode;
}) {
  if (!open) return null;

  return (
    <div className="absolute bottom-full left-0 z-50 mb-2 min-w-[200px] rounded-xl border bg-card shadow-lg">
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
      <div className="max-h-[200px] overflow-y-auto p-2">
        {items.length > 0 ? (
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
            None
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

  const badgeBaseClass = (active: boolean) =>
    `inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium uppercase tracking-[0.08em] transition-colors ${
      active
        ? "border-primary bg-primary/5 text-primary"
        : "border-muted text-muted-foreground hover:border-primary/30"
    }`;

  return (
    <div className="flex flex-wrap items-center gap-2">
      {/* Knowledge Bases 徽章 */}
      <div className="relative">
        <button
          type="button"
          onClick={() => setKbPopoverOpen(!kbPopoverOpen)}
          className={badgeBaseClass(knowledgeBaseCount > 0)}
        >
          <Book className="h-3 w-3" />
          KB:{knowledgeBaseCount}
        </button>
        <BadgePopover
          open={kbPopoverOpen}
          onClose={() => setKbPopoverOpen(false)}
          title={t("knowledgeBasesLabel", "Knowledge Bases")}
          items={knowledgeBaseIds}
          icon={<Book className="h-3.5 w-3.5" />}
        />
      </div>

      {/* MCP Servers 徽章 */}
      <div className="relative">
        <button
          type="button"
          onClick={() => setMcpPopoverOpen(!mcpPopoverOpen)}
          className={badgeBaseClass(mcpServerCount > 0)}
        >
          <Database className="h-3 w-3" />
          MCP:{mcpServerCount}
        </button>
        <BadgePopover
          open={mcpPopoverOpen}
          onClose={() => setMcpPopoverOpen(false)}
          title={t("mcpServersLabel", "MCP Servers")}
          items={mcpServerIds}
          icon={<Database className="h-3.5 w-3.5" />}
        />
      </div>

      {/* Workspace Read 开关徽章 */}
      <button
        type="button"
        onClick={onWorkspaceReadToggle}
        className={badgeBaseClass(workspaceReadEnabled)}
      >
        <FileText className="h-3 w-3" />
        WS
      </button>

      {/* Notes Search 开关徽章 */}
      <button
        type="button"
        onClick={onNotesSearchToggle}
        className={badgeBaseClass(notesSearchEnabled)}
      >
        <FileText className="h-3 w-3" />
        NOTE
      </button>

      {/* Memory 开关徽章 */}
      <button
        type="button"
        onClick={onMemoryToggle}
        className={badgeBaseClass(memoryEnabled)}
      >
        <MemoryStick className="h-3 w-3" />
        MEM
      </button>

      {/* Web Search 开关徽章 */}
      <button
        type="button"
        onClick={onWebSearchToggle}
        className={badgeBaseClass(webSearchEnabled)}
        title={t("networkRetrieval", "联网检索")}
        aria-label={t("networkRetrievalToggle", "切换联网检索")}
        aria-pressed={webSearchEnabled}
      >
        <Globe className="h-3 w-3" />
        WEB
      </button>
    </div>
  );
}
