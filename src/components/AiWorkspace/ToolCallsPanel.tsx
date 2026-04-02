import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { AssistantToolCall } from "@/lib/aiWorkspace";
import {
  getToolCallDisplayName,
  getToolCallMeta,
} from "@/lib/assistantToolCalls";

function getStatusLabel(status: string, t: (key: string, fallback: string) => string) {
  switch (status) {
    case "pending":
      return t("toolCallStatusPending", "Pending");
    case "running":
      return t("toolCallStatusRunning", "Running");
    case "success":
      return t("toolCallStatusSuccess", "Success");
    case "failed":
      return t("toolCallStatusFailed", "Failed");
    case "cancelled":
      return t("toolCallStatusCancelled", "Cancelled");
    default:
      return status;
  }
}

function getStatusClass(status: string) {
  switch (status) {
    case "pending":
    case "running":
      return "border-primary/30 bg-primary/5 text-primary";
    case "success":
      return "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400";
    case "failed":
      return "border-destructive/30 bg-destructive/5 text-destructive";
    case "cancelled":
      return "border-muted bg-muted/40 text-muted-foreground";
    default:
      return "border-muted text-muted-foreground";
  }
}

export function ToolCallsPanel({
  toolCalls,
  className,
}: {
  toolCalls: AssistantToolCall[];
  className?: string;
}) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);

  if (toolCalls.length === 0) {
    return null;
  }

  const hasActiveTool = toolCalls.some(
    (tool) => tool.status === "pending" || tool.status === "running",
  );

  return (
    <div
      className={`${className ? `${className} ` : ""}rounded-xl border border-dashed bg-muted/20 px-3 py-3`}
    >
      <div className="flex items-center justify-between gap-3">
        <button
          type="button"
          onClick={() => setExpanded((current) => !current)}
          className="inline-flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground hover:text-foreground"
        >
          <ChevronRight
            className={`h-3 w-3 transition-transform ${expanded ? "rotate-90" : ""}`}
          />
          {t("toolCallsLabel", "Tool Calls")}
        </button>
        <div className="inline-flex items-center gap-2 text-[11px] text-muted-foreground">
          {hasActiveTool ? (
            <span className="rounded-full border border-primary/20 bg-primary/10 px-2 py-0.5 font-medium text-primary">
              {t("toolCallStatusRunning", "Running")}
            </span>
          ) : null}
          <span className="rounded-full border border-border/70 bg-background px-2 py-0.5 font-medium">
            {toolCalls.length}
          </span>
        </div>
      </div>
      {expanded ? (
        <div className="mt-3 space-y-2">
          {toolCalls.map((tool, index) => {
            const meta = getToolCallMeta(tool);
            return (
              <div
                key={tool.id || `${tool.name}-${index}`}
                className="select-text rounded-lg border bg-background px-3 py-2"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="break-words text-sm font-medium">
                      {getToolCallDisplayName(tool)}
                    </div>
                    {meta ? (
                      <div className="mt-1 break-words text-[11px] leading-5 text-muted-foreground">
                        {meta}
                      </div>
                    ) : null}
                    {tool.summary ? (
                      <div className="mt-1 text-xs leading-5 text-muted-foreground">
                        {tool.summary}
                      </div>
                    ) : null}
                  </div>
                  <span
                    className={`shrink-0 rounded-full border px-2 py-0.5 text-[11px] uppercase tracking-[0.16em] ${getStatusClass(tool.status)}`}
                  >
                    {getStatusLabel(tool.status, t)}
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
