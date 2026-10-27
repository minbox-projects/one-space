import { Download, FileCode2, Plus, Trash2 } from "lucide-react";

import type { SslCertificateController } from "./hooks/useSslCertificate";

export function IssuePanel({ controller }: { controller: SslCertificateController }) {
  const {
    issuedCerts,
    setApplyStep,
    setShowApplyModal,
    setSelectedCert,
    setSelectedCertTab,
    handleExportFiles,
    handleDeleteIssued,
  } = controller;

  return (
    <div className="mx-auto max-w-5xl space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-base font-semibold text-slate-900 dark:text-white">
            免费申请与本地部署管理
          </h2>
          <p className="text-xs text-slate-500">
            基于 ACME (Let's Encrypt) 协议申请免费证书，一键导出契合 Nginx 标准规范的文件
          </p>
        </div>

        <button
          type="button"
          onClick={() => {
            setApplyStep(1);
            setShowApplyModal(true);
          }}
          className="flex items-center gap-2 rounded-xl bg-emerald-600 px-4 py-2.5 text-xs font-medium text-white shadow-sm hover:bg-emerald-700"
        >
          <Plus className="h-4 w-4" />
          申请新证书
        </button>
      </div>

      {/* 已签发证书列表 */}
      <div className="rounded-2xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <h3 className="mb-4 text-sm font-semibold text-slate-900 dark:text-white">
          本地已申请证书库 ({issuedCerts.length})
        </h3>

        {issuedCerts.length === 0 ? (
          <div className="py-16 text-center text-xs text-slate-400">
            暂无申请的证书记录，点击右上角“申请新证书”开始免费创建
          </div>
        ) : (
          <div className="space-y-3">
            {issuedCerts.map((cert) => (
              <div
                key={cert.id}
                className="flex flex-wrap items-center justify-between gap-4 rounded-xl border border-slate-200 p-4 transition-all hover:border-emerald-300 dark:border-slate-800 dark:bg-slate-800/40"
              >
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-semibold text-slate-900 dark:text-white">
                      {cert.primaryDomain}
                    </span>
                    <span className="rounded bg-emerald-100 px-2 py-0.5 text-[10px] font-medium text-emerald-800 dark:bg-emerald-900/40 dark:text-emerald-300">
                      {cert.keyType}
                    </span>
                    <span className="rounded bg-slate-100 px-2 py-0.5 text-[10px] text-slate-600 dark:bg-slate-700 dark:text-slate-300">
                      剩余 {cert.daysRemaining} 天
                    </span>
                  </div>
                  <div className="mt-1 text-xs text-slate-400">
                    颁发机构: {cert.issuer} · 到期时间: {new Date(cert.expiresAt * 1000).toLocaleDateString()}
                    {cert.sanDomains.length > 0 && ` · 包含备用域名: ${cert.sanDomains.join(", ")}`}
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => {
                      setSelectedCert(cert);
                      setSelectedCertTab("nginx");
                    }}
                    className="flex items-center gap-1 rounded-lg border border-slate-200 px-3 py-1.5 text-xs font-medium text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
                  >
                    <FileCode2 className="h-3.5 w-3.5" />
                    查看与复制配置
                  </button>
                  <button
                    type="button"
                    onClick={() => handleExportFiles(cert.id)}
                    className="flex items-center gap-1 rounded-lg bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
                  >
                    <Download className="h-3.5 w-3.5" />
                    导出 Nginx 文件包
                  </button>
                  <button
                    type="button"
                    onClick={() => handleDeleteIssued(cert.id, cert.primaryDomain)}
                    className="rounded-lg p-1.5 text-slate-400 hover:bg-rose-50 hover:text-rose-600 dark:hover:bg-rose-950/40"
                    title="删除记录"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
