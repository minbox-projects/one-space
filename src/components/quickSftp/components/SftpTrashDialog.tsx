import type { TFunction } from "i18next";
import {
  FileText,
  Folder,
  Loader2,
  RefreshCw,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react";
import { formatBytes } from "@/lib/sftp";
import type { SftpTrashItem } from "@/lib/sftp";

type SftpTrashDialogProps = {
  t: TFunction;
  trashItems: SftpTrashItem[];
  trashLoading: boolean;
  restoringTrashId: string | null;
  onClose: () => void;
  onRefresh: () => void;
  onClearTrash: () => void;
  onRestoreTrashItem: (item: SftpTrashItem) => void;
};

export function SftpTrashDialog({
  t,
  trashItems,
  trashLoading,
  restoringTrashId,
  onClose,
  onRefresh,
  onClearTrash,
  onRestoreTrashItem,
}: SftpTrashDialogProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div className="w-full max-w-3xl rounded-lg border bg-card p-4 shadow-2xl space-y-3">
        {/* 弹窗头部 */}
        <div className="flex items-center justify-between border-b pb-3">
          <div className="flex items-center gap-2">
            <div className="rounded-md bg-destructive/10 p-1.5 text-destructive">
              <Trash2 className="h-4 w-4" />
            </div>
            <div>
              <h3 className="text-sm font-semibold text-foreground">
                {t("sftpTrashModalTitle", "远程回收站")}
              </h3>
              <p className="text-[11px] text-muted-foreground">
                {t("sftpTrashItemCount", "共 {{count}} 个已回收项目", {
                  count: trashItems.length,
                })}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {trashItems.length > 0 ? (
              <button
                type="button"
                onClick={onClearTrash}
                className="inline-flex h-7 items-center gap-1.5 rounded-md border border-destructive/30 bg-destructive/10 px-2.5 text-xs font-medium text-destructive hover:bg-destructive/20 transition-colors"
              >
                <Trash2 className="h-3 w-3" />
                <span>{t("sftpClearTrash", "清空回收站")}</span>
              </button>
            ) : null}

            <button
              type="button"
              onClick={onRefresh}
              disabled={trashLoading}
              className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50"
              title={t("sftpRefresh", "刷新")}
            >
              <RefreshCw
                className={`h-3.5 w-3.5 ${trashLoading ? "animate-spin" : ""}`}
              />
            </button>

            <button
              type="button"
              onClick={onClose}
              className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
              title={t("close", "关闭")}
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>

        {/* 回收站列表内容 */}
        <div className="max-h-96 overflow-y-auto">
          {trashLoading && trashItems.length === 0 ? (
            <div className="flex items-center justify-center py-12 text-muted-foreground text-xs gap-2">
              <Loader2 className="h-4 w-4 animate-spin text-primary" />
              <span>{t("loading", "正在加载...")}</span>
            </div>
          ) : trashItems.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-12 text-center text-xs text-muted-foreground space-y-2">
              <div className="rounded-full bg-muted/60 p-3">
                <Trash2 className="h-6 w-6 text-muted-foreground/50" />
              </div>
              <span>{t("sftpTrashEmpty", "回收站为空。")}</span>
            </div>
          ) : (
            <table className="w-full text-left text-xs">
              <thead className="border-b bg-muted/20 text-[11px] text-muted-foreground uppercase">
                <tr>
                  <th className="py-2 px-3 font-medium">
                    {t("sftpColName", "名称")}
                  </th>
                  <th className="py-2 px-3 font-medium">
                    {t("sftpColOriginalPath", "原路径")}
                  </th>
                  <th className="py-2 px-3 font-medium w-24">
                    {t("sftpColSize", "大小")}
                  </th>
                  <th className="py-2 px-3 font-medium w-36">
                    {t("sftpColDeletedAt", "删除时间")}
                  </th>
                  <th className="py-2 px-3 font-medium w-28 text-right">
                    {t("sftpColActions", "操作")}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border/60">
                {trashItems.map((item) => (
                  <tr
                    key={item.id}
                    className="hover:bg-muted/50 transition-colors"
                  >
                    <td className="py-2 px-3 font-mono">
                      <div className="flex items-center gap-2">
                        {item.is_dir ? (
                          <Folder className="h-4 w-4 text-sky-500 fill-sky-500/20 shrink-0" />
                        ) : (
                          <FileText className="h-4 w-4 text-muted-foreground shrink-0" />
                        )}
                        <span className="font-medium text-foreground truncate max-w-[180px]">
                          {item.name}
                        </span>
                      </div>
                    </td>
                    <td className="py-2 px-3 font-mono text-muted-foreground text-[11px] truncate max-w-xs">
                      <span title={item.original_path}>
                        {item.original_path}
                      </span>
                    </td>
                    <td className="py-2 px-3 font-mono text-muted-foreground">
                      {item.is_dir ? "-" : formatBytes(item.size)}
                    </td>
                    <td className="py-2 px-3 text-muted-foreground text-[11px]">
                      {item.deleted_at}
                    </td>
                    <td className="py-2 px-3 text-right">
                      <button
                        type="button"
                        onClick={() => onRestoreTrashItem(item)}
                        disabled={restoringTrashId === item.id}
                        className="inline-flex items-center gap-1.5 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-1 text-xs font-medium text-primary hover:bg-primary/20 disabled:opacity-50 transition-colors"
                        title={t("sftpRestoreToOriginal", "恢复到原路径")}
                      >
                        {restoringTrashId === item.id ? (
                          <Loader2 className="h-3 w-3 animate-spin" />
                        ) : (
                          <RotateCcw className="h-3 w-3" />
                        )}
                        <span>{t("sftpRestore", "恢复")}</span>
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}
