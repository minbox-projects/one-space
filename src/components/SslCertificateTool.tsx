import { useCallback, useEffect, useState } from "react";
import {
  AlertCircle,
  AlertTriangle,
  Bell,
  CheckCircle2,
  Copy,
  Download,
  FileCode2,
  Globe,
  History,
  Layers,
  Loader2,
  Plus,
  Clock,
  RefreshCw,
  Search,
  ShieldCheck,
  Star,
  Trash2,
} from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";

import { useToast } from "./ToastProvider";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import {
  sslCertificateCreateOrder,
  sslCertificateDelete,
  sslCertificateDohPrecheck,
  sslCertificateExport,
  sslCertificateList,
  sslCertificateVerifyAndFinalize,
  sslCheckCertificate,
  sslGetConfig,
  sslWatchlistAdd,
  sslWatchlistCheckAll,
  sslWatchlistList,
  sslWatchlistRemove,
  type SslChallengeItem,
  type SslCheckResult,
  type SslConfig,
  type SslIssuedCertificate,
  type SslWatchItem,
} from "@/lib/sslCertificate";

type ActiveTab = "check" | "issue";

const SSL_CHECK_HISTORY_KEY = "onespace:ssl-check-history";

function sanitizeDomainInput(domain: string): string {
  return domain
    .trim()
    .replace(/^https?:\/\//i, "")
    .split("/")[0]
    .split(":")[0]
    .trim()
    .toLowerCase();
}

export function SslCertificateTool() {
  const { pushToast } = useToast();
  const { copy } = useCopyToClipboard();

  const [activeTab, setActiveTab] = useState<ActiveTab>("check");

  // === Tab 1: Check & Watchlist State ===
  const [checkDomain, setCheckDomain] = useState("");
  const [checkPort, setCheckPort] = useState("443");
  const [isChecking, setIsChecking] = useState(false);
  const [checkResult, setCheckResult] = useState<SslCheckResult | null>(null);
  const [checkError, setCheckError] = useState<string | null>(null);

  const [watchlist, setWatchlist] = useState<SslWatchItem[]>([]);
  const [isCheckingAllWatchlist, setIsCheckingAllWatchlist] = useState(false);
  const [sslConfig, setSslConfig] = useState<SslConfig | null>(null);

  const loadSslConfigData = useCallback(async () => {
    try {
      const cfg = await sslGetConfig();
      setSslConfig(cfg);
    } catch (e) {
      console.error(e);
    }
  }, []);

  // === Tab 2: Issue & Management State ===
  const [issuedCerts, setIssuedCerts] = useState<SslIssuedCertificate[]>([]);
  const [showApplyModal, setShowApplyModal] = useState(false);

  // Apply Form State
  const [applyStep, setApplyStep] = useState<1 | 2>(1);
  const [primaryDomain, setPrimaryDomain] = useState("");
  const [sanDomainsInput, setSanDomainsInput] = useState("");
  const [keyType, setKeyType] = useState<"ecc_p256" | "rsa2048">("ecc_p256");
  const [useStaging, setUseStaging] = useState(false);
  const [emailInput, setEmailInput] = useState("");
  const [verifyMode, setVerifyMode] = useState<"manual_dns" | "cloudflare">("manual_dns");
  const [cfToken, setCfToken] = useState("");

  const [isCreatingOrder, setIsCreatingOrder] = useState(false);
  const [activeOrderId, setActiveOrderId] = useState<string | null>(null);
  const [challenges, setChallenges] = useState<SslChallengeItem[]>([]);
  const [dohChecking, setDohChecking] = useState<Record<string, boolean>>({});
  const [dohResolved, setDohResolved] = useState<Record<string, boolean>>({});
  const [isVerifyingOrder, setIsVerifyingOrder] = useState(false);

  // Selected Cert Preview Drawer
  const [selectedCert, setSelectedCert] = useState<SslIssuedCertificate | null>(null);
  const [selectedCertTab, setSelectedCertTab] = useState<"nginx" | "fullchain" | "privkey">("nginx");

  // Load initial data
  const loadWatchlistData = useCallback(async () => {
    try {
      const items = await sslWatchlistList();
      setWatchlist(items);
    } catch (e) {
      console.error(e);
    }
  }, []);

  const loadIssuedData = useCallback(async () => {
    try {
      const certs = await sslCertificateList();
      setIssuedCerts(certs);
    } catch (e) {
      console.error(e);
    }
  }, []);

  useEffect(() => {
    loadWatchlistData();
    loadIssuedData();
    loadSslConfigData();

    const unlistenPromise = listen("ssl-watchlist-updated", () => {
      loadWatchlistData();
    });

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [loadWatchlistData, loadIssuedData, loadSslConfigData]);

  const [historyDomains, setHistoryDomains] = useState<string[]>(() => {
    try {
      const stored = localStorage.getItem(SSL_CHECK_HISTORY_KEY);
      return stored ? JSON.parse(stored) : [];
    } catch {
      return [];
    }
  });

  const recordHistoryDomain = (rawDomain: string) => {
    const clean = sanitizeDomainInput(rawDomain);
    if (!clean) return;
    setHistoryDomains((prev) => {
      const next = [clean, ...prev.filter((d) => d.toLowerCase() !== clean)].slice(0, 10);
      try {
        localStorage.setItem(SSL_CHECK_HISTORY_KEY, JSON.stringify(next));
      } catch (err) {
        console.error("Failed to save ssl check history", err);
      }
      return next;
    });
  };

  const handleClearHistory = () => {
    try {
      localStorage.removeItem(SSL_CHECK_HISTORY_KEY);
    } catch (err) {
      console.error("Failed to clear ssl check history", err);
    }
    setHistoryDomains([]);
  };

  // Handle immediate check
  const handleCheck = async (domainToTest?: string) => {
    const rawDomain = (domainToTest || checkDomain).trim();
    if (!rawDomain) {
      pushToast({ title: "请输入需要检测的域名", kind: "warning" });
      return;
    }
    const clean = sanitizeDomainInput(rawDomain);
    recordHistoryDomain(clean);
    setCheckDomain(clean);

    const port = parseInt(checkPort, 10) || 443;
    setIsChecking(true);
    setCheckError(null);
    setCheckResult(null);

    try {
      const res = await sslCheckCertificate(clean, port);
      setCheckResult(res);
      pushToast({ title: `TLS 握手检测成功: ${res.domain}`, kind: "success" });
    } catch (e: any) {
      setCheckError(String(e));
      pushToast({ title: "TLS 检测失败", description: String(e), kind: "error" });
    } finally {
      setIsChecking(false);
    }
  };

  // Add current checked domain to watchlist
  const handleAddToWatchlist = async () => {
    if (!checkResult) return;
    try {
      await sslWatchlistAdd({
        domain: checkResult.domain,
        port: checkResult.port,
        alertDaysThreshold: sslConfig?.alertDaysThreshold || 30,
      });
      pushToast({ title: `已将 ${checkResult.domain} 加入监控列表`, kind: "success" });
      loadWatchlistData();
    } catch (e: any) {
      pushToast({ title: "关注失败", description: String(e), kind: "error" });
    }
  };

  // Remove item from watchlist
  const handleRemoveWatch = async (id: string, domain: string) => {
    try {
      await sslWatchlistRemove(id);
      pushToast({ title: `已取消关注 ${domain}`, kind: "info" });
      setWatchlist((prev) => prev.filter((it) => it.id !== id));
    } catch (e: any) {
      pushToast({ title: "取消关注失败", description: String(e), kind: "error" });
    }
  };

  // Check all watchlist
  const handleCheckAllWatchlist = async () => {
    setIsCheckingAllWatchlist(true);
    try {
      const items = await sslWatchlistCheckAll();
      setWatchlist(items);
      pushToast({ title: "全部关注项巡检完成", kind: "success" });
    } catch (e: any) {
      pushToast({ title: "批量巡检失败", description: String(e), kind: "error" });
    } finally {
      setIsCheckingAllWatchlist(false);
    }
  };

  // Start Order Creation
  const handleStartApply = async () => {
    const cleanPrimary = primaryDomain.trim();
    if (!cleanPrimary) {
      pushToast({ title: "请输入主域名", kind: "warning" });
      return;
    }

    const sans = sanDomainsInput
      .split(/[\n,]/)
      .map((s) => s.trim())
      .filter(Boolean);

    setIsCreatingOrder(true);
    try {
      const res = await sslCertificateCreateOrder({
        domain: cleanPrimary,
        sanDomains: sans,
        email: emailInput.trim() || undefined,
        keyType,
        useStaging,
        cloudflareApiToken: verifyMode === "cloudflare" ? cfToken.trim() : undefined,
      });

      setActiveOrderId(res.orderId);
      setChallenges(res.challenges);
      setApplyStep(2);
      pushToast({ title: "ACME 订单创建成功，请配置 DNS TXT 记录", kind: "success" });
    } catch (e: any) {
      pushToast({ title: "订单创建失败", description: String(e), kind: "error" });
    } finally {
      setIsCreatingOrder(false);
    }
  };

  // DoH Precheck single challenge
  const handleDohCheck = async (ch: SslChallengeItem) => {
    setDohChecking((prev) => ({ ...prev, [ch.recordName]: true }));
    try {
      const res = await sslCertificateDohPrecheck(ch.recordName, ch.recordValue);
      setDohResolved((prev) => ({ ...prev, [ch.recordName]: res.resolved }));
      if (res.resolved) {
        pushToast({ title: `TXT 记录已全球生效: ${ch.recordName}`, kind: "success" });
      } else {
        pushToast({
          title: `公网 DNS 尚未查询到匹配记录`,
          description: `期望值: ${ch.recordValue}，当前查询到: ${res.foundValues.join(", ") || "无"}`,
          kind: "warning",
        });
      }
    } catch (e: any) {
      pushToast({ title: "DoH 检测异常", description: String(e), kind: "error" });
    } finally {
      setDohChecking((prev) => ({ ...prev, [ch.recordName]: false }));
    }
  };

  // Verify and Finalize Order
  const handleVerifyAndFinalize = async () => {
    if (!activeOrderId) return;
    setIsVerifyingOrder(true);
    try {
      const cert = await sslCertificateVerifyAndFinalize(activeOrderId);
      pushToast({ title: `🎉 证书签发成功: ${cert.primaryDomain}`, kind: "success" });
      setShowApplyModal(false);
      setApplyStep(1);
      setActiveOrderId(null);
      setChallenges([]);
      loadIssuedData();
      setSelectedCert(cert);
    } catch (e: any) {
      pushToast({ title: "证书核验或签发失败", description: String(e), kind: "error" });
    } finally {
      setIsVerifyingOrder(false);
    }
  };

  // Export certificate files
  const handleExportFiles = async (certId: string) => {
    try {
      const selectedPath = await openDialog({
        directory: true,
        multiple: false,
        title: "选择证书导出目标目录",
      });

      if (!selectedPath || typeof selectedPath !== "string") {
        return;
      }

      const res = await sslCertificateExport(certId, selectedPath);
      pushToast({
        title: "证书及 Nginx 配置导出成功！",
        description: `已保存至: ${res.exportPath}`,
        kind: "success",
      });
    } catch (e: any) {
      pushToast({ title: "导出失败", description: String(e), kind: "error" });
    }
  };

  // Delete issued certificate
  const handleDeleteIssued = async (id: string, domain: string) => {
    if (!window.confirm(`确定要删除本地保存的 ${domain} 证书吗？`)) return;
    try {
      await sslCertificateDelete(id);
      pushToast({ title: `已删除证书 ${domain}`, kind: "info" });
      setIssuedCerts((prev) => prev.filter((c) => c.id !== id));
      if (selectedCert?.id === id) {
        setSelectedCert(null);
      }
    } catch (e: any) {
      pushToast({ title: "删除失败", description: String(e), kind: "error" });
    }
  };

  const isDomainWatched = checkResult && watchlist.some((w) => w.domain === checkResult.domain && w.port === checkResult.port);

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
                              ? `上次检测: ${new Date(item.lastCheckAt * 1000).toLocaleDateString()}`
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
        ) : (
          /* Tab 2: Issue & Management */
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
        )}
      </div>

      {/* 申请向导弹窗 (Wizard Modal) */}
      {showApplyModal && (
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
                      <select
                        value={keyType}
                        onChange={(e: any) => setKeyType(e.target.value)}
                        className="w-full rounded-xl border border-slate-200 bg-slate-50 px-3.5 py-2.5 text-sm text-slate-900 dark:border-slate-700 dark:bg-slate-800 dark:text-white"
                      >
                        <option value="ecc_p256">ECC P-256 (推荐，性能极高)</option>
                        <option value="rsa2048">RSA 2048 (传统兼容性好)</option>
                      </select>
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
      )}

      {/* 证书详情与 Nginx 配置预览抽屉 (Detail Drawer) */}
      {selectedCert && (
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
      )}
    </div>
  );
}
