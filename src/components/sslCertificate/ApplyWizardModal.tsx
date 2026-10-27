import { CheckCircle2, Copy, Loader2, RefreshCw } from "lucide-react";

import { CustomSelect } from "@/components/common/CustomSelect";
import type { SslCertificateController } from "./hooks/useSslCertificate";

export function ApplyWizardModal({ controller }: { controller: SslCertificateController }) {
  const {
    setShowApplyModal,
    applyStep,
    primaryDomain,
    setPrimaryDomain,
    sanDomainsInput,
    setSanDomainsInput,
    keyType,
    setKeyType,
    emailInput,
    setEmailInput,
    verifyMode,
    setVerifyMode,
    cfToken,
    setCfToken,
    useStaging,
    setUseStaging,
    isCreatingOrder,
    handleStartApply,
    challenges,
    dohChecking,
    handleDohCheck,
    copy,
    pushToast,
    dohResolved,
    isVerifyingOrder,
    handleVerifyAndFinalize,
    setApplyStep,
  } = controller;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
      <div className="w-full max-w-2xl overflow-hidden rounded-2xl border border-slate-200 bg-white shadow-2xl dark:border-slate-800 dark:bg-slate-900">
        <div className="flex items-center justify-between border-b border-slate-100 px-6 py-4 dark:border-slate-800">
          <h3 className="text-base font-semibold text-slate-900 dark:text-white">
            申请免费 SSL 证书向导
          </h3>
          <button
            type="button"
            onClick={() => setShowApplyModal(false)}
            className="text-slate-400 hover:text-slate-600 dark:hover:text-slate-200"
          >
            ✕
          </button>
        </div>

        <div className="p-6">
          {applyStep === 1 ? (
            <div className="space-y-4">
              <div>
                <label className="mb-1.5 block text-xs font-medium text-slate-700 dark:text-slate-300">
                  主域名 (Primary Domain) *
                </label>
                <input
                  type="text"
                  value={primaryDomain}
                  onChange={(e) => setPrimaryDomain(e.target.value)}
                  placeholder="例如 example.com 或 *.example.com (支持泛域名)"
                  className="w-full rounded-xl border border-slate-200 bg-slate-50 px-3.5 py-2.5 text-sm text-slate-900 focus:border-emerald-500 focus:bg-white focus:outline-none dark:border-slate-700 dark:bg-slate-800 dark:text-white"
                />
              </div>

              <div>
                <label className="mb-1.5 block text-xs font-medium text-slate-700 dark:text-slate-300">
                  附加 SAN 域名列表 (可选，逗号或换行分隔)
                </label>
                <textarea
                  rows={2}
                  value={sanDomainsInput}
                  onChange={(e) => setSanDomainsInput(e.target.value)}
                  placeholder="www.example.com, api.example.com"
                  className="w-full rounded-xl border border-slate-200 bg-slate-50 px-3.5 py-2 text-sm text-slate-900 focus:border-emerald-500 focus:bg-white focus:outline-none dark:border-slate-700 dark:bg-slate-800 dark:text-white"
                />
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="mb-1.5 block text-xs font-medium text-slate-700 dark:text-slate-300">
                    私钥算法 (Key Type)
                  </label>
                  <CustomSelect
                    value={keyType}
                    onChange={(val) => setKeyType(val)}
                    ariaLabel="私钥算法 (Key Type)"
                    options={[
                      {
                        value: "ecc_p256",
                        label: "ECC P-256 (推荐，性能极高)",
                      },
                      {
                        value: "rsa2048",
                        label: "RSA 2048 (传统兼容性好)",
                      },
                    ]}
                  />
                </div>

                <div>
                  <label className="mb-1.5 block text-xs font-medium text-slate-700 dark:text-slate-300">
                    通知邮箱 (可选)
                  </label>
                  <input
                    type="email"
                    value={emailInput}
                    onChange={(e) => setEmailInput(e.target.value)}
                    placeholder="admin@example.com"
                    className="w-full rounded-xl border border-slate-200 bg-slate-50 px-3.5 py-2.5 text-sm text-slate-900 dark:border-slate-700 dark:bg-slate-800 dark:text-white"
                  />
                </div>
              </div>

              <div>
                <label className="mb-1.5 block text-xs font-medium text-slate-700 dark:text-slate-300">
                  验证模式
                </label>
                <div className="grid grid-cols-2 gap-3">
                  <button
                    type="button"
                    onClick={() => setVerifyMode("manual_dns")}
                    className={`rounded-xl border p-3 text-left transition-all ${
                      verifyMode === "manual_dns"
                        ? "border-emerald-500 bg-emerald-50/50 text-emerald-900 dark:bg-emerald-950/30 dark:text-emerald-200"
                        : "border-slate-200 bg-slate-50 text-slate-700 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300"
                    }`}
                  >
                    <div className="text-xs font-semibold">手动 DNS-01 验证</div>
                    <div className="mt-1 text-[11px] opacity-70">
                      零门槛，任何域名注册商均适用，展示 TXT 记录手动添加
                    </div>
                  </button>

                  <button
                    type="button"
                    onClick={() => setVerifyMode("cloudflare")}
                    className={`rounded-xl border p-3 text-left transition-all ${
                      verifyMode === "cloudflare"
                        ? "border-emerald-500 bg-emerald-50/50 text-emerald-900 dark:bg-emerald-950/30 dark:text-emerald-200"
                        : "border-slate-200 bg-slate-50 text-slate-700 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300"
                    }`}
                  >
                    <div className="text-xs font-semibold">Cloudflare DNS API</div>
                    <div className="mt-1 text-[11px] opacity-70">
                      全自动添加并清理 TXT 解析，适合在 CF 托管的域名
                    </div>
                  </button>
                </div>
              </div>

              {verifyMode === "cloudflare" && (
                <div>
                  <label className="mb-1.5 block text-xs font-medium text-slate-700 dark:text-slate-300">
                    Cloudflare API Token
                  </label>
                  <input
                    type="password"
                    value={cfToken}
                    onChange={(e) => setCfToken(e.target.value)}
                    placeholder="输入具备 Zone:DNS:Edit 权限的 Token"
                    className="w-full rounded-xl border border-slate-200 bg-slate-50 px-3.5 py-2.5 text-sm text-slate-900 dark:border-slate-700 dark:bg-slate-800 dark:text-white"
                  />
                </div>
              )}

              <div className="flex items-center gap-2 pt-2">
                <input
                  type="checkbox"
                  id="useStaging"
                  checked={useStaging}
                  onChange={(e) => setUseStaging(e.target.checked)}
                  className="rounded text-emerald-600 focus:ring-emerald-500"
                />
                <label htmlFor="useStaging" className="text-xs text-slate-600 dark:text-slate-400">
                  使用 Let's Encrypt Staging 测试环境 (用于初次测试，免触碰生产限流)
                </label>
              </div>

              <div className="mt-6 flex justify-end gap-3">
                <button
                  type="button"
                  onClick={() => setShowApplyModal(false)}
                  className="rounded-xl border border-slate-200 px-4 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300"
                >
                  取消
                </button>
                <button
                  type="button"
                  disabled={isCreatingOrder}
                  onClick={handleStartApply}
                  className="flex items-center gap-2 rounded-xl bg-emerald-600 px-5 py-2 text-xs font-medium text-white hover:bg-emerald-700 disabled:opacity-60"
                >
                  {isCreatingOrder && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
                  下一步：获取验证要求
                </button>
              </div>
            </div>
          ) : (
            /* Step 2: DNS Challenges & Finalize */
            <div className="space-y-4">
              <div className="rounded-xl bg-amber-50 p-3 text-xs text-amber-800 dark:bg-amber-950/40 dark:text-amber-200">
                请在您的域名服务商控制台（如阿里云、腾讯云、Cloudflare、GoDaddy）添加以下 TXT 解析记录：
              </div>

              <div className="space-y-3">
                {challenges.map((ch, idx) => (
                  <div
                    key={idx}
                    className="rounded-xl border border-slate-200 bg-slate-50/70 p-4 dark:border-slate-800 dark:bg-slate-800/40"
                  >
                    <div className="mb-2 flex items-center justify-between">
                      <span className="text-xs font-bold text-slate-800 dark:text-slate-200">
                        针对域名: {ch.domain}
                      </span>
                      <button
                        type="button"
                        disabled={dohChecking[ch.recordName]}
                        onClick={() => handleDohCheck(ch)}
                        className="flex items-center gap-1 rounded-md border border-slate-300 bg-white px-2.5 py-1 text-[11px] font-medium text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200"
                      >
                        <RefreshCw className={`h-3 w-3 ${dohChecking[ch.recordName] ? "animate-spin" : ""}`} />
                        公网 DoH 预检
                      </button>
                    </div>

                    <div className="grid grid-cols-1 gap-2 text-xs sm:grid-cols-3">
                      <div className="rounded-lg bg-white p-2.5 shadow-sm dark:bg-slate-800">
                        <div className="text-[10px] text-slate-400">记录类型</div>
                        <div className="mt-0.5 font-mono font-semibold text-slate-800 dark:text-slate-200">
                          {ch.recordType}
                        </div>
                      </div>
                      <div className="relative rounded-lg bg-white p-2.5 shadow-sm dark:bg-slate-800">
                        <div className="text-[10px] text-slate-400">主机记录 (Host / Name)</div>
                        <div className="mt-0.5 font-mono text-slate-800 dark:text-slate-200">
                          {ch.recordName}
                        </div>
                        <button
                          type="button"
                          onClick={() => {
                            copy(ch.recordName);
                            pushToast({ title: "已复制主机记录", kind: "success" });
                          }}
                          className="absolute right-2 top-2 text-slate-400 hover:text-emerald-600"
                          title="复制主机记录"
                        >
                          <Copy className="h-3.5 w-3.5" />
                        </button>
                      </div>
                      <div className="relative rounded-lg bg-white p-2.5 shadow-sm dark:bg-slate-800">
                        <div className="text-[10px] text-slate-400">记录值 (Value)</div>
                        <div className="mt-0.5 truncate font-mono text-slate-800 dark:text-slate-200" title={ch.recordValue}>
                          {ch.recordValue}
                        </div>
                        <button
                          type="button"
                          onClick={() => {
                            copy(ch.recordValue);
                            pushToast({ title: "已复制记录值", kind: "success" });
                          }}
                          className="absolute right-2 top-2 text-slate-400 hover:text-emerald-600"
                          title="复制记录值"
                        >
                          <Copy className="h-3.5 w-3.5" />
                        </button>
                      </div>
                    </div>

                    {dohResolved[ch.recordName] && (
                      <div className="mt-2 flex items-center gap-1.5 text-xs text-emerald-600 dark:text-emerald-400">
                        <CheckCircle2 className="h-3.5 w-3.5" />
                        DoH 探测验证通过，公网 DNS 解析已生效！
                      </div>
                    )}
                  </div>
                ))}
              </div>

              <div className="mt-6 flex justify-between gap-3 border-t border-slate-100 pt-4 dark:border-slate-800">
                <button
                  type="button"
                  onClick={() => setApplyStep(1)}
                  className="rounded-xl border border-slate-200 px-4 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300"
                >
                  上一步
                </button>
                <button
                  type="button"
                  disabled={isVerifyingOrder}
                  onClick={handleVerifyAndFinalize}
                  className="flex items-center gap-2 rounded-xl bg-emerald-600 px-5 py-2 text-xs font-medium text-white shadow-sm hover:bg-emerald-700 disabled:opacity-60"
                >
                  {isVerifyingOrder && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
                  已添加解析，验证并签发证书
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
