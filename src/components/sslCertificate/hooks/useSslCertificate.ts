import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";

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
import type { ActionContext } from "@/lib/userActions";
import type { UseCopyToClipboardResult } from "@/toolbox/useCopyToClipboard";
import type { ActiveTab } from "../types";

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

export function useSslCertificate(
  pushToast: ActionContext["pushToast"],
  copy: UseCopyToClipboardResult["copy"],
) {
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

  return {
    activeTab,
    setActiveTab,
    checkDomain,
    setCheckDomain,
    checkPort,
    setCheckPort,
    isChecking,
    checkResult,
    checkError,
    watchlist,
    isCheckingAllWatchlist,
    sslConfig,
    issuedCerts,
    showApplyModal,
    setShowApplyModal,
    applyStep,
    setApplyStep,
    primaryDomain,
    setPrimaryDomain,
    sanDomainsInput,
    setSanDomainsInput,
    keyType,
    setKeyType,
    useStaging,
    setUseStaging,
    emailInput,
    setEmailInput,
    verifyMode,
    setVerifyMode,
    cfToken,
    setCfToken,
    isCreatingOrder,
    challenges,
    dohChecking,
    dohResolved,
    isVerifyingOrder,
    selectedCert,
    setSelectedCert,
    selectedCertTab,
    setSelectedCertTab,
    historyDomains,
    handleClearHistory,
    handleCheck,
    handleAddToWatchlist,
    handleRemoveWatch,
    handleCheckAllWatchlist,
    handleStartApply,
    handleDohCheck,
    handleVerifyAndFinalize,
    handleExportFiles,
    handleDeleteIssued,
    isDomainWatched,
    copy,
    pushToast,
  };
}

export type SslCertificateController = ReturnType<typeof useSslCertificate>;
