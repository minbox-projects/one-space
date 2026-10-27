import type { TFunction } from "i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../ui/dialog";

type CreateProfileDialogProps = {
  t: TFunction;
  isCreateDialogOpen: boolean;
  setIsCreateDialogOpen: (open: boolean) => void;
  newProfileName: string;
  setNewProfileName: (name: string) => void;
  setCreateError: (error: string | null) => void;
  createSourceMode: "clone" | "blank";
  setCreateSourceMode: (mode: "clone" | "blank") => void;
  selectedProfile: string | null;
  createError: string | null;
  handleCreateProfileSubmit: () => void;
};

export function CreateProfileDialog({
  t,
  isCreateDialogOpen,
  setIsCreateDialogOpen,
  newProfileName,
  setNewProfileName,
  setCreateError,
  createSourceMode,
  setCreateSourceMode,
  selectedProfile,
  createError,
  handleCreateProfileSubmit,
}: CreateProfileDialogProps) {
  return (
    <Dialog open={isCreateDialogOpen} onOpenChange={setIsCreateDialogOpen}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>
            {t("aiWorkflow.createProfileTitle", "新建配置方案")}
          </DialogTitle>
          <DialogDescription>
            {t(
              "aiWorkflow.createProfileDesc",
              "为 AI Workflow subagent 矩阵创建一套新的模型与推理强度配置方案。",
            )}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4 py-2 text-sm">
          <div>
            <label className="mb-1 block text-xs font-medium text-muted-foreground">
              {t("aiWorkflow.profileName", "方案名称")}
            </label>
            <input
              type="text"
              data-testid="create-profile-name-input"
              value={newProfileName}
              onChange={(e) => {
                setNewProfileName(e.target.value);
                setCreateError(null);
              }}
              placeholder={t(
                "aiWorkflow.profileNamePlaceholder",
                "例如 my-new-profile",
              )}
              className="h-8 w-full rounded border bg-background px-2.5 font-mono text-xs focus:outline-none focus:ring-1 focus:ring-primary"
            />
            <p className="mt-1 text-[11px] text-muted-foreground">
              {t("aiWorkflow.profileNameHelp", "仅支持字母、数字、短横线与点")}
            </p>
          </div>
          <div>
            <label className="mb-1.5 block text-xs font-medium text-muted-foreground">
              {t("aiWorkflow.initialConfigFrom", "初始配置来源")}
            </label>
            <div className="space-y-2">
              <label className="flex cursor-pointer items-center gap-2 text-xs">
                <input
                  type="radio"
                  name="cloneSource"
                  data-testid="create-profile-radio-clone"
                  checked={createSourceMode === "clone"}
                  onChange={() => setCreateSourceMode("clone")}
                />
                <span>
                  {t("aiWorkflow.cloneCurrent", {
                    name: selectedProfile || "当前方案",
                    defaultValue: `从当前方案复制 (${selectedProfile || "当前方案"})`,
                  })}
                </span>
              </label>
              <label className="flex cursor-pointer items-center gap-2 text-xs">
                <input
                  type="radio"
                  name="cloneSource"
                  data-testid="create-profile-radio-blank"
                  checked={createSourceMode === "blank"}
                  onChange={() => setCreateSourceMode("blank")}
                />
                <span>{t("aiWorkflow.blankProfile", "创建空白方案")}</span>
              </label>
            </div>
          </div>
          {createError && (
            <div
              role="alert"
              className="rounded border border-destructive/20 bg-destructive/10 p-2 text-xs text-destructive"
            >
              {createError}
            </div>
          )}
        </div>
        <DialogFooter>
          <button
            type="button"
            onClick={() => setIsCreateDialogOpen(false)}
            className="rounded border border-input bg-background px-3 py-1.5 text-xs font-medium hover:bg-muted"
          >
            {t("aiWorkflow.cancel", "取消")}
          </button>
          <button
            type="button"
            data-testid="create-profile-submit"
            onClick={handleCreateProfileSubmit}
            className="rounded bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90"
          >
            {t("aiWorkflow.create", "创建")}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
