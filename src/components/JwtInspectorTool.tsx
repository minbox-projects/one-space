import { useMemo, useState } from "react";
import {
  Building,
  Check,
  CheckCircle2,
  Clock,
  Copy,
  KeyRound,
  RotateCcw,
  ShieldCheck,
  User,
  XCircle,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ToolShell } from "@/components/toolbox/ToolShell";
import { ToolStatusBadge } from "@/components/toolbox/ToolStatusBadge";
import { useToast } from "./ToastProvider";
import { getToolboxTool } from "@/toolbox/registry";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import { parseJwt, verifyJwtSignature } from "@/lib/jwtInspector";

export function JwtInspectorTool() {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const toolDescriptor = getToolboxTool("jwt-inspector");
  const ToolIcon = toolDescriptor?.icon ?? ShieldCheck;
  const iconClassName =
    toolDescriptor?.iconClassName ?? "bg-emerald-500/10 text-emerald-600";

  const [tokenInput, setTokenInput] = useState("");
  const [secretInput, setSecretInput] = useState("");
  const [verifyStatus, setVerifyStatus] = useState<{
    tested: boolean;
    valid: boolean;
    message?: string;
  } | null>(null);

  const { copied: copiedPayload, copy: copyPayload } = useCopyToClipboard({
    onSuccess: () =>
      pushToast({
        title: t("jwtPayloadCopied", "Payload JSON copied"),
        kind: "success",
      }),
  });

  const { copied: copiedToken, copy: copyToken } = useCopyToClipboard({
    onSuccess: () =>
      pushToast({
        title: t("jwtTokenCopied", "Token copied"),
        kind: "success",
      }),
  });

  // 解析结果
  const parseResult = useMemo(() => {
    if (!tokenInput.trim()) return null;
    return parseJwt(tokenInput.trim());
  }, [tokenInput]);

  const handleVerify = async () => {
    if (!tokenInput.trim() || !secretInput) return;
    const res = await verifyJwtSignature(
      tokenInput.trim(),
      secretInput,
      parseResult?.header.alg,
    );
    setVerifyStatus({
      tested: true,
      valid: res.verified,
      message: res.message,
    });
  };

  const handleClear = () => {
    setTokenInput("");
    setVerifyStatus(null);
  };

  return (
    <ToolShell
      titleId="jwt-inspector-title"
      title={t("jwtInspector", "JWT Inspector & Debugger")}
      description={t(
        "jwtInspectorDesc",
        "Inspect, decode and verify JWT claims and subject data securely offline.",
      )}
      icon={
        <div className={`rounded-lg p-2 ${iconClassName}`}>
          <ToolIcon className="h-5 w-5" />
        </div>
      }
      actions={
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={handleClear}
            disabled={!tokenInput}
            className="inline-flex h-8 items-center gap-1.5 rounded-md border bg-background px-3 text-xs font-medium text-muted-foreground hover:bg-muted disabled:opacity-50"
          >
            <RotateCcw className="h-3.5 w-3.5" />
            {t("jwtClear", "Clear")}
          </button>
        </div>
      }
      error={
        parseResult && !parseResult.valid
          ? `${t("jwtParseError", "Invalid JWT format")}: ${parseResult.error}`
          : null
      }
    >
      <div className="space-y-5">
        {/* 输入区 */}
        <div className="space-y-2 rounded-lg border bg-card p-4">
          <div className="flex items-center justify-between text-xs font-medium text-muted-foreground">
            <span>{t("jwtInputLabel", "Encoded JWT Token")}</span>
            {tokenInput ? (
              <button
                type="button"
                onClick={() => void copyToken(tokenInput)}
                className="inline-flex items-center gap-1 hover:text-foreground"
              >
                {copiedToken ? (
                  <Check className="h-3.5 w-3.5 text-emerald-600" />
                ) : (
                  <Copy className="h-3.5 w-3.5" />
                )}
                {t("jwtCopyToken", "Copy Token")}
              </button>
            ) : null}
          </div>
          <textarea
            value={tokenInput}
            onChange={(e) => {
              setTokenInput(e.target.value);
              setVerifyStatus(null);
            }}
            placeholder={t(
              "jwtInputPlaceholder",
              "Paste JWT token string here (header.payload.signature)...",
            )}
            className="h-28 w-full resize-none rounded-md border bg-background p-3 font-mono text-xs leading-5 outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring"
            spellCheck={false}
          />
        </div>

        {/* 解析成功后展示各部分 */}
        {parseResult && parseResult.valid ? (
          <>
            {/* 核心卡片 1：主体身份与权限全景 (Subject & Identity Panorama) */}
            <div className="rounded-lg border bg-card p-4 shadow-sm">
              <div className="mb-3 flex items-center gap-2 border-b pb-2">
                <User className="h-4 w-4 text-primary" />
                <h3 className="text-sm font-semibold tracking-tight">
                  {t("jwtSubjectTitle", "Subject Identity & Authority")}
                </h3>
                <span className="text-xs text-muted-foreground">
                  ({t("jwtSubjectSubtitle", "Extracted business claims")})
                </span>
              </div>

              <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
                {/* 主体 ID */}
                <div className="rounded-md border bg-background/50 p-3">
                  <div className="text-[11px] font-medium text-muted-foreground">
                    {t("jwtSubjectId", "Subject / User ID")}
                  </div>
                  <div className="mt-1 font-mono text-xs font-semibold text-foreground truncate">
                    {parseResult.subjectContext.subjectId || "-"}
                  </div>
                </div>

                {/* 主体名称 */}
                <div className="rounded-md border bg-background/50 p-3">
                  <div className="text-[11px] font-medium text-muted-foreground">
                    {t("jwtSubjectName", "Username / Name")}
                  </div>
                  <div className="mt-1 text-xs font-semibold text-foreground truncate">
                    {parseResult.subjectContext.subjectName || "-"}
                  </div>
                </div>

                {/* 租户信息 */}
                <div className="rounded-md border bg-background/50 p-3">
                  <div className="flex items-center gap-1 text-[11px] font-medium text-muted-foreground">
                    <Building className="h-3 w-3" />
                    {t("jwtTenantId", "Tenant / Organization")}
                  </div>
                  <div className="mt-1 font-mono text-xs font-semibold text-foreground truncate">
                    {parseResult.subjectContext.tenantId || "-"}
                  </div>
                </div>

                {/* 邮箱 */}
                <div className="rounded-md border bg-background/50 p-3">
                  <div className="text-[11px] font-medium text-muted-foreground">
                    {t("jwtEmail", "Email")}
                  </div>
                  <div className="mt-1 text-xs font-medium text-foreground truncate">
                    {parseResult.subjectContext.email || "-"}
                  </div>
                </div>
              </div>

              {/* 角色与权限胶囊 */}
              <div className="mt-4 space-y-3">
                {parseResult.subjectContext.roles.length > 0 ? (
                  <div>
                    <div className="text-[11px] font-medium text-muted-foreground mb-1.5">
                      {t("jwtRoles", "Roles")}:
                    </div>
                    <div className="flex flex-wrap gap-1.5">
                      {parseResult.subjectContext.roles.map((role) => (
                        <span
                          key={role}
                          className="inline-flex items-center rounded-md border border-sky-500/20 bg-sky-500/10 px-2 py-0.5 font-mono text-[11px] font-medium text-sky-600"
                        >
                          {role}
                        </span>
                      ))}
                    </div>
                  </div>
                ) : null}

                {parseResult.subjectContext.permissions.length > 0 ? (
                  <div>
                    <div className="text-[11px] font-medium text-muted-foreground mb-1.5">
                      {t("jwtPermissions", "Permissions & Scopes")}:
                    </div>
                    <div className="flex flex-wrap gap-1.5">
                      {parseResult.subjectContext.permissions.map((perm) => (
                        <span
                          key={perm}
                          className="inline-flex items-center rounded-md border border-violet-500/20 bg-violet-500/10 px-2 py-0.5 font-mono text-[11px] text-violet-600"
                        >
                          {perm}
                        </span>
                      ))}
                    </div>
                  </div>
                ) : null}
              </div>
            </div>

            {/* 核心卡片 2：生命周期与过期状态 */}
            <div className="rounded-lg border bg-card p-4 shadow-sm">
              <div className="mb-3 flex items-center justify-between border-b pb-2">
                <div className="flex items-center gap-2">
                  <Clock className="h-4 w-4 text-primary" />
                  <h3 className="text-sm font-semibold tracking-tight">
                    {t("jwtLifecycleTitle", "Lifecycle & Expiration")}
                  </h3>
                </div>

                <div>
                  {parseResult.lifecycle.status === "valid" ? (
                    <ToolStatusBadge
                      tone="success"
                      label={t("jwtStatusValid", "Active & Valid")}
                    />
                  ) : parseResult.lifecycle.status === "expired" ? (
                    <ToolStatusBadge
                      tone="error"
                      label={t("jwtStatusExpired", "Expired")}
                    />
                  ) : parseResult.lifecycle.status === "not-yet-valid" ? (
                    <ToolStatusBadge
                      tone="warning"
                      label={t("jwtStatusNotYetValid", "Not Yet Valid")}
                    />
                  ) : (
                    <ToolStatusBadge
                      tone="neutral"
                      label={t("jwtStatusNoExp", "No Expiration Set")}
                    />
                  )}
                </div>
              </div>

              <div className="grid gap-3 sm:grid-cols-3 text-xs">
                <div>
                  <span className="text-muted-foreground">
                    {t("jwtIat", "Issued At (iat)")}:
                  </span>
                  <div className="mt-0.5 font-medium">
                    {parseResult.lifecycle.issuedAt?.formatted || "-"}
                  </div>
                </div>
                <div>
                  <span className="text-muted-foreground">
                    {t("jwtNbf", "Not Before (nbf)")}:
                  </span>
                  <div className="mt-0.5 font-medium">
                    {parseResult.lifecycle.notBefore?.formatted || "-"}
                  </div>
                </div>
                <div>
                  <span className="text-muted-foreground">
                    {t("jwtExp", "Expires At (exp)")}:
                  </span>
                  <div className="mt-0.5 font-medium">
                    {parseResult.lifecycle.expiresAt?.formatted || "-"}
                  </div>
                </div>
              </div>

              {/* 进度条 */}
              {parseResult.lifecycle.progressPercentage !== undefined ? (
                <div className="mt-3">
                  <div className="flex justify-between text-[11px] text-muted-foreground mb-1">
                    <span>{t("jwtTimeElapsed", "Lifecycle elapsed")}</span>
                    <span>{parseResult.lifecycle.progressPercentage}%</span>
                  </div>
                  <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                    <div
                      className={`h-full transition-all duration-300 ${
                        parseResult.lifecycle.status === "expired"
                          ? "bg-rose-500"
                          : "bg-emerald-500"
                      }`}
                      style={{
                        width: `${Math.min(
                          100,
                          parseResult.lifecycle.progressPercentage,
                        )}%`,
                      }}
                    />
                  </div>
                </div>
              ) : null}
            </div>

            {/* 核心卡片 3：结构化 JSON 双栏对照 */}
            <div className="grid gap-4 lg:grid-cols-[280px_1fr]">
              {/* Header */}
              <div className="space-y-2 rounded-lg border bg-card p-4 shadow-sm">
                <div className="flex items-center justify-between text-xs font-semibold">
                  <span>{t("jwtHeader", "Header (Algorithm & Type)")}</span>
                  <span className="font-mono text-[11px] text-muted-foreground">
                    {parseResult.header.alg || "Unknown"}
                  </span>
                </div>
                <pre className="overflow-x-auto rounded-md bg-muted/60 p-2.5 font-mono text-xs leading-5 text-foreground">
                  {JSON.stringify(parseResult.header, null, 2)}
                </pre>
              </div>

              {/* Payload 完整视图 */}
              <div className="space-y-2 rounded-lg border bg-card p-4 shadow-sm">
                <div className="flex items-center justify-between text-xs font-semibold">
                  <span>{t("jwtPayload", "Payload (All Claims)")}</span>
                  <button
                    type="button"
                    onClick={() =>
                      void copyPayload(
                        JSON.stringify(parseResult.payload, null, 2),
                      )
                    }
                    className="inline-flex items-center gap-1 text-[11px] font-normal text-muted-foreground hover:text-foreground"
                  >
                    {copiedPayload ? (
                      <Check className="h-3 w-3 text-emerald-600" />
                    ) : (
                      <Copy className="h-3 w-3" />
                    )}
                    {t("jwtCopyPayload", "Copy JSON")}
                  </button>
                </div>
                <pre className="max-h-72 overflow-auto rounded-md bg-muted/60 p-2.5 font-mono text-xs leading-5 text-foreground">
                  {JSON.stringify(parseResult.payload, null, 2)}
                </pre>
              </div>
            </div>

            {/* 核心卡片 4：离线签名验证 (Offline Verification) */}
            <div className="rounded-lg border bg-card p-4 shadow-sm space-y-3">
              <div className="flex items-center gap-2">
                <KeyRound className="h-4 w-4 text-primary" />
                <h3 className="text-sm font-semibold tracking-tight">
                  {t("jwtVerifyTitle", "Offline Signature Verification")}
                </h3>
                <span className="text-xs text-muted-foreground">
                  ({t("jwtVerifyNotice", "Local Web Crypto only, zero network leakage")})
                </span>
              </div>

              <div className="flex flex-wrap items-center gap-3">
                <input
                  type="text"
                  value={secretInput}
                  onChange={(e) => {
                    setSecretInput(e.target.value);
                    setVerifyStatus(null);
                  }}
                  placeholder={t(
                    "jwtSecretPlaceholder",
                    "Enter HMAC secret key to verify...",
                  )}
                  className="h-9 min-w-64 flex-1 rounded-md border bg-background px-3 font-mono text-xs outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring"
                />
                <button
                  type="button"
                  onClick={handleVerify}
                  disabled={!secretInput.trim()}
                  className="inline-flex h-9 items-center gap-1.5 rounded-md bg-primary px-4 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                >
                  <ShieldCheck className="h-4 w-4" />
                  {t("jwtVerifyAction", "Verify Signature")}
                </button>
              </div>

              {verifyStatus?.tested ? (
                <div
                  className={`flex items-center gap-2 rounded-md p-2.5 text-xs font-medium ${
                    verifyStatus.valid
                      ? "bg-emerald-500/10 text-emerald-600 border border-emerald-500/20"
                      : "bg-destructive/10 text-destructive border border-destructive/20"
                  }`}
                >
                  {verifyStatus.valid ? (
                    <>
                      <CheckCircle2 className="h-4 w-4" />
                      <span>
                        {t(
                          "jwtVerifySuccess",
                          "Signature verified successfully with provided secret key.",
                        )}
                      </span>
                    </>
                  ) : (
                    <>
                      <XCircle className="h-4 w-4" />
                      <span>
                        {t(
                          "jwtVerifyFail",
                          "Signature mismatch. Invalid secret key or corrupted token.",
                        )}
                      </span>
                    </>
                  )}
                </div>
              ) : null}
            </div>
          </>
        ) : null}
      </div>
    </ToolShell>
  );
}
