import {
  AlertCircle,
  AlertTriangle,
  Bell,
  CheckCircle2,
  Clock,
  Globe,
  History,
  Layers,
  Loader2,
  RefreshCw,
  Search,
  ShieldCheck,
  Star,
  Trash2,
} from "lucide-react";

import type { SslCertificateController } from "./hooks/useSslCertificate";

export function CheckPanel({ controller }: { controller: SslCertificateController }) {
  const {
    checkDomain,
    setCheckDomain,
    checkPort,
    handleCheck,
    isChecking,
    historyDomains,
    handleClearHistory,
    checkError,
    checkResult,
    handleAddToWatchlist,
    isDomainWatched,
    setPrimaryDomain,
    setActiveTab,
    setShowApplyModal,
    watchlist,
    sslConfig,
    isCheckingAllWatchlist,
    handleCheckAllWatchlist,
    setCheckPort,
    handleRemoveWatch,
  } = controller;

  return (
    <div className="mx-auto max-w-5xl space-y-6">
      {/* 快速即时检测输入框 */}
      <div className="rounded-2xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <h2 className="mb-3 text-sm font-semibold text-slate-800 dark:text-slate-200">
          SSL/TLS 握手实时检测
        </h2>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            handleCheck();
          }}
          className="flex flex-wrap items-center gap-3"
        >
          <div className="relative min-w-[260px] flex-1">
            <Globe className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-400" />
            <input
              type="text"
              value={checkDomain}
              onChange={(e) => setCheckDomain(e.target.value)}
              placeholder="输入域名，例如 github.com 或 api.yourdomain.com"
              className="w-full rounded-xl border border-slate-200 bg-slate-50/50 py-2.5 pl-9 pr-4 text-sm text-slate-900 placeholder:text-slate-400 focus:border-emerald-500 focus:bg-white focus:outline-none focus:ring-2 focus:ring-emerald-500/20 dark:border-slate-700 dark:bg-slate-800/50 dark:text-white dark:focus:bg-slate-800"
            />
          </div>
          <div className="w-24">
            <input
              type="text"
              value={checkPort}
              onChange={(e) => setCheckPort(e.target.value)}
              placeholder="443"
              className="w-full rounded-xl border border-slate-200 bg-slate-50/50 py-2.5 px-3 text-center text-sm text-slate-900 focus:border-emerald-500 focus:bg-white focus:outline-none focus:ring-2 focus:ring-emerald-500/20 dark:border-slate-700 dark:bg-slate-800/50 dark:text-white dark:focus:bg-slate-800"
            />
          </div>
          <button
            type="submit"
            disabled={isChecking}
            className="flex items-center gap-2 rounded-xl bg-emerald-600 px-5 py-2.5 text-sm font-medium text-white shadow-sm hover:bg-emerald-700 focus:outline-none focus:ring-2 focus:ring-emerald-500/30 disabled:opacity-60"
          >
            {isChecking ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                正在建立握手...
              </>
            ) : (
              <>
                <Search className="h-4 w-4" />
                立即检测
              </>
            )}
          </button>
        </form>

        {/* 最近检测历史域名列表 */}
        {historyDomains.length > 0 ? (
          <div
            data-testid="ssl-check-history-list"
            className="mt-3 flex flex-wrap items-center gap-1.5 text-xs text-slate-500 dark:text-slate-400"
          >
            <span className="flex items-center gap-1 font-medium text-slate-600 dark:text-slate-300">
              <History className="h-3.5 w-3.5 text-slate-400" />
              最近检测:
            </span>
            {historyDomains.map((d) => (
              <button
                key={d}
                type="button"
                data-testid={`ssl-history-item-${d}`}
                onClick={() => {
                  setCheckDomain(d);
                  handleCheck(d);
                }}
                className="inline-flex items-center rounded-md border border-slate-200/80 bg-slate-50 px-2 py-0.5 text-xs font-mono text-slate-700 transition hover:border-emerald-500/50 hover:bg-emerald-50 hover:text-emerald-700 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200 dark:hover:border-emerald-500/50 dark:hover:bg-emerald-950/40 dark:hover:text-emerald-300"
                title={`再次检测 ${d}`}
              >
                {d}
              </button>
            ))}
            <button
              type="button"
              data-testid="ssl-history-clear"
              onClick={handleClearHistory}
              title="清空检测历史"
              className="ml-1 inline-flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[11px] text-slate-400 transition hover:bg-slate-100 hover:text-rose-500 dark:hover:bg-slate-800 dark:hover:text-rose-400"
            >
              <Trash2 className="h-3 w-3" />
              清空
            </button>
          </div>
        ) : (
          /* 快捷示例 */
          <div className="mt-3 flex items-center gap-2 text-xs text-slate-500 dark:text-slate-400">
            <span>快速试用:</span>
            {["github.com", "cloudflare.com", "apple.com"].map((d) => (
              <button
                key={d}
                type="button"
                onClick={() => {
                  setCheckDomain(d);
                  handleCheck(d);
                }}
                className="rounded-md border border-slate-200 bg-slate-50 px-2 py-0.5 hover:bg-slate-100 hover:text-emerald-600 dark:border-slate-700 dark:bg-slate-800 dark:hover:bg-slate-700"
              >
                {d}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* 检测结果展现 */}
      {checkError && (
        <div className="flex items-start gap-3 rounded-2xl border border-rose-200 bg-rose-50/70 p-4 text-rose-800 dark:border-rose-900/50 dark:bg-rose-950/30 dark:text-rose-300">
          <AlertCircle className="mt-0.5 h-5 w-5 shrink-0 text-rose-500" />
          <div>
            <h3 className="text-sm font-semibold">连接或握手异常</h3>
            <p className="mt-1 text-xs opacity-90">{checkError}</p>
          </div>
        </div>
      )}

      {checkResult && (
        <div className="space-y-4">
          {/* 概览大卡片 */}
          <div className="relative overflow-hidden rounded-2xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <div className="flex items-center gap-2.5">
                  <span
                    className={`flex h-3 w-3 rounded-full ${
                      checkResult.isExpired
                        ? "bg-rose-500 ring-4 ring-rose-500/20"
                        : checkResult.isExpiringSoon
                        ? "bg-amber-500 ring-4 ring-amber-500/20"
                        : "bg-emerald-500 ring-4 ring-emerald-500/20"
                    }`}
                  />
                  <h3 className="text-xl font-bold tracking-tight text-slate-900 dark:text-white">
                    {checkResult.domain}:{checkResult.port}
                  </h3>
                  {checkResult.ipAddress && (
                    <span className="rounded-md bg-slate-100 px-2 py-0.5 text-xs font-mono text-slate-600 dark:bg-slate-800 dark:text-slate-400">
                      {checkResult.ipAddress}
                    </span>
                  )}
                  <span
                    className={`rounded-full px-2.5 py-0.5 text-xs font-semibold ${
                      checkResult.isExpired
                        ? "bg-rose-100 text-rose-700 dark:bg-rose-900/30 dark:text-rose-400"
                        : checkResult.isExpiringSoon
                        ? "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400"
                        : "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
                    }`}
                  >
                    {checkResult.isExpired
                      ? "已过期"
                      : checkResult.isExpiringSoon
                      ? "即将过期"
                      : "健康有效"}
                  </span>
                </div>

                <p className="mt-2 text-xs text-slate-500 dark:text-slate-400">
                  颁发者:{" "}
                  <span className="font-medium text-slate-700 dark:text-slate-200">
                    {checkResult.certificates[0]?.issuerCn || checkResult.certificates[0]?.issuer || "未知"}
                  </span>
                  {checkResult.tlsVersion && (
                    <span className="ml-3">
                      协议: <span className="font-mono text-emerald-600 dark:text-emerald-400">{checkResult.tlsVersion}</span>
                    </span>
                  )}
                  {checkResult.cipherSuite && (
                    <span className="ml-3">
                      套件: <span className="font-mono text-slate-600 dark:text-slate-400">{checkResult.cipherSuite}</span>
                    </span>
                  )}
                </p>
              </div>

              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={handleAddToWatchlist}
                  className={`flex items-center gap-1.5 rounded-xl border px-3.5 py-2 text-xs font-medium shadow-sm transition-all ${
                    isDomainWatched
                      ? "border-amber-300 bg-amber-50 text-amber-700 dark:border-amber-700/50 dark:bg-amber-950/30 dark:text-amber-400"
                      : "border-slate-200 bg-white text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200 dark:hover:bg-slate-700"
                  }`}
                >
                  <Star className={`h-3.5 w-3.5 ${isDomainWatched ? "fill-amber-400 text-amber-500" : ""}`} />
                  {isDomainWatched ? "已在关注监控中" : "设为关注"}
                </button>

                <button
                  type="button"
                  onClick={() => {
                    setPrimaryDomain(checkResult.domain);
                    setActiveTab("issue");
                    setShowApplyModal(true);
                  }}
                  className="flex items-center gap-1.5 rounded-xl bg-slate-900 px-3.5 py-2 text-xs font-medium text-white shadow-sm hover:bg-slate-800 dark:bg-emerald-600 dark:hover:bg-emerald-700"
                >
                  <ShieldCheck className="h-3.5 w-3.5" />
                  申请新证书
                </button>
              </div>
            </div>

            {/* 倒计时与指标 */}
            <div className="mt-6 grid grid-cols-2 gap-4 border-t border-slate-100 pt-5 sm:grid-cols-4 dark:border-slate-800">
              <div>
                <div className="text-xs text-slate-500 dark:text-slate-400">剩余有效期</div>
                <div className="mt-1 text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
                  {checkResult.daysRemaining > 0 ? `${checkResult.daysRemaining} 天` : "0 天 (超期)"}
                </div>
              </div>
              <div>
                <div className="text-xs text-slate-500 dark:text-slate-400">生效与到期</div>
                <div className="mt-1 text-xs text-slate-700 dark:text-slate-300">
                  到期: {new Date(checkResult.certificates[0]?.notAfter * 1000).toLocaleDateString()}
                </div>
                <div className="text-[11px] text-slate-400">
                  生效: {new Date(checkResult.certificates[0]?.notBefore * 1000).toLocaleDateString()}
                </div>
              </div>
              <div>
                <div className="text-xs text-slate-500 dark:text-slate-400">域名覆盖匹配</div>
                <div className="mt-1 flex items-center gap-1.5 text-xs font-medium">
                  {checkResult.domainMatches ? (
                    <>
                      <CheckCircle2 className="h-4 w-4 text-emerald-500" />
                      <span className="text-emerald-600 dark:text-emerald-400">当前域名匹配已覆盖</span>
                    </>
                  ) : (
                    <>
                      <AlertTriangle className="h-4 w-4 text-amber-500" />
                      <span className="text-amber-600 dark:text-amber-400">当前域名未在 SAN 中</span>
                    </>
                  )}
                </div>
              </div>
              <div>
                <div className="text-xs text-slate-500 dark:text-slate-400">证书链层级</div>
                <div className="mt-1 text-xs font-medium text-slate-700 dark:text-slate-300">
                  共 {checkResult.certificates.length} 个证书节点
                </div>
              </div>
            </div>
          </div>

          {/* 证书链层级展开树 */}
          <div className="rounded-2xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-800 dark:bg-slate-900">
            <h4 className="mb-4 flex items-center gap-2 text-sm font-semibold text-slate-900 dark:text-white">
              <Layers className="h-4 w-4 text-emerald-600" />
              完整证书链分析 (Certificate Chain)
            </h4>
            <div className="space-y-3">
              {checkResult.certificates.map((cert, idx) => (
                <div
                  key={idx}
                  className="rounded-xl border border-slate-200/80 bg-slate-50/60 p-4 transition-all hover:bg-slate-50 dark:border-slate-800 dark:bg-slate-800/40 dark:hover:bg-slate-800/60"
                >
                  <div className="flex flex-wrap items-start justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <span className="flex h-5 w-5 items-center justify-center rounded-full bg-slate-200 text-xs font-semibold text-slate-700 dark:bg-slate-700 dark:text-slate-300">
                        {idx + 1}
                      </span>
                      <span className="text-xs font-bold text-slate-900 dark:text-white">
                        {idx === 0
                          ? "服务器实体证书 (End-Entity / Leaf)"
                          : cert.isRoot
                          ? "根证书 (Root CA)"
                          : "中间证书 (Intermediate CA)"}
                      </span>
                      <span className="text-xs text-slate-500">
                        CN: {cert.subjectCn || cert.subject}
                      </span>
                    </div>
                    <span className="text-[11px] font-mono text-slate-400">
                      {cert.signatureAlgorithm}
                    </span>
                  </div>

                  <div className="mt-3 grid grid-cols-1 gap-2 text-xs sm:grid-cols-2">
                    <div>
                      <span className="text-slate-400">颁发者 (Issuer): </span>
                      <span className="text-slate-700 dark:text-slate-300">
                        {cert.issuerCn || cert.issuerOrg || cert.issuer}
                      </span>
                    </div>
                    <div>
                      <span className="text-slate-400">有效期: </span>
                      <span className="text-slate-700 dark:text-slate-300">
                        {new Date(cert.notBefore * 1000).toLocaleDateString()} 至{" "}
                        {new Date(cert.notAfter * 1000).toLocaleDateString()} (
                        {cert.daysRemaining > 0 ? `剩余 ${cert.daysRemaining} 天` : "已过期"})
                      </span>
                    </div>
                    <div className="sm:col-span-2">
                      <span className="text-slate-400">SHA-256 指纹: </span>
                      <span className="font-mono text-[11px] text-slate-600 dark:text-slate-300">
                        {cert.sha256Fingerprint}
                      </span>
                    </div>
                  </div>

                  {cert.sans.length > 0 && (
                    <div className="mt-3 border-t border-slate-200/60 pt-2.5 dark:border-slate-700/60">
                      <div className="mb-1.5 text-[11px] font-medium text-slate-400">
                        备用名称列表 (SANs - 共 {cert.sans.length} 个):
                      </div>
                      <div className="flex flex-wrap gap-1.5">
                        {cert.sans.map((san, sIdx) => (
                          <span
                            key={sIdx}
                            className={`rounded-md px-2 py-0.5 text-[11px] font-mono ${
                              san === checkResult.domain ||
                              (san.startsWith("*.") && checkResult.domain.endsWith(san.slice(2)))
                                ? "bg-emerald-100 font-semibold text-emerald-800 dark:bg-emerald-900/40 dark:text-emerald-300"
                                : "bg-slate-200/70 text-slate-700 dark:bg-slate-700/70 dark:text-slate-300"
                            }`}
                          >
                            {san}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* 关注列表看板 (Watchlist Dashboard) */}
      <div className="rounded-2xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900">
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <Bell className="h-4 w-4 text-emerald-600" />
            <h3 className="text-sm font-semibold text-slate-900 dark:text-white">
              关注列表与临期告警监控
            </h3>
            <span className="rounded-full bg-slate-100 px-2 py-0.5 text-xs text-slate-500 dark:bg-slate-800">
              {watchlist.length} 个受控站点
            </span>
            {sslConfig && (
              <div className="flex items-center gap-1.5 rounded-full bg-slate-100 px-2.5 py-0.5 text-xs text-slate-600 dark:bg-slate-800 dark:text-slate-300">
                <Clock className="h-3 w-3 text-emerald-600 dark:text-emerald-400" />
                <span>
                  {sslConfig.autoCheckEnabled
                    ? `巡检: ${sslConfig.checkTimes.join(", ")} | 告警阈值: ${sslConfig.alertDaysThreshold}天`
                    : "自动巡检已禁用"}
                </span>
              </div>
            )}
          </div>

          <button
            type="button"
            disabled={isCheckingAllWatchlist || watchlist.length === 0}
            onClick={handleCheckAllWatchlist}
            className="flex items-center gap-1.5 rounded-lg border border-slate-200 bg-white px-3 py-1.5 text-xs font-medium text-slate-700 shadow-sm hover:bg-slate-50 disabled:opacity-50 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200 dark:hover:bg-slate-700"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${isCheckingAllWatchlist ? "animate-spin" : ""}`} />
            批量巡检刷新
          </button>
        </div>

        {watchlist.length === 0 ? (
          <div className="py-12 text-center text-xs text-slate-400">
            暂无关注的站点，在上方输入域名检测后点击“设为关注”，即可加入自动监控与临期告警通知
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {watchlist.map((item) => {
              const threshold = sslConfig?.alertDaysThreshold ?? item.alertDaysThreshold;
              const isExpiring = item.lastDaysRemaining !== undefined && item.lastDaysRemaining !== null && item.lastDaysRemaining <= threshold;
              const isExpired = item.lastDaysRemaining !== undefined && item.lastDaysRemaining !== null && item.lastDaysRemaining <= 0;
              const isCritical = item.lastDaysRemaining !== undefined && item.lastDaysRemaining !== null && item.lastDaysRemaining > 0 && item.lastDaysRemaining <= 7;

              return (
                <div
                  key={item.id}
                  className={`group relative rounded-xl border p-4 transition-all ${
                    isExpired
                      ? "border-rose-300 bg-rose-50/40 dark:border-rose-900/50 dark:bg-rose-950/20"
                      : isCritical
                      ? "border-rose-300 bg-rose-50/40 dark:border-rose-900/50 dark:bg-rose-950/20"
                      : isExpiring
                      ? "border-amber-300 bg-amber-50/40 dark:border-amber-900/50 dark:bg-amber-950/20"
                      : "border-slate-200 bg-white hover:border-emerald-300 dark:border-slate-800 dark:bg-slate-800/60"
                  }`}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0 flex-1">
                      <div
                        className="truncate font-semibold text-slate-900 dark:text-white"
                        title={`${item.domain}:${item.port}`}
                      >
                        {item.domain}:{item.port}
                      </div>
                      {item.alias && (
                        <div className="truncate text-xs text-slate-400" title={item.alias}>
                          {item.alias}
                        </div>
                      )}
                    </div>
                    <span
                      className={`shrink-0 whitespace-nowrap rounded-full px-2 py-0.5 text-[10px] font-semibold ${
                        isExpired
                          ? "bg-rose-100 text-rose-700 dark:bg-rose-900/40 dark:text-rose-300"
                          : isCritical
                          ? "bg-rose-100 text-rose-700 dark:bg-rose-900/40 dark:text-rose-300"
                          : isExpiring
                          ? "bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300"
                          : "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300"
                      }`}
                    >
                      {isExpired
                        ? "已过期"
                        : isCritical
                        ? `剩余 ${item.lastDaysRemaining} 天 (每日提醒)`
                        : item.lastDaysRemaining !== undefined && item.lastDaysRemaining !== null
                        ? `剩余 ${item.lastDaysRemaining} 天`
                        : "未检测"}
                    </span>
                  </div>

                  <div className="mt-3 flex items-center justify-between text-xs text-slate-500 dark:text-slate-400">
                    <span>
                      {item.lastCheckAt
                        ? `上次检测: ${new Date(item.lastCheckAt * 1000).toLocaleString()}`
                        : "待首次巡检"}
                    </span>
                    <div className="flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
                      <button
                        type="button"
                        onClick={() => {
                          setCheckDomain(item.domain);
                          setCheckPort(String(item.port));
                          handleCheck(item.domain);
                        }}
                        className="rounded p-1 hover:bg-slate-100 hover:text-emerald-600 dark:hover:bg-slate-700"
                        title="立即测试"
                      >
                        <Search className="h-3.5 w-3.5" />
                      </button>
                      <button
                        type="button"
                        onClick={() => handleRemoveWatch(item.id, item.domain)}
                        className="rounded p-1 hover:bg-rose-100 hover:text-rose-600 dark:hover:bg-rose-900/40"
                        title="取消关注"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
