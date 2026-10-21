/**
 * JWT 核心解析与离线安全分析库
 */

export interface JwtHeader {
  alg?: string;
  typ?: string;
  kid?: string;
  [key: string]: unknown;
}

export interface JwtSubjectContext {
  subjectId?: string;
  subjectName?: string;
  email?: string;
  tenantId?: string;
  roles: string[];
  permissions: string[];
  customClaims: Record<string, unknown>;
}

export type JwtLifecycleStatus = "valid" | "expired" | "not-yet-valid" | "no-exp";

export interface JwtLifecycle {
  issuedAt?: { timestamp: number; formatted: string };
  notBefore?: { timestamp: number; formatted: string };
  expiresAt?: { timestamp: number; formatted: string };
  status: JwtLifecycleStatus;
  remainingSeconds?: number;
  progressPercentage?: number;
}

export interface JwtParseResult {
  valid: boolean;
  rawHeader: string;
  rawPayload: string;
  rawSignature: string;
  header: JwtHeader;
  payload: Record<string, unknown>;
  subjectContext: JwtSubjectContext;
  lifecycle: JwtLifecycle;
  error?: string;
}

/**
 * 安全解码 Base64URL 字符串，支持 UTF-8 多字节字符（如中文）
 */
export function decodeBase64Url(base64UrlStr: string): string {
  let base64 = base64UrlStr.replace(/-/g, "+").replace(/_/g, "/");
  while (base64.length % 4 !== 0) {
    base64 += "=";
  }

  // 跨浏览器与 Node 兼容的 UTF-8 解码
  if (typeof Buffer !== "undefined") {
    return Buffer.from(base64, "base64").toString("utf-8");
  }

  const binaryStr = atob(base64);
  const bytes = new Uint8Array(binaryStr.length);
  for (let i = 0; i < binaryStr.length; i++) {
    bytes[i] = binaryStr.charCodeAt(i);
  }
  return new TextDecoder().decode(bytes);
}

/**
 * Base64URL 转 Uint8Array 字节数组
 */
export function base64UrlToBytes(base64UrlStr: string): Uint8Array {
  let base64 = base64UrlStr.replace(/-/g, "+").replace(/_/g, "/");
  while (base64.length % 4 !== 0) {
    base64 += "=";
  }
  if (typeof Buffer !== "undefined") {
    const buf = Buffer.from(base64, "base64");
    return new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
  }
  const binaryStr = atob(base64);
  const bytes = new Uint8Array(binaryStr.length);
  for (let i = 0; i < binaryStr.length; i++) {
    bytes[i] = binaryStr.charCodeAt(i);
  }
  return bytes;
}

/**
 * 提取主体身份与业务鉴权上下文
 */
export function extractSubjectContext(
  payload: Record<string, unknown>,
): JwtSubjectContext {
  // 1. 主体唯一标识
  const subjectId = [
    payload.sub,
    payload.user_id,
    payload.uid,
    payload.userId,
    payload.accountId,
  ].find((v) => typeof v === "string" || typeof v === "number");

  // 2. 主体可读名称
  const subjectName = [
    payload.username,
    payload.preferred_username,
    payload.name,
    payload.nickname,
    payload.email,
  ].find((v) => typeof v === "string");

  // 3. 邮箱
  const email = typeof payload.email === "string" ? payload.email : undefined;

  // 4. 租户与多组织信息
  const tenantId = [
    payload.tenant_id,
    payload.tenantId,
    payload.org_id,
    payload.dept_id,
    payload.company,
  ].find((v) => typeof v === "string" || typeof v === "number");

  // 5. 角色列表（兼容数组、逗号分隔、空格分隔）
  const roles: string[] = [];
  const rawRoles = payload.roles ?? payload.role;
  if (Array.isArray(rawRoles)) {
    for (const r of rawRoles) {
      if (typeof r === "string" && r.trim()) roles.push(r.trim());
    }
  } else if (typeof rawRoles === "string") {
    roles.push(...rawRoles.split(/[\s,]+/).filter(Boolean));
  }

  // 6. 权限/Scope 列表（兼容 authorities, permissions, scope, scp）
  const permissions: string[] = [];
  const rawPerms =
    payload.permissions ??
    payload.authorities ??
    payload.scope ??
    payload.scp;
  if (Array.isArray(rawPerms)) {
    for (const p of rawPerms) {
      if (typeof p === "string" && p.trim()) permissions.push(p.trim());
      else if (typeof p === "object" && p !== null && "authority" in p) {
        // Spring Security 常见结构 [{ authority: "ROLE_USER" }]
        const auth = (p as { authority: unknown }).authority;
        if (typeof auth === "string" && auth.trim()) permissions.push(auth.trim());
      }
    }
  } else if (typeof rawPerms === "string") {
    permissions.push(...rawPerms.split(/[\s,]+/).filter(Boolean));
  }

  // 7. 提取业务自定义 Claims（排除标准与已提取键）
  const standardKeys = new Set([
    "iss",
    "sub",
    "aud",
    "exp",
    "nbf",
    "iat",
    "jti",
    "user_id",
    "uid",
    "userId",
    "accountId",
    "username",
    "preferred_username",
    "name",
    "nickname",
    "email",
    "tenant_id",
    "tenantId",
    "org_id",
    "dept_id",
    "company",
    "roles",
    "role",
    "permissions",
    "authorities",
    "scope",
    "scp",
  ]);

  const customClaims: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(payload)) {
    if (!standardKeys.has(key)) {
      customClaims[key] = value;
    }
  }

  return {
    subjectId: subjectId !== undefined ? String(subjectId) : undefined,
    subjectName: subjectName !== undefined ? String(subjectName) : undefined,
    email,
    tenantId: tenantId !== undefined ? String(tenantId) : undefined,
    roles,
    permissions,
    customClaims,
  };
}

