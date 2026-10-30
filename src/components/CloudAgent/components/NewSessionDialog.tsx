import { useState, useEffect } from "react";
import { Sparkles, GitBranch, FolderGit2, Shield, Loader2, Terminal } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { TFunction } from "i18next";
import type { CloudAgentSettings } from "@/lib/cloudAgent";

interface NewSessionDialogProps {
  t: TFunction;
  settings: CloudAgentSettings;
  isOpen: boolean;
  onClose: () => void;
  onSubmit: (data: {
    repo: string;
    branch?: string;
    useYolo: boolean;
    extraArgs: string;
  }) => Promise<void>;
}

export function NewSessionDialog({
  t,
  settings,
  isOpen,
  onClose,
  onSubmit,
}: NewSessionDialogProps) {
  const [repo, setRepo] = useState("");
  const [branch, setBranch] = useState("");
  const [useYolo, setUseYolo] = useState(settings.opencodeYoloMode);
  const [extraArgs, setExtraArgs] = useState(settings.opencodeExtraArgs);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 当弹窗打开时，根据当前设置同步重置表单状态
  useEffect(() => {
    if (isOpen) {
      setRepo("");
      setBranch("");
      setUseYolo(settings.opencodeYoloMode);
      setExtraArgs(settings.opencodeExtraArgs);
      setError(null);
    }
  }, [isOpen, settings]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!repo.trim()) {
      setError(t("repoRequired", "请输入仓库名称、Git URL 或目录名"));
      return;
    }

    try {
      setIsSubmitting(true);
      setError(null);
      await onSubmit({
        repo: repo.trim(),
        branch: branch.trim() || undefined,
        useYolo,
        extraArgs: extraArgs.trim(),
      });
      onClose();
    } catch (err: any) {
      setError(err?.message || String(err));
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-lg sm:max-w-lg">
        <DialogHeader>
          <div className="flex items-center gap-2.5">
            <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <Sparkles className="h-5 w-5" />
            </div>
            <div>
              <DialogTitle className="text-base font-semibold">
                {t("createNewCloudTaskTitle", "新建云端特性任务")}
              </DialogTitle>
              <DialogDescription className="text-xs text-muted-foreground mt-0.5">
                {t(
                  "createNewCloudTaskDesc",
                  "在云端创建独立的 Git Worktree 隔离开发环境并调起本地终端直连。"
                )}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        {error && (
          <div className="rounded-lg border border-destructive/20 bg-destructive/10 p-3 text-xs text-destructive">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4 pt-1">
          {/* 目标仓库 */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-foreground flex items-center gap-1.5">
              <FolderGit2 className="h-3.5 w-3.5 text-muted-foreground" />
              {t("targetRepoLabel", "目标仓库或项目路径")}
              <span className="text-destructive">*</span>
            </label>
            <input
              type="text"
              value={repo}
              onChange={(e) => setRepo(e.target.value)}
              placeholder="例如: one-space 或 https://github.com/user/repo.git"
              className="w-full rounded-lg border border-input bg-background px-3 py-2 text-xs text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            />
          </div>

          {/* 特性分支名 */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-foreground flex items-center gap-1.5">
              <GitBranch className="h-3.5 w-3.5 text-muted-foreground" />
              {t("featureBranchLabel", "特性分支名称 (选填，留空则使用仓库主目录)")}
            </label>
            <input
              type="text"
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
              placeholder="例如: feat-login 或 fix-payment"
              className="w-full rounded-lg border border-input bg-background px-3 py-2 font-mono text-xs text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            />
          </div>

          {/* 授权模式 Switch */}
          <div className="rounded-lg border border-border bg-muted/30 p-3 flex items-center justify-between">
            <div className="space-y-0.5">
              <div className="text-xs font-medium text-foreground flex items-center gap-1.5">
                <Shield className="h-3.5 w-3.5 text-primary" />
                {t("yoloModeLabel", "完全自动授权模式 (--yolo)")}
              </div>
              <p className="text-[11px] text-muted-foreground">
                {t("yoloModeHint", "开启后 OpenCode 在云端会话无需交互式权限确认")}
              </p>
            </div>
            <Switch checked={useYolo} onCheckedChange={setUseYolo} />
          </div>

          {/* 额外启动参数 */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-foreground flex items-center gap-1.5">
              <Terminal className="h-3.5 w-3.5 text-muted-foreground" />
              {t("extraArgsLabel", "CLI 额外启动参数 (选填)")}
            </label>
            <input
              type="text"
              value={extraArgs}
              onChange={(e) => setExtraArgs(e.target.value)}
              placeholder="例如: --model cloud-coder"
              className="w-full rounded-lg border border-input bg-background px-3 py-2 font-mono text-xs text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            />
          </div>

          {/* 底部按钮栏 */}
          <div className="flex items-center justify-end gap-2 pt-2 border-t border-border/60">
            <button
              type="button"
              disabled={isSubmitting}
              onClick={onClose}
              className="rounded-lg border border-border bg-background px-3.5 py-1.5 text-xs font-medium text-foreground hover:bg-muted transition-colors cursor-pointer"
            >
              {t("cancel", "取消")}
            </button>
            <button
              type="submit"
              disabled={isSubmitting}
              className="inline-flex items-center gap-1.5 rounded-lg bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-foreground shadow-2xs hover:bg-primary/90 active:scale-98 transition-all disabled:opacity-50 cursor-pointer"
            >
              {isSubmitting ? (
                <>
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {t("creatingAndLaunching", "正在创建并进入现场...")}
                </>
              ) : (
                <>
                  <Sparkles className="h-3.5 w-3.5" />
                  {t("createAndEnter", "创建并进入现场")}
                </>
              )}
            </button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

// 保持向后兼容别名导出
export { NewSessionDialog as NewSessionModal };
