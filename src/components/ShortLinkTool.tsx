import { useEffect, useRef, useState } from "react";
import {
  Check,
  Copy,
  Eye,
  EyeOff,
  Loader2,
  RefreshCw,
  Save,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useConfirmDialog } from "./ConfirmDialogProvider";
import { useToast } from "./ToastProvider";
import { getMoreToolPresentation } from "@/lib/moreToolPresentation";
import {
  ShortLinkError,
  shortLinkConfigStatus,
  shortLinkCreate,
  shortLinkDeleteToken,
  shortLinkSaveToken,
  type ShortLinkCreateResponse,
  type ShortLinkErrorCode,
} from "@/lib/shortLink";
import {
  addShortLinkHistory,
  clearShortLinkHistory,
  deleteShortLinkHistory,
  loadShortLinkHistory,
  type ShortLinkHistoryRecord,
  type ShortLinkHistoryResult,
} from "@/lib/shortLinkHistory";

const ERROR_FALLBACKS: Record<ShortLinkErrorCode, string> = {
  not_configured: "Configure a TinyURL API Token before generating a short link.",
  invalid_url: "Enter a valid HTTP or HTTPS URL.",
  authentication_failed: "TinyURL rejected the saved API Token. Replace it and try again.",
  rate_limited: "TinyURL rate limit reached. Wait a moment and try again.",
  request_rejected: "TinyURL rejected this request. Check the URL and try again.",
  service_unavailable: "TinyURL is currently unavailable. Try again later.",
  network_error: "Could not reach TinyURL. Check your network connection and try again.",
  invalid_response: "TinyURL returned an invalid response. Try again later.",
  storage_error: "Unable to access the encrypted TinyURL credential on this device.",
};

function isValidHttpUrl(value: string) {
  try {
    const parsed = new URL(value);
    return (parsed.protocol === "http:" || parsed.protocol === "https:") && Boolean(parsed.hostname);
  } catch {
    return false;
  }
}

