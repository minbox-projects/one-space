import { Copy, Check, QrCode } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";

interface QrCodeModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  url: string;
  tunnelName: string;
}

export function QrCodeModal({ open, onOpenChange, url, tunnelName }: QrCodeModalProps) {
  const { t } = useTranslation();
  const { copied, copy } = useCopyToClipboard();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <div className="flex items-center gap-2">
            <div className="rounded-md bg-emerald-500/10 p-2 text-emerald-600">
              <QrCode className="h-5 w-5" />
            </div>
            <div>
              <DialogTitle className="text-base font-semibold">
                {t("portTunnelQrTitle", "Scan with Mobile Camera")}
              </DialogTitle>
              <DialogDescription className="text-xs text-muted-foreground">
                {tunnelName} • {t("portTunnelQrSubtitle", "Access your local service through the public internet from any device.")}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        <div className="flex flex-col items-center justify-center p-4">
          <div className="rounded-xl border border-border/80 bg-white p-4 shadow-sm">
            <QRCodeSVG
              value={url}
              size={200}
              level="M"
              includeMargin={false}
            />
          </div>
          <p className="mt-3 max-w-xs break-all text-center font-mono text-xs text-muted-foreground">
            {url}
          </p>
        </div>

        <DialogFooter className="flex sm:justify-between items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => copy(url)}
            className="flex items-center gap-1.5"
          >
            {copied ? (
              <>
                <Check className="h-4 w-4 text-emerald-500" />
                <span>{t("portTunnelCopied", "Copied")}</span>
              </>
            ) : (
              <>
                <Copy className="h-4 w-4" />
                <span>{t("portTunnelCopyUrl", "Copy URL")}</span>
              </>
            )}
          </Button>
          <Button variant="default" size="sm" onClick={() => onOpenChange(false)}>
            {t("portTunnelClose", "Close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
