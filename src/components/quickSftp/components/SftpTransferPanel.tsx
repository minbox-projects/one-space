import type { TFunction } from "i18next";
import {
  AlertCircle,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Download,
  RotateCw,
  Upload,
  X,
} from "lucide-react";
import {
  formatBytes,
  formatEta,
  formatSpeed,
} from "@/lib/sftp";
import type { SftpTransferProgressPayload } from "@/lib/sftp";

type SftpTransferPanelProps = {
  t: TFunction;
  isZh: boolean;
  taskList: SftpTransferProgressPayload[];
  activeTaskCount: number;
  transferPanelOpen: boolean;
  onTogglePanel: () => void;
  onClearCompleted: () => void;
  onCancelTransfer: (taskId: string) => void;
  onRetryTransfer: (task: SftpTransferProgressPayload) => void;
};

export function SftpTransferPanel({
  t,
  isZh,
  taskList,
  activeTaskCount,
  transferPanelOpen,
  onTogglePanel,
  onClearCompleted,
  onCancelTransfer,
  onRetryTransfer,
}: SftpTransferPanelProps) {
  return (
    <div className="rounded-lg border bg-card shadow-sm overflow-hidden text-xs">
      {/* 顶栏控制条 */}
      <div className="flex items-center justify-between border-b bg-muted/30 px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="font-semibold text-foreground">
            {t("sftpTransfersTitle", "Transfers")}
          </span>
          <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
            {activeTaskCount > 0
              ? `${activeTaskCount} ${t("sftpTransferring", "Active")}`
              : t("sftpTransferCompleted", "Completed")}
          </span>
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onClearCompleted}
            className="text-muted-foreground hover:text-foreground transition-colors text-[11px]"
          >
            {t("sftpClearCompleted", "Clear Completed")}
          </button>
          <button
            type="button"
            onClick={onTogglePanel}
            className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            {transferPanelOpen ? (
              <ChevronUp className="h-3.5 w-3.5" />
            ) : (
              <ChevronDown className="h-3.5 w-3.5" />
            )}
          </button>
        </div>
      </div>

      {/* 任务内容列表 */}
      {transferPanelOpen ? (
        <div className="divide-y divide-border/60 max-h-56 overflow-y-auto p-1">
          {taskList.map((task) => {
            const isUpload = task.transfer_type === "upload";
            return (
              <div
                key={task.task_id}
                className="p-2 space-y-1.5 hover:bg-muted/30 rounded-md transition-colors"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2 min-w-0">
                    {isUpload ? (
                      <span className="inline-flex items-center gap-1 rounded bg-purple-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-purple-600 dark:text-purple-400">
                        <Upload className="h-3 w-3" />
                        {t("sftpUpload", "Upload")}
                      </span>
                    ) : (
                      <span className="inline-flex items-center gap-1 rounded bg-sky-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-sky-600 dark:text-sky-400">
                        <Download className="h-3 w-3" />
                        {t("sftpDownload", "Download")}
                      </span>
                    )}
                    <span className="font-mono font-medium truncate max-w-xs text-foreground">
                      {task.file_name}
                    </span>
                  </div>

                  <div className="flex items-center gap-2 shrink-0">
                    {task.status === "transferring" ? (
                      <>
                        <span className="font-mono text-xs font-semibold text-primary">
                          {task.percentage.toFixed(1)}%
                        </span>
                        <button
                          type="button"
                          onClick={() =>
                            onCancelTransfer(task.task_id)
                          }
                          className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-rose-500 transition-colors"
                          title={t("sftpCancel", "Cancel")}
                        >
                          <X className="h-3.5 w-3.5" />
                        </button>
                      </>
                    ) : task.status === "completed" ? (
                      <span className="inline-flex items-center gap-1 text-emerald-600 dark:text-emerald-400 text-[11px] font-medium">
                        <CheckCircle2 className="h-3.5 w-3.5" />
                        {t("sftpTransferCompleted", "Completed")}
                      </span>
                    ) : task.status === "cancelled" ? (
                      <span className="text-muted-foreground text-[11px]">
                        {t("sftpTransferCancelled", "Cancelled")}
                      </span>
                    ) : (
                      <div className="flex items-center gap-1.5">
                        <span
                          className="inline-flex items-center gap-1 text-rose-500 text-[11px] max-w-[140px] truncate"
                          title={task.error || t("sftpTransferFailed", "Failed")}
                        >
                          <AlertCircle className="h-3.5 w-3.5 shrink-0" />
                          <span>{t("sftpTransferFailed", "Failed")}</span>
                        </span>
                        {task.retry_params ? (
                          <button
                            type="button"
                            onClick={() => onRetryTransfer(task)}
                            className="inline-flex items-center gap-1 rounded bg-rose-500/10 px-2 py-0.5 text-[11px] font-medium text-rose-600 dark:text-rose-400 hover:bg-rose-500/20 transition-colors"
                            title={t("sftpRetry", "Retry")}
                          >
                            <RotateCw className="h-3 w-3" />
                            <span>{t("sftpRetry", "Retry")}</span>
                          </button>
                        ) : null}
                      </div>
                    )}
                  </div>
                </div>

                {/* 进度条 */}
                <div className="h-1.5 w-full rounded-full bg-muted overflow-hidden">
                  <div
                    className={`h-full transition-all duration-200 ${
                      task.status === "error"
                        ? "bg-rose-500"
                        : task.status === "completed"
                          ? "bg-emerald-500"
                          : task.status === "cancelled"
                            ? "bg-zinc-400"
                            : "bg-primary"
                    }`}
                    style={{
                      width: `${Math.min(100, Math.max(0, task.percentage))}%`,
                    }}
                  />
                </div>

                {/* 统计指标行：大小 / 网速 / 剩余时长 */}
                <div className="flex items-center justify-between text-[11px] text-muted-foreground font-mono">
                  <div>
                    {formatBytes(task.transferred_bytes)} /{" "}
                    {formatBytes(task.total_bytes)}
                  </div>
                  {task.status === "transferring" ? (
                    <div className="flex items-center gap-3">
                      <span className="text-foreground">
                        {formatSpeed(task.bytes_per_second)}
                      </span>
                      <span className="text-muted-foreground">
                        {formatEta(task.eta_seconds, isZh)}
                      </span>
                    </div>
                  ) : task.status === "error" && task.error ? (
                    <span
                      className="text-rose-500 text-[10px] truncate max-w-xs"
                      title={task.error}
                    >
                      {task.error}
                    </span>
                  ) : null}
                </div>
              </div>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
