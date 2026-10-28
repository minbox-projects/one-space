import { useState } from "react";
import { X, Sparkles, GitBranch, FolderGit2, Shield, Loader2, Terminal } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import type { TFunction } from "i18next";
import type { CloudAgentSettings } from "@/lib/cloudAgent";

interface NewSessionModalProps {
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

export function NewSessionModal({
  t,
  settings,
  isOpen,
  onClose,
  onSubmit,
}: NewSessionModalProps) {
  const [repo, setRepo] = useState("");
  const [branch, setBranch] = useState("");
  const [useYolo, setUseYolo] = useState(settings.opencodeYoloMode);
  const [extraArgs, setExtraArgs] = useState(settings.opencodeExtraArgs);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!isOpen) return null;

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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4">
      <div className="relative w-full max-w-lg rounded-2xl border border-slate-200 bg-white p-6 shadow-xl dark:border-slate-800 dark:bg-slate-900 animate-in fade-in zoom-in-95 duration-200">
        {/* 关闭按钮 */}
        <button
          type="button"
          onClick={onClose}
          className="absolute right-4 top-4 rounded-xl p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600 dark:hover:bg-slate-800 dark:hover:text-slate-200 transition-colors"
        >
          <X className="h-4 w-4" />
        </button>

        {/* 头部标题 */}
        <div className="flex items-center gap-3 mb-5">
          <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400">
            <Sparkles className="h-5 w-5" />
          </div>
          <div>
            <h3 className="text-base font-semibold text-slate-900 dark:text-white">
              {t("createNewCloudTaskTitle", "新建云端特性任务")}
            </h3>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              {t(
                "createNewCloudTaskDesc",
                "在云端创建独立的 Git Worktree 隔离开发环境并调起本地终端直连。"
              )}
            </p>
          </div>
        </div>

        {error && (
          <div className="mb-4 rounded-xl bg-rose-50 p-3 text-xs text-rose-600 dark:bg-rose-950/40 dark:text-rose-400">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          {/* 目标仓库 */}
          <div className="space-y-1.5">
            <label className="text-xs font-semibold text-slate-700 dark:text-slate-300 flex items-center gap-1.5">
              <FolderGit2 className="h-3.5 w-3.5 text-slate-400" />
              {t("targetRepoLabel", "目标仓库或项目路径")}
              <span className="text-rose-500">*</span>
            </label>
            <input
              type="text"
              value={repo}
              onChange={(e) => setRepo(e.target.value)}
              placeholder="例如: one-space 或 https://github.com/user/repo.git"
              className="w-full rounded-xl border border-slate-200 bg-slate-50/50 py-2.5 px-3 text-sm text-slate-900 placeholder:text-slate-400 focus:border-emerald-500 focus:bg-white focus:outline-none focus:ring-2 focus:ring-emerald-500/20 dark:border-slate-700 dark:bg-slate-800/50 dark:text-white dark:focus:bg-slate-800"
            />
          </div>

          {/* 特性分支名 */}
          <div className="space-y-1.5">
            <label className="text-xs font-semibold text-slate-700 dark:text-slate-300 flex items-center gap-1.5">
              <GitBranch className="h-3.5 w-3.5 text-slate-400" />
              {t("featureBranchLabel", "特性分支名称 (选填，留空则使用仓库主目录)")}
            </label>
            <input
              type="text"
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
              placeholder="例如: feat-login 或 fix-payment"
              className="w-full rounded-xl border border-slate-200 bg-slate-50/50 py-2.5 px-3 text-sm font-mono text-slate-900 placeholder:text-slate-400 focus:border-emerald-500 focus:bg-white focus:outline-none focus:ring-2 focus:ring-emerald-500/20 dark:border-slate-700 dark:bg-slate-800/50 dark:text-white dark:focus:bg-slate-800"
            />
          </div>

          {/* 授权模式 Switch */}
          <div className="rounded-xl border border-slate-200/80 bg-slate-50/50 p-3.5 dark:border-slate-800 dark:bg-slate-800/40 flex items-center justify-between">
            <div className="space-y-0.5">
              <div className="text-xs font-semibold text-slate-800 dark:text-slate-200 flex items-center gap-1.5">
                <Shield className="h-3.5 w-3.5 text-emerald-500" />
                {t("yoloModeLabel", "完全自动授权模式 (--yolo)")}
              </div>
              <p className="text-[11px] text-slate-500 dark:text-slate-400">
                {t("yoloModeHint", "开启后 OpenCode 在云端会话无需交互式权限确认")}
              </p>
            </div>
            <Switch checked={useYolo} onCheckedChange={setUseYolo} />
          </div>

          {/* 额外启动参数 */}
          <div className="space-y-1.5">
            <label className="text-xs font-semibold text-slate-700 dark:text-slate-300 flex items-center gap-1.5">
              <Terminal className="h-3.5 w-3.5 text-slate-400" />
              {t("extraArgsLabel", "CLI 额外启动参数 (选填)")}
            </label>
            <input
              type="text"
              value={extraArgs}
              onChange={(e) => setExtraArgs(e.target.value)}
              placeholder="例如: --model cloud-coder"
              className="w-full rounded-xl border border-slate-200 bg-slate-50/50 py-2 px-3 text-xs font-mono text-slate-900 placeholder:text-slate-400 focus:border-emerald-500 focus:bg-white focus:outline-none focus:ring-2 focus:ring-emerald-500/20 dark:border-slate-700 dark:bg-slate-800/50 dark:text-white dark:focus:bg-slate-800"
            />
          </div>

          {/* 底部按钮 */}
          <div className="flex items-center justify-end gap-2 pt-2">
            <button
              type="button"
              disabled={isSubmitting}
              onClick={onClose}
              className="rounded-xl border border-slate-200 px-4 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-800 transition-colors"
            >
              {t("cancel", "取消")}
            </button>
            <button
              type="submit"
              disabled={isSubmitting}
              className="inline-flex items-center gap-1.5 rounded-xl bg-emerald-600 px-4 py-2 text-xs font-medium text-white shadow-sm hover:bg-emerald-700 transition-all disabled:opacity-60"
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
      </div>
    </div>
  );
}