export function ShortLinkTool() {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const confirm = useConfirmDialog();
  const { icon: ToolIcon, iconClassName } = getMoreToolPresentation("short-link");
  const [configured, setConfigured] = useState<boolean | null>(null);
  const [configOpen, setConfigOpen] = useState(false);
  const [configLoading, setConfigLoading] = useState(true);
  const [token, setToken] = useState("");
  const [tokenVisible, setTokenVisible] = useState(false);
  const [tokenError, setTokenError] = useState("");
  const [tokenSaving, setTokenSaving] = useState(false);
  const [tokenDeleting, setTokenDeleting] = useState(false);
  const [longUrl, setLongUrl] = useState("");
  const [urlError, setUrlError] = useState("");
  const [generating, setGenerating] = useState(false);
  const generatingRef = useRef(false);
  const [result, setResult] = useState<ShortLinkCreateResponse | null>(null);
  const [history, setHistory] = useState<ShortLinkHistoryRecord[]>([]);
  const [historyLoading, setHistoryLoading] = useState(true);
  const recoveryNotifiedRef = useRef(false);
  const [copiedTarget, setCopiedTarget] = useState<string | null>(null);
  const tokenVisibilityLabel = tokenVisible
    ? t("shortLinkTokenHide", "Hide Token")
    : t("shortLinkTokenShow", "Show Token");

  const messageForError = (error: unknown) => {
    const code = error instanceof ShortLinkError ? error.code : "unknown";
    if (code === "unknown") {
      return t(
        "shortLinkError_unknown",
        "Unable to complete the short-link operation. Try again.",
      );
    }
    return t(`shortLinkError_${code}`, ERROR_FALLBACKS[code]);
  };

  const notifyHistoryResult = (historyResult: ShortLinkHistoryResult) => {
    if (historyResult.status === "recovered" && !recoveryNotifiedRef.current) {
      recoveryNotifiedRef.current = true;
      pushToast({
        title: t(
          "shortLinkHistoryCorruptRecovered",
          "Damaged local history was discarded. You can continue creating short links.",
        ),
        kind: "warning",
      });
    } else if (historyResult.status === "failure") {
      pushToast({
        title:
          historyResult.error.code === "read_failed"
            ? t(
                "shortLinkHistoryReadFailed",
                "Unable to read local history. You can still create and copy short links.",
              )
            : t(
                "shortLinkHistoryWriteFailed",
                "The short link is available, but the local history update could not be saved.",
              ),
        kind: "error",
      });
    }
  };

  useEffect(() => {
    let active = true;

    void shortLinkConfigStatus()
      .then((status) => {
        if (!active) return;
        setConfigured(status.configured);
        setConfigOpen(!status.configured);
      })
      .catch((error: unknown) => {
        if (!active) return;
        setConfigured(false);
        setConfigOpen(true);
        pushToast({ title: messageForError(error), kind: "error" });
      })
      .finally(() => {
        if (active) setConfigLoading(false);
      });

    void Promise.resolve()
      .then(loadShortLinkHistory)
      .then((historyResult) => {
        if (!active) return;
        setHistory(historyResult.records);
        notifyHistoryResult(historyResult);
      })
      .catch(() => {
        if (!active) return;
        pushToast({
          title: t(
            "shortLinkHistoryReadFailed",
            "Unable to read local history. You can still create and copy short links.",
          ),
          kind: "error",
        });
      })
      .finally(() => {
        if (active) setHistoryLoading(false);
      });

    return () => {
      active = false;
    };
    // Initial configuration and history reads intentionally start together.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const saveToken = async () => {
    const nextToken = token.trim();
    if (!nextToken) {
      setTokenError(t("shortLinkTokenRequired", "Enter an API Token before saving."));
      return;
    }

    setTokenSaving(true);
    setTokenError("");
    try {
      const status = await shortLinkSaveToken(nextToken);
      setConfigured(status.configured);
      setConfigOpen(!status.configured);
      setToken("");
      setTokenVisible(false);
      pushToast({ title: t("shortLinkTokenSaved", "API Token saved."), kind: "success" });
    } catch (error) {
      pushToast({ title: messageForError(error), kind: "error" });
    } finally {
      setTokenSaving(false);
    }
  };

  const deleteToken = async () => {
    const accepted = await confirm(
      t(
        "shortLinkTokenDeleteConfirm",
        "Delete the saved Token from this device? Existing short links and local history will remain available.",
      ),
      {
        title: t("shortLinkTokenDeleteConfirmTitle", "Delete saved Token?"),
        okLabel: t("shortLinkTokenDelete", "Delete saved Token"),
        kind: "warning",
      },
    );
    if (!accepted) return;

    setTokenDeleting(true);
    try {
      const status = await shortLinkDeleteToken();
      setConfigured(status.configured);
      setConfigOpen(!status.configured);
      setToken("");
      setTokenVisible(false);
      pushToast({
        title: t(
          "shortLinkTokenDeleted",
          "Saved Token deleted from this device. Existing short links and local history were not changed.",
        ),
        kind: "success",
      });
    } catch (error) {
      pushToast({ title: messageForError(error), kind: "error" });
    } finally {
      setTokenDeleting(false);
    }
  };

  const generate = async () => {
    if (generatingRef.current) return;
    const nextUrl = longUrl.trim();
    if (!isValidHttpUrl(nextUrl)) {
      setUrlError(t("shortLinkError_invalid_url", ERROR_FALLBACKS.invalid_url));
      return;
    }

    generatingRef.current = true;
    setGenerating(true);
    setUrlError("");
    try {
      const response = await shortLinkCreate(nextUrl);
      setResult(response);

      const historyResult = addShortLinkHistory(response.longUrl, response.shortUrl);
      if (historyResult.status !== "failure") setHistory(historyResult.records);
      notifyHistoryResult(historyResult);
    } catch (error) {
      if (error instanceof ShortLinkError && error.code === "not_configured") {
        setConfigured(false);
        setConfigOpen(true);
      }
      pushToast({ title: messageForError(error), kind: "error" });
    } finally {
      generatingRef.current = false;
      setGenerating(false);
    }
  };

  const copyShortUrl = async (shortUrl: string, target: string) => {
    try {
      await navigator.clipboard.writeText(shortUrl);
      setCopiedTarget(target);
      pushToast({ title: t("shortLinkCopied", "Short link copied."), kind: "success" });
    } catch {
      pushToast({
        title: t(
          "shortLinkCopyFailed",
          "Unable to copy the short link to the clipboard.",
        ),
        kind: "error",
      });
    }
  };

  const deleteHistoryRecord = async (record: ShortLinkHistoryRecord) => {
    const accepted = await confirm(
      t(
        "shortLinkHistoryDeleteConfirm",
        "Delete this record from this device? The TinyURL link will remain active remotely.",
      ),
      {
        title: t("shortLinkHistoryDeleteConfirmTitle", "Delete local record?"),
        okLabel: t("shortLinkHistoryDelete", "Delete local record"),
        kind: "warning",
      },
    );
    if (!accepted) return;

    const historyResult = deleteShortLinkHistory(record.id);
    if (historyResult.status === "failure") {
      notifyHistoryResult(historyResult);
      return;
    }
    setHistory(historyResult.records);
    notifyHistoryResult(historyResult);
    pushToast({
      title: t(
        "shortLinkHistoryDeleted",
        "Local record deleted. The TinyURL link remains active remotely.",
      ),
      kind: "success",
    });
  };

  const clearHistory = async () => {
    const accepted = await confirm(
      t(
        "shortLinkHistoryClearConfirm",
        "Clear all short-link records from this device? TinyURL links will remain active remotely.",
      ),
      {
        title: t("shortLinkHistoryClearConfirmTitle", "Clear local history?"),
        okLabel: t("shortLinkHistoryClear", "Clear local history"),
        kind: "warning",
      },
    );
    if (!accepted) return;

    const historyResult = clearShortLinkHistory();
    if (historyResult.status === "failure") {
      notifyHistoryResult(historyResult);
      return;
    }
    setHistory(historyResult.records);
    pushToast({
      title: t(
        "shortLinkHistoryCleared",
        "Local history cleared. TinyURL links remain active remotely.",
      ),
      kind: "success",
    });
  };

  return (
    <section className="space-y-5 pb-5" aria-labelledby="short-link-title">
      <div className="flex items-start gap-3">
        <div className={`rounded-lg p-2 ${iconClassName}`}>
          <ToolIcon className="h-5 w-5" />
        </div>
        <div>
          <h2 id="short-link-title" className="text-lg font-semibold">
            {t("shortLink", "Short Link")}
          </h2>
          <p className="text-sm text-muted-foreground">
            {t(
              "shortLinkToolDesc",
              "Create TinyURL short links and keep recent results on this device.",
            )}
          </p>
        </div>
      </div>

      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(18rem,0.75fr)]">
        <div className="space-y-5">
          <section className="space-y-4 rounded-lg border bg-card p-4" aria-labelledby="short-link-credentials-title">
            <div>
              <h3 id="short-link-credentials-title" className="text-sm font-semibold">
                {t("shortLinkCredentialTitle", "TinyURL credentials")}
              </h3>
              <p className="mt-1 text-sm text-muted-foreground">
                {t(
                  "shortLinkCredentialDesc",
                  "Your API Token is encrypted on this device. A saved Token is never displayed again.",
                )}
              </p>
            </div>

            {configLoading ? (
              <p className="text-sm text-muted-foreground">
                {t("shortLinkCredentialStatusLoading", "Checking credential status...")}
              </p>
            ) : (
              <div className="flex flex-wrap items-center justify-between gap-3">
                <span className="inline-flex items-center gap-2 text-sm font-medium">
                  <span
                    className={`h-2 w-2 rounded-full ${configured ? "bg-emerald-500" : "bg-amber-500"}`}
                    aria-hidden="true"
                  />
                  {configured
                    ? t("shortLinkTokenConfigured", "API Token configured")
                    : t("shortLinkTokenNotConfigured", "API Token not configured")}
                </span>
                {configured ? (
                  <div className="flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={() => {
                        setToken("");
                        setTokenError("");
                        setTokenVisible(false);
                        setConfigOpen(true);
                      }}
                      disabled={tokenSaving || tokenDeleting}
                      className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm font-medium hover:bg-muted disabled:opacity-50"
                    >
                      <RefreshCw className="h-4 w-4" />
                      {t("shortLinkTokenReplace", "Replace Token")}
                    </button>
                    <button
                      type="button"
                      onClick={() => void deleteToken()}
                      disabled={tokenSaving || tokenDeleting}
                      className="inline-flex h-9 items-center gap-2 rounded-md border border-destructive/30 px-3 text-sm font-medium text-destructive hover:bg-destructive/10 disabled:opacity-50"
                    >
                      {tokenDeleting ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <Trash2 className="h-4 w-4" />
                      )}
                      {t("shortLinkTokenDelete", "Delete saved Token")}
                    </button>
                  </div>
                ) : null}
              </div>
            )}

            {!configLoading && (!configured || configOpen) ? (
              <form
                className="space-y-2"
                onSubmit={(event) => {
                  event.preventDefault();
                  void saveToken();
                }}
              >
                <label className="grid gap-1.5 text-sm font-medium" htmlFor="short-link-token">
                  {t("shortLinkTokenLabel", "TinyURL API Token")}
                  <span className="flex h-10 overflow-hidden rounded-md border bg-background focus-within:ring-2 focus-within:ring-ring">
                    <input
                      id="short-link-token"
                      type={tokenVisible ? "text" : "password"}
                      value={token}
                      onChange={(event) => {
                        setToken(event.target.value);
                        setTokenError("");
                      }}
                      autoComplete="off"
                      placeholder={t("shortLinkTokenPlaceholder", "Enter a TinyURL API Token")}
                      className="min-w-0 flex-1 bg-transparent px-3 text-sm outline-none"
                    />
                    <button
                      type="button"
                      onClick={() => setTokenVisible((visible) => !visible)}
                      className="inline-flex w-10 shrink-0 items-center justify-center border-l text-muted-foreground hover:bg-muted hover:text-foreground"
                      aria-label={tokenVisibilityLabel}
                      title={tokenVisibilityLabel}
                    >
                      {tokenVisible ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                    </button>
                  </span>
                </label>
                {tokenError ? (
                  <p role="alert" className="text-sm text-destructive">
                    {tokenError}
                  </p>
                ) : null}
                <button
                  type="submit"
                  disabled={tokenSaving || tokenDeleting}
                  className="inline-flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                >
                  {tokenSaving ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <Save className="h-4 w-4" />
                  )}
                  {tokenSaving
                    ? t("shortLinkTokenSaving", "Saving...")
                    : t("shortLinkTokenSave", "Save Token")}
                </button>
              </form>
            ) : null}
          </section>

          <section className="space-y-4 rounded-lg border bg-card p-4" aria-labelledby="short-link-generate-title">
            <h3 id="short-link-generate-title" className="text-sm font-semibold">
              {t("shortLinkGenerate", "Generate short link")}
            </h3>
            <form
              className="space-y-2"
              noValidate
              onSubmit={(event) => {
                event.preventDefault();
                void generate();
              }}
            >
              <label className="grid gap-1.5 text-sm font-medium" htmlFor="short-link-long-url">
                {t("shortLinkLongUrlLabel", "Long URL")}
                <input
                  id="short-link-long-url"
                  type="url"
                  value={longUrl}
                  onChange={(event) => {
                    setLongUrl(event.target.value);
                    setUrlError("");
                  }}
                  placeholder={t("shortLinkLongUrlPlaceholder", "https://example.com/path")}
                  className="h-10 rounded-md border bg-background px-3 text-sm outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring"
                />
              </label>
              {urlError ? (
                <p role="alert" className="text-sm text-destructive">
                  {urlError}
                </p>
              ) : null}
              <button
                type="submit"
                disabled={generating}
                className="inline-flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
              >
                {generating ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <ToolIcon className="h-4 w-4" />
                )}
                {generating
                  ? t("shortLinkGenerating", "Generating...")
                  : t("shortLinkGenerate", "Generate short link")}
              </button>
            </form>

            {result ? (
              <div className="space-y-3 border-t pt-4" data-testid="short-link-current-result">
                <h4 className="text-sm font-semibold">
                  {t("shortLinkCurrentResult", "Current result")}
                </h4>
                <div className="grid gap-2 text-sm">
                  <div>
                    <span className="font-medium">
                      {t("shortLinkOriginalUrl", "Original URL")}
                    </span>
                    <p className="break-all text-muted-foreground">{result.longUrl}</p>
                  </div>
                  <div className="flex items-start gap-2">
                    <a
                      href={result.shortUrl}
                      target="_blank"
                      rel="noreferrer"
                      className="min-w-0 flex-1 break-all font-medium text-primary underline-offset-4 hover:underline"
                    >
                      {result.shortUrl}
                    </a>
                    <button
                      type="button"
                      onClick={() => void copyShortUrl(result.shortUrl, "current")}
                      className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md border text-muted-foreground hover:bg-muted hover:text-foreground"
                      aria-label={t("shortLinkCopy", "Copy short link")}
                      title={t("shortLinkCopy", "Copy short link")}
                    >
                      {copiedTarget === "current" ? (
                        <Check className="h-4 w-4 text-emerald-600" />
                      ) : (
                        <Copy className="h-4 w-4" />
                      )}
                    </button>
                  </div>
                </div>
              </div>
            ) : null}
          </section>
        </div>

        <section className="min-w-0 rounded-lg border bg-card p-4" aria-labelledby="short-link-history-title">
          <div className="flex items-center justify-between gap-3">
            <h3 id="short-link-history-title" className="text-sm font-semibold">
              {t("shortLinkHistory", "Local history")}
            </h3>
            {history.length ? (
              <button
                type="button"
                onClick={() => void clearHistory()}
                className="inline-flex h-8 items-center gap-1.5 rounded-md px-2 text-xs font-medium text-muted-foreground hover:bg-muted hover:text-foreground"
              >
                <Trash2 className="h-3.5 w-3.5" />
                {t("shortLinkHistoryClear", "Clear local history")}
              </button>
            ) : null}
          </div>
          <p className="mt-2 text-xs text-muted-foreground">
            {t(
              "shortLinkHistoryLocalBoundary",
              "History is stored only on this device. Deleting local records does not delete or disable TinyURL links.",
            )}
          </p>

          {historyLoading ? (
            <p className="mt-4 text-sm text-muted-foreground">
              {t("shortLinkHistoryLoading", "Loading local short-link history...")}
            </p>
          ) : history.length ? (
            <ul className="mt-4 max-h-[32rem] space-y-3 overflow-y-auto" aria-label={t("shortLinkHistory", "Local history")}>
              {history.map((record) => (
                <li
                  key={record.id}
                  data-testid="short-link-history-item"
                  className="space-y-2 rounded-md border p-3 text-sm"
                >
                  <a
                    href={record.shortUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="block break-all font-medium text-primary underline-offset-4 hover:underline"
                  >
                    {record.shortUrl}
                  </a>
                  <p className="break-all text-xs text-muted-foreground">{record.longUrl}</p>
                  <div className="flex items-center justify-between gap-2">
                    <time className="text-xs text-muted-foreground" dateTime={record.createdAt}>
                      {new Date(record.createdAt).toLocaleString()}
                    </time>
                    <div className="flex gap-1">
                      <button
                        type="button"
                        onClick={() => void copyShortUrl(record.shortUrl, record.id)}
                        className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
                        aria-label={t("shortLinkHistoryCopy", "Copy history item")}
                        title={t("shortLinkHistoryCopy", "Copy history item")}
                      >
                        {copiedTarget === record.id ? (
                          <Check className="h-4 w-4 text-emerald-600" />
                        ) : (
                          <Copy className="h-4 w-4" />
                        )}
                      </button>
                      <button
                        type="button"
                        onClick={() => void deleteHistoryRecord(record)}
                        className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                        aria-label={t("shortLinkHistoryDelete", "Delete local record")}
                        title={t("shortLinkHistoryDelete", "Delete local record")}
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <p className="mt-4 text-sm text-muted-foreground">
              {t(
                "shortLinkHistoryEmpty",
                "Short links created on this device will appear here.",
              )}
            </p>
          )}
        </section>
      </div>
    </section>
  );
}
