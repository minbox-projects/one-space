import { Copy, Download } from "lucide-react";

import type { SslCertificateController } from "./hooks/useSslCertificate";

export function CertificateDetailDrawer({ controller }: { controller: SslCertificateController }) {
  const {
    selectedCert,
    setSelectedCert,
    selectedCertTab,
    setSelectedCertTab,
    copy,
    pushToast,
    handleExportFiles,
  } = controller;

  if (!selectedCert) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/40 backdrop-blur-sm">
      <div className="flex h-full w-full max-w-2xl flex-col bg-white shadow-2xl dark:bg-slate-900">
        <div className="flex items-center justify-between border-b border-slate-100 px-6 py-4 dark:border-slate-800">
          <div>
            <h3 className="text-base font-semibold text-slate-900 dark:text-white">
              证书部署与配置预览
            </h3>
            <p className="text-xs text-slate-400">{selectedCert.primaryDomain}</p>
          </div>
          <button
            type="button"
            onClick={() => setSelectedCert(null)}
            className="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200"
          >
            ✕
          </button>
        </div>

        {/* Tab 选项卡 */}
        <div className="flex border-b border-slate-200 px-6 dark:border-slate-800">
          <button
            type="button"
            onClick={() => setSelectedCertTab("nginx")}
            className={`border-b-2 py-2.5 text-xs font-medium transition-all mr-6 ${
              selectedCertTab === "nginx"
                ? "border-emerald-600 text-emerald-600 dark:text-emerald-400"
                : "border-transparent text-slate-500 hover:text-slate-800 dark:text-slate-400"
            }`}
          >
            Nginx 配置模板
          </button>
          <button
            type="button"
            onClick={() => setSelectedCertTab("fullchain")}
            className={`border-b-2 py-2.5 text-xs font-medium transition-all mr-6 ${
              selectedCertTab === "fullchain"
                ? "border-emerald-600 text-emerald-600 dark:text-emerald-400"
                : "border-transparent text-slate-500 hover:text-slate-800 dark:text-slate-400"
            }`}
          >
            fullchain.pem
          </button>
          <button
            type="button"
            onClick={() => setSelectedCertTab("privkey")}
            className={`border-b-2 py-2.5 text-xs font-medium transition-all ${
              selectedCertTab === "privkey"
                ? "border-emerald-600 text-emerald-600 dark:text-emerald-400"
                : "border-transparent text-slate-500 hover:text-slate-800 dark:text-slate-400"
            }`}
          >
            privkey.pem
          </button>
        </div>

        {/* 文本内容与复制 */}
        <div className="relative flex-1 overflow-y-auto p-6 font-mono text-xs">
          <div className="absolute right-8 top-8">
            <button
              type="button"
              onClick={() => {
                const text =
                  selectedCertTab === "nginx"
                    ? selectedCert.nginxConfig
                    : selectedCertTab === "fullchain"
                    ? selectedCert.fullchainPem
                    : selectedCert.privkeyPem || "";
                copy(text);
                pushToast({ title: "已复制到剪贴板", kind: "success" });
              }}
              className="flex items-center gap-1.5 rounded-lg border border-slate-200 bg-white/90 px-3 py-1.5 text-xs font-medium text-slate-700 shadow-sm backdrop-blur hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-800/90 dark:text-slate-200"
            >
              <Copy className="h-3.5 w-3.5" />
              复制全文
            </button>
          </div>

          <pre className="rounded-xl bg-slate-900 p-4 text-slate-100 overflow-x-auto">
            {selectedCertTab === "nginx" && selectedCert.nginxConfig}
            {selectedCertTab === "fullchain" && selectedCert.fullchainPem}
            {selectedCertTab === "privkey" && (selectedCert.privkeyPem || "(私钥未解密或未包含)")}
          </pre>
        </div>

        {/* 底部操作 */}
        <div className="flex items-center justify-between border-t border-slate-100 p-4 dark:border-slate-800">
          <div className="text-xs text-slate-500">
            支持直接将 Nginx 文件包导出到任意目录
          </div>
          <button
            type="button"
            onClick={() => handleExportFiles(selectedCert.id)}
            className="flex items-center gap-2 rounded-xl bg-emerald-600 px-4 py-2 text-xs font-medium text-white hover:bg-emerald-700"
          >
            <Download className="h-3.5 w-3.5" />
            导出整套文件包
          </button>
        </div>
      </div>
    </div>
  );
}
