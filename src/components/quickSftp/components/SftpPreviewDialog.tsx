import { X } from "lucide-react";
import type { SftpPreviewFile } from "../types";

type SftpPreviewDialogProps = {
  previewFile: SftpPreviewFile;
  onClose: () => void;
};

export function SftpPreviewDialog({
  previewFile,
  onClose,
}: SftpPreviewDialogProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div className="w-full max-w-3xl rounded-lg border bg-card p-4 shadow-2xl space-y-3">
        <div className="flex items-center justify-between border-b pb-2">
          <span className="font-mono text-xs font-semibold">
            {previewFile.name}
          </span>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-1 text-muted-foreground hover:bg-muted"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
        <pre className="max-h-96 overflow-auto rounded-md bg-muted/60 p-3 font-mono text-xs leading-5">
          {previewFile.content}
        </pre>
      </div>
    </div>
  );
}