function formatUnixTime(timestampSec: number): string {
  try {
    const date = new Date(timestampSec * 1000);
    return date.toLocaleString();
  } catch {
    return String(timestampSec);
  }
}

/**
 * 提取生命周期与状态
 */
export function extractLifecycle(
  payload: Record<string, unknown>,
  nowSec: number = Math.floor(Date.now() / 1000),
): JwtLifecycle {
  const iat = typeof payload.iat === "number" ? payload.iat : undefined;
  const nbf = typeof payload.nbf === "number" ? payload.nbf : undefined;
  const exp = typeof payload.exp === "number" ? payload.exp : undefined;

  let status: JwtLifecycleStatus = "no-exp";
  let remainingSeconds: number | undefined;
  let progressPercentage: number | undefined;

  if (exp !== undefined) {
    remainingSeconds = exp - nowSec;
    if (remainingSeconds < 0) {
      status = "expired";
    } else if (nbf !== undefined && nowSec < nbf) {
      status = "not-yet-valid";
    } else {
      status = "valid";
    }

    if (iat !== undefined && exp > iat) {
      const totalSpan = exp - iat;
      const elapsed = Math.max(0, Math.min(totalSpan, nowSec - iat));
      progressPercentage = Math.round((elapsed / totalSpan) * 100);
    }
  } else if (nbf !== undefined && nowSec < nbf) {
    status = "not-yet-valid";
  }

  return {
    issuedAt: iat !== undefined ? { timestamp: iat, formatted: formatUnixTime(iat) } : undefined,
    notBefore: nbf !== undefined ? { timestamp: nbf, formatted: formatUnixTime(nbf) } : undefined,
    expiresAt: exp !== undefined ? { timestamp: exp, formatted: formatUnixTime(exp) } : undefined,
    status,
    remainingSeconds,
    progressPercentage,
  };
}

/**
 * 解析完整 JWT 字符串
 */
export function parseJwt(token: string, nowSec?: number): JwtParseResult {
  const cleanToken = token.trim();
  const parts = cleanToken.split(".");

  if (parts.length !== 3) {
    return {
      valid: false,
      rawHeader: "",
      rawPayload: "",
      rawSignature: "",
      header: {},
      payload: {},
      subjectContext: { roles: [], permissions: [], customClaims: {} },
      lifecycle: { status: "no-exp" },
      error: `Invalid JWT format. Expected 3 dot-separated parts, got ${parts.length}`,
    };
  }

  try {
    const rawHeader = decodeBase64Url(parts[0]);
    const rawPayload = decodeBase64Url(parts[1]);
    const rawSignature = parts[2];

    const header = JSON.parse(rawHeader) as JwtHeader;
    const payload = JSON.parse(rawPayload) as Record<string, unknown>;

    const subjectContext = extractSubjectContext(payload);
    const lifecycle = extractLifecycle(payload, nowSec);

    return {
      valid: true,
      rawHeader,
      rawPayload,
      rawSignature,
      header,
      payload,
      subjectContext,
      lifecycle,
    };
  } catch (err) {
    return {
      valid: false,
      rawHeader: "",
      rawPayload: "",
      rawSignature: parts[2] || "",
      header: {},
      payload: {},
      subjectContext: { roles: [], permissions: [], customClaims: {} },
      lifecycle: { status: "no-exp" },
      error: err instanceof Error ? err.message : String(err),
    };
  }
}

/**
 * 使用 Web Crypto API 纯前端离线验签
 */
export async function verifyJwtSignature(
  token: string,
  secretOrPublicKey: string,
  algorithm?: string,
): Promise<{ verified: boolean; message?: string }> {
  const parts = token.trim().split(".");
  if (parts.length !== 3) {
    return { verified: false, message: "Invalid JWT format" };
  }

  const [headerB64, payloadB64, signatureB64] = parts;
  let parsedAlg = algorithm;

  if (!parsedAlg) {
    try {
      const header = JSON.parse(decodeBase64Url(headerB64)) as JwtHeader;
      parsedAlg = header.alg;
    } catch {
      return { verified: false, message: "Cannot determine algorithm from JWT header" };
    }
  }

  if (!parsedAlg) {
    return { verified: false, message: "Missing algorithm specification" };
  }

  const upperAlg = parsedAlg.toUpperCase();
  const signedContent = new TextEncoder().encode(`${headerB64}.${payloadB64}`);
  const signatureBytes = base64UrlToBytes(signatureB64);

  try {
    const subtle = globalThis.crypto?.subtle;
    if (!subtle) {
      return { verified: false, message: "Web Crypto API not available in current environment" };
    }

    if (upperAlg === "HS256" || upperAlg === "HS384" || upperAlg === "HS512") {
      const hashName = `SHA-${upperAlg.slice(2)}`;
      const keyBytes = new TextEncoder().encode(secretOrPublicKey);

      const cryptoKey = await subtle.importKey(
        "raw",
        keyBytes,
        { name: "HMAC", hash: { name: hashName } },
        false,
        ["verify"],
      );

      const isValid = await subtle.verify(
        "HMAC",
        cryptoKey,
        signatureBytes as BufferSource,
        signedContent as BufferSource,
      );

      return {
        verified: isValid,
        message: isValid ? "Signature verified successfully" : "Signature mismatch",
      };
    }

    return {
      verified: false,
      message: `Unsupported algorithm: ${upperAlg}. Currently supports HMAC algorithms (HS256, HS384, HS512).`,
    };
  } catch (err) {
    return {
      verified: false,
      message: `Verification failed: ${err instanceof Error ? err.message : String(err)}`,
    };
  }
}
