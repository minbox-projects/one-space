import type { TFunction } from "i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../ui/dialog";

type EditProfileDialogProps = {
  t: TFunction;
  isEditDialogOpen: boolean;
  setIsEditDialogOpen: (open: boolean) => void;
  editProfileName: string;
  setEditProfileName: (name: string) => void;
  setEditError: (error: string | null) => void;
  editError: string | null;
  handleEditProfileSubmit: () => void;
};

export function EditProfileDialog({
  t,
  isEditDialogOpen,
  setIsEditDialogOpen,
  editProfileName,
  setEditProfileName,
  setEditError,
  editError,
  handleEditProfileSubmit,
}: EditProfileDialogProps) {
  return (
    <Dialog open={isEditDialogOpen} onOpenChange={setIsEditDialogOpen}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle>
            {t("aiWorkflow.editProfileTitle", "编辑方案名称")}
          </DialogTitle>
          <DialogDescription>
            {t(
              "aiWorkflow.editProfileDesc",
              "修改配置方案的名称。",
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
              data-testid="edit-profile-name-input"
              value={editProfileName}
              onChange={(e) => {
                setEditProfileName(e.target.value);
                setEditError(null);
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
          {editError && (
            <div
              role="alert"
              className="rounded border border-destructive/20 bg-destructive/10 p-2 text-xs text-destructive"
            >
              {editError}
            </div>
          )}
        </div>
        <DialogFooter>
          <button
            type="button"
            onClick={() => setIsEditDialogOpen(false)}
            className="rounded border border-input bg-background px-3 py-1.5 text-xs font-medium hover:bg-muted"
          >
            {t("aiWorkflow.cancel", "取消")}
          </button>
          <button
            type="button"
            data-testid="edit-profile-submit"
            onClick={handleEditProfileSubmit}
            className="rounded bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90"
          >
            {t("save", "保存")}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
