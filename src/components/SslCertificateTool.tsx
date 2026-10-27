import { Download, Search, ShieldCheck } from "lucide-react";

import { useToast } from "./ToastProvider";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import { ApplyWizardModal } from "./sslCertificate/ApplyWizardModal";
import { CertificateDetailDrawer } from "./sslCertificate/CertificateDetailDrawer";
import { CheckPanel } from "./sslCertificate/CheckPanel";
import { IssuePanel } from "./sslCertificate/IssuePanel";
import { useSslCertificate } from "./sslCertificate/hooks/useSslCertificate";

export function SslCertificateTool() {
  const { pushToast } = useToast();
  const { copy } = useCopyToClipboard();
  const controller = useSslCertificate(pushToast, copy);
  const { activeTab, setActiveTab, watchlist, issuedCerts } = controller;

  return (
    <div className="flex h-full flex-col bg-slate-50 text-slate-800 dark:bg-slate-950 dark:text-slate-100">
      {/* 顶部主工具栏 */}
      <div className="border-b border-slate-200/80 bg-white/70 px-6 py-4 backdrop-blur-md dark:border-slate-800/80 dark:bg-slate-900/70">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-emerald-500/10 text-emerald-600 shadow-sm dark:bg-emerald-500/20 dark:text-emerald-400">
              <ShieldCheck className="h-6 w-6" />
            </div>
            <div>
              <h1 className="text-lg font-semibold tracking-tight text-slate-900 dark:text-white">
                SSL 证书助手
              </h1>
              <p className="text-xs text-slate-500 dark:text-slate-400">
                免费申请与 Nginx 部署 · TLS 握手健康检测 · 证书链分析与到期监控
              </p>
            </div>
          </div>

          {/* 模式切换 Tab */}
          <div className="flex rounded-xl bg-slate-100 p-1 dark:bg-slate-800">
            <button
              type="button"
              onClick={() => setActiveTab("check")}
              className={`flex items-center gap-2 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
                activeTab === "check"
                  ? "bg-white text-emerald-600 shadow-sm dark:bg-slate-700 dark:text-emerald-400"
                  : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
              }`}
            >
              <Search className="h-3.5 w-3.5" />
              健康检测与监控
              {watchlist.length > 0 && (
                <span className="ml-1 rounded-full bg-emerald-500/10 px-1.5 py-0.2 text-[10px] font-semibold text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400">
                  {watchlist.length}
                </span>
              )}
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("issue")}
              className={`flex items-center gap-2 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
                activeTab === "issue"
                  ? "bg-white text-emerald-600 shadow-sm dark:bg-slate-700 dark:text-emerald-400"
                  : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
              }`}
            >
              <Download className="h-3.5 w-3.5" />
              免费申请与 Nginx 部署
              {issuedCerts.length > 0 && (
                <span className="ml-1 rounded-full bg-emerald-500/10 px-1.5 py-0.2 text-[10px] font-semibold text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400">
                  {issuedCerts.length}
                </span>
              )}
            </button>
          </div>
        </div>
      </div>

      {/* 主工作区 */}
      <div className="flex-1 overflow-y-auto p-6">
        {activeTab === "check" ? (
          <CheckPanel controller={controller} />
        ) : (
          <IssuePanel controller={controller} />
        )}
      </div>

      {/* 申请向导弹窗 (Wizard Modal) */}
      {controller.showApplyModal && (
        <ApplyWizardModal controller={controller} />
      )}

      {/* 证书详情与 Nginx 配置预览抽屉 (Detail Drawer) */}
      {controller.selectedCert && (
        <CertificateDetailDrawer controller={controller} />
      )}
    </div>
  );
}
