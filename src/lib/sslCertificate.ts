import { invokeToolboxCommand } from "@/toolbox/invoke";

export interface CertificateNode {
  subject: string;
  subjectCn?: string;
  issuer: string;
  issuerCn?: string;
  issuerOrg?: string;
  notBefore: number;
  notAfter: number;
  daysRemaining: number;
  sans: string[];
  serialNumber: string;
  signatureAlgorithm: string;
  publicKeyAlgorithm: string;
  sha256Fingerprint: string;
  isRoot: boolean;
  isCa: boolean;
}

export interface SslCheckResult {
  domain: string;
  port: number;
  ipAddress?: string;
  handshakeSuccess: boolean;
  tlsVersion?: string;
  cipherSuite?: string;
  isValid: boolean;
  validationError?: string;
  daysRemaining: number;
  isExpired: boolean;
  isExpiringSoon: boolean;
  domainMatches: boolean;
  certificates: CertificateNode[];
}

export type SslStatus = "valid" | "expiring_soon" | "expired" | "error";

export interface SslWatchItem {
  id: string;
  domain: string;
  port: number;
  alias?: string;
  enabled: boolean;
  alertDaysThreshold: number;
  lastCheckAt?: number;
  lastDaysRemaining?: number;
  lastStatus?: SslStatus;
  lastError?: string;
  issuerCn?: string;
  lastNotifiedDate?: string;
  createdAt: number;
}

export interface SslConfig {
  autoCheckEnabled: boolean;
  checkTimes: string[];
  alertDaysThreshold: number;
}

export interface SslWatchItemInput {
  domain: string;
  port?: number;
  alias?: string;
  alertDaysThreshold?: number;
}

export interface SslChallengeItem {
  domain: string;
  recordName: string;
  recordType: string;
  recordValue: string;
}

export interface SslOrderRequest {
  domain: string;
  sanDomains?: string[];
  email?: string;
  useStaging?: boolean;
  keyType?: "ecc_p256" | "rsa2048";
  cloudflareApiToken?: string;
}

export interface SslOrderResponse {
  orderId: string;
  challenges: SslChallengeItem[];
  status: string;
}

export interface SslIssuedCertificate {
  id: string;
  primaryDomain: string;
  sanDomains: string[];
  issuer: string;
  issuedAt: number;
  expiresAt: number;
  daysRemaining: number;
  keyType: string;
  certPem: string;
  chainPem: string;
  fullchainPem: string;
  privkeyPem?: string;
  nginxConfig: string;
  createdAt: number;
}

export async function sslCheckCertificate(
  domain: string,
  port: number = 443,
): Promise<SslCheckResult> {
  return invokeToolboxCommand<SslCheckResult>("ssl_check_certificate", {
    domain,
    port,
  });
}

export async function sslWatchlistList(): Promise<SslWatchItem[]> {
  return invokeToolboxCommand<SslWatchItem[]>("ssl_watchlist_list", {});
}

export async function sslWatchlistAdd(
  input: SslWatchItemInput,
): Promise<SslWatchItem> {
  return invokeToolboxCommand<SslWatchItem>("ssl_watchlist_add", { input });
}

export async function sslWatchlistRemove(id: string): Promise<void> {
  return invokeToolboxCommand<void>("ssl_watchlist_remove", { id });
}

export async function sslWatchlistCheckAll(): Promise<SslWatchItem[]> {
  return invokeToolboxCommand<SslWatchItem[]>("ssl_watchlist_check_all", {});
}

export async function sslCertificateCreateOrder(
  req: SslOrderRequest,
): Promise<SslOrderResponse> {
  return invokeToolboxCommand<SslOrderResponse>("ssl_certificate_create_order", {
    request: req,
  });
}

export async function sslCertificateDohPrecheck(
  recordName: string,
  expectedValue: string,
): Promise<{ resolved: boolean; foundValues: string[] }> {
  return invokeToolboxCommand<{ resolved: boolean; foundValues: string[] }>(
    "ssl_certificate_doh_precheck",
    {
      recordName,
      expectedValue,
    },
  );
}

export async function sslCertificateVerifyAndFinalize(
  orderId: string,
): Promise<SslIssuedCertificate> {
  return invokeToolboxCommand<SslIssuedCertificate>(
    "ssl_certificate_verify_and_finalize",
    {
      orderId,
    },
  );
}

export async function sslCertificateList(): Promise<SslIssuedCertificate[]> {
  return invokeToolboxCommand<SslIssuedCertificate[]>(
    "ssl_certificate_list",
    {},
  );
}

export async function sslCertificateDelete(id: string): Promise<void> {
  return invokeToolboxCommand<void>("ssl_certificate_delete", { id });
}

export async function sslCertificateExport(
  id: string,
  targetDir: string,
): Promise<{ success: boolean; exportPath: string }> {
  return invokeToolboxCommand<{ success: boolean; exportPath: string }>(
    "ssl_certificate_export",
    {
      id,
      targetDir,
    },
  );
}

export async function sslGetConfig(): Promise<SslConfig> {
  return invokeToolboxCommand<SslConfig>("ssl_get_config", {});
}

export async function sslSaveConfig(config: SslConfig): Promise<SslConfig> {
  return invokeToolboxCommand<SslConfig>("ssl_save_config", { config });
}
