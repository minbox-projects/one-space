import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { TFunction } from "i18next";
import {
  AlertTriangle,
  CheckCircle2,
  Loader2,
  Rocket,
  Server,
} from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { CustomSelect } from "@/components/common/CustomSelect";
import {
  CLOUD_AGENT_DEPLOY_PROGRESS_EVENT,
  cloudAgentDeploy,
  cloudAgentGetInstallationInfo,
  compareSemanticVersions,
  formatInstalledAt,
  mapDeploymentProgress,
  type CloudAgentDeployPhase,
  type CloudAgentDeploymentProgress,
  type CloudAgentDeploymentResult,
  type CloudAgentInstallationInfo,
  type CloudAgentInstallationState,
} from "@/lib/cloudAgent";

export interface CloudAgentHostLike {
  id?: string;
  name?: string;
  label?: string;
}

export interface CloudAgentDeploymentDialogProps {
  t: TFunction;
  isOpen: boolean;
  onClose: () => void;
  onInstalled: (result: CloudAgentDeploymentResult) => void;
  onDeploymentStart?: (host: string) => void;
  onDeploymentEnd?: (host: string) => void;
  hosts?: CloudAgentHostLike[];
  selectedHost?: string;
}

type DisplayState = CloudAgentInstallationState | "checking";

const STATE_LABEL_KEYS: Record<DisplayState, string> = {
  installed: "deploymentStateInstalled",
  not_installed: "deploymentStateNotInstalled",
  incomplete: "deploymentStateIncomplete",
  probe_error: "deploymentStateProbeError",
  checking: "deploymentStateChecking",
};

function createOperationId(): string {
  const cryptoObject =
    typeof globalThis !== "undefined"
      ? (globalThis.crypto as Crypto | undefined)
      : undefined;
  if (cryptoObject && typeof cryptoObject.randomUUID === "function") {
    return cryptoObject.randomUUID();
  }
  return `deploy-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

function normalizeHosts(hosts?: CloudAgentHostLike[]) {
  const seen = new Set<string>();
  const options: { value: string; label: string }[] = [];
  for (const host of hosts ?? []) {
    const value = host.id ?? host.name;
    if (!value || seen.has(value)) continue;
    seen.add(value);
    options.push({ value, label: host.label ?? value });
  }
  return options;
}

function failedPhaseFromMessage(message: string): CloudAgentDeployPhase | null {
  const match = message.match(/phase\s+([a-z_]+)/i);
  if (!match) return null;
  return match[1].toLowerCase() as CloudAgentDeployPhase;
}

export function CloudAgentDeploymentDialog({
  t,
  isOpen,
  onClose,
  onInstalled,
  onDeploymentStart,
  onDeploymentEnd,
  hosts,
  selectedHost,
}: CloudAgentDeploymentDialogProps) {
  const language = (t as TFunction & { i18n?: { language?: string } }).i18n
    ?.language;
  const hostOptions = useMemo(() => normalizeHosts(hosts), [hosts]);
  const defaultTarget = useMemo(() => {
    if (selectedHost && hostOptions.some((host) => host.value === selectedHost)) {
      return selectedHost;
    }
    return hostOptions[0]?.value ?? "";
  }, [hostOptions, selectedHost]);

  const [target, setTarget] = useState("");
  const [installation, setInstallation] =
    useState<CloudAgentInstallationInfo | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [inspectionError, setInspectionError] = useState<string | null>(null);
  const [deploying, setDeploying] = useState(false);
  const [phases, setPhases] = useState<CloudAgentDeploymentProgress[]>([]);
  const [deployError, setDeployError] = useState<string | null>(null);
  const [failedPhase, setFailedPhase] = useState<CloudAgentDeployPhase | null>(
    null
  );

  const wasOpenRef = useRef(false);
  const inspectingRef = useRef(0);
  const targetRef = useRef("");
  const operationRef = useRef<string | null>(null);
  const deployingRef = useRef(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    targetRef.current = target;
  }, [target]);

  useEffect(() => {
    const wasOpen = wasOpenRef.current;
    wasOpenRef.current = isOpen;
    if (!isOpen) return;
    if (!wasOpen) {
      inspectingRef.current += 1;
      setTarget(defaultTarget);
      setInstallation(null);
      setInspectionError(null);
      setInspecting(Boolean(defaultTarget));
      setDeploying(false);
      setPhases([]);
      setDeployError(null);
      setFailedPhase(null);
      operationRef.current = null;
      deployingRef.current = false;
    } else if (!target && defaultTarget) {
      setTarget(defaultTarget);
    }
  }, [isOpen, defaultTarget, target]);

  useEffect(() => {
    if (!isOpen || !target) {
      setInspecting(false);
      return;
    }
    const requestId = ++inspectingRef.current;
    let cancelled = false;
    setInspecting(true);
    setInstallation(null);
    setInspectionError(null);

    cloudAgentGetInstallationInfo(target)
      .then((info) => {
        if (cancelled || requestId !== inspectingRef.current) return;
        setInstallation(info);
      })
      .catch((error: unknown) => {
        if (cancelled || requestId !== inspectingRef.current) return;
        setInstallation(null);
        setInspectionError(
          error instanceof Error ? error.message : String(error)
        );
      })
      .finally(() => {
        if (cancelled || requestId !== inspectingRef.current) return;
        setInspecting(false);
      });

    return () => {
      cancelled = true;
    };
  }, [isOpen, target]);

  useEffect(() => {
    if (!isOpen) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listen(
      CLOUD_AGENT_DEPLOY_PROGRESS_EVENT,
      (event: { payload?: unknown }) => {
        if (disposed) return;
        const progress = mapDeploymentProgress(event?.payload);
        if (!progress.phase || !progress.status) return;
        if (progress.host !== targetRef.current) return;
        if (!operationRef.current || progress.operationId !== operationRef.current) {
          return;
        }
        setPhases((previous) => {
          const index = previous.findIndex(
            (item) => item.phase === progress.phase
          );
          if (index === -1) return [...previous, progress];
          const next = [...previous];
          next[index] = progress;
          return next;
        });
        if (progress.status === "error") {
          setFailedPhase(progress.phase);
          if (progress.reason) {
            setDeployError(progress.reason);
          }
        }
      }
    ).then((dispose) => {
      if (disposed) {
        dispose();
      } else {
        unlisten = dispose;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [isOpen]);

  const displayState: DisplayState = inspecting
    ? "checking"
    : inspectionError
    ? "probe_error"
    : installation?.state ?? "checking";

  const canDeploy =
    !deploying &&
    !inspectionError &&
    Boolean(target) &&
    !installation?.configurationError &&
    (displayState === "not_installed" || displayState === "incomplete");

  const handleClose = useCallback(() => {
    if (deployingRef.current) return;
    onClose();
  }, [onClose]);

  const handleSubmit = async () => {
    if (deployingRef.current) return;
    if (!canDeploy || !target) return;

    const capturedHost = target;
    const operationId = createOperationId();
    operationRef.current = operationId;
    deployingRef.current = true;
    setDeploying(true);
    setPhases([]);
    setDeployError(null);
    setFailedPhase(null);
    onDeploymentStart?.(capturedHost);

    try {
      const result = await cloudAgentDeploy(capturedHost, operationId);
      if (!result.success) {
        throw new Error(
          result.reason || `deployment failed in phase ${result.phase}`
        );
      }
      if (!mountedRef.current) return;
      onInstalled({
        ...result,
        host: result.host || capturedHost,
        operationId: result.operationId || operationId,
      });
      onClose();
    } catch (error: unknown) {
      if (!mountedRef.current) return;
      const message = error instanceof Error ? error.message : String(error);
      setDeployError(message);
      setFailedPhase(failedPhaseFromMessage(message));
    } finally {
      deployingRef.current = false;
      if (mountedRef.current) {
        setDeploying(false);
      }
      operationRef.current = null;
      onDeploymentEnd?.(capturedHost);
    }
  };

  const remoteVersion = installation?.remoteVersion || null;
  const localVersion = installation?.localLatestVersion || null;
  const comparison =
    remoteVersion && localVersion
      ? compareSemanticVersions(remoteVersion, localVersion)
      : "unknown";
  const unknownLabel = t("installationUnknown", "Unknown");

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) handleClose();
      }}
    >
      <DialogContent
        className="max-w-xl"
        data-testid="cloud-agent-deployment-dialog"
      >
        <DialogHeader>
          <div className="flex items-center gap-2.5">
            <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <Rocket className="h-5 w-5" />
            </div>
            <div>
              <DialogTitle className="text-base font-semibold">
                {t("deploymentDialogTitle", "Deploy Cloud Agent")}
              </DialogTitle>
              <DialogDescription className="mt-0.5 text-xs text-muted-foreground">
                {t(
                  "deployServerHint",
                  "Inspect a remote SSH host and deploy the bundled Cloud Agent."
                )}
              </DialogDescription>
            </div>
          </div>
        </DialogHeader>

        {hostOptions.length === 0 ? (
          <div className="space-y-3 rounded-lg border border-border bg-muted/30 p-4 text-xs text-muted-foreground">
            <div className="flex items-start gap-2">
              <Server className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
              <p>{t("deploymentHostEmpty", "Configure an SSH host first.")}</p>
            </div>
            <code className="block rounded bg-background px-2 py-1.5 font-mono text-[11px] text-foreground">
              ssh-copy-id user@host
            </code>
          </div>
        ) : (
          <div className="space-y-4">
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-foreground">
                {t("deploymentTargetLabel", "Target SSH Host")}
              </label>
              <CustomSelect
                value={target}
                onChange={setTarget}
                options={hostOptions}
                disabled={deploying}
                placeholder={t("cloudAgentSelectHostPlaceholder", "Select Host")}
                ariaLabel={t("deploymentTargetLabel", "Target SSH Host")}
                selectTestId="cloud-agent-deploy-host-select"
              />
            </div>

            {target ? (
              <>
                <div
                  data-testid="cloud-agent-deployment-state"
                  data-state={displayState}
                  className="flex items-center gap-2 rounded-lg border border-border/80 bg-muted/30 px-3 py-2 text-xs font-medium text-foreground"
                >
                  {displayState === "checking" ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
                  ) : displayState === "installed" ? (
                    <CheckCircle2 className="h-3.5 w-3.5 text-emerald-500" />
                  ) : (
                    <AlertTriangle className="h-3.5 w-3.5 text-amber-500" />
                  )}
                  <span>
                    {t(STATE_LABEL_KEYS[displayState], displayState)}
                  </span>
                  {comparison !== "unknown" && installation ? (
                    <span className="ml-auto rounded-full bg-background px-2 py-0.5 text-[10px] font-semibold text-muted-foreground">
                      {t(
                        comparison === "older"
                          ? "installationVersionOlder"
                          : comparison === "newer"
                          ? "installationVersionNewer"
                          : "installationVersionEqual",
                        comparison
                      )}
                    </span>
                  ) : null}
                </div>

                <div className="grid gap-2 sm:grid-cols-3">
                  <div className="rounded-lg border border-border/60 bg-background p-2.5">
                    <div className="text-[11px] text-muted-foreground">
                      {t("installationRemoteVersion", "Remote version")}
                    </div>
                    <div
                      data-testid="cloud-agent-remote-version"
                      className="mt-1 truncate font-mono text-xs text-foreground"
                    >
                      {remoteVersion || unknownLabel}
                    </div>
                  </div>
                  <div className="rounded-lg border border-border/60 bg-background p-2.5">
                    <div className="text-[11px] text-muted-foreground">
                      {t("installationLocalVersion", "Local latest")}
                    </div>
                    <div
                      data-testid="cloud-agent-local-version"
                      className="mt-1 truncate font-mono text-xs text-foreground"
                    >
                      {localVersion || unknownLabel}
                    </div>
                  </div>
                  <div className="rounded-lg border border-border/60 bg-background p-2.5">
                    <div className="text-[11px] text-muted-foreground">
                      {t("installationInstalledAt", "Installed at")}
                    </div>
                    <div
                      data-testid="cloud-agent-installed-at"
                      className="mt-1 truncate text-xs text-foreground"
                    >
                      {formatInstalledAt(installation?.installedAt, language)}
                    </div>
                  </div>
                </div>

                <div
                  data-testid="cloud-agent-deploy-requirements"
                  className="space-y-1 rounded-lg border border-border/60 bg-muted/20 p-3 text-[11px] text-muted-foreground"
                >
                  <p>
                    {t(
                      "deploymentRequirements",
                      "Requires Linux, root access and systemd."
                    )}
                  </p>
                  <p className="font-mono text-[10px] text-foreground/70">
                    Linux · root · systemd · Ubuntu 22.04/24.04 · Debian 12
                  </p>
                </div>

                {displayState === "probe_error" ? (
                  <div className="rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-[11px] text-amber-700 dark:text-amber-300">
                    {t(
                      "deploymentConflictHelp",
                      "Resolve the path conflict or repair the installation manually."
                    )}
                    {inspectionError ? (
                      <div className="mt-1 font-mono text-[10px] opacity-80">
                        {inspectionError}
                      </div>
                    ) : null}
                  </div>
                ) : null}

                {deploying || phases.length > 0 ? (
                  <div
                    data-testid="cloud-agent-deploy-progress"
                    className="space-y-2 rounded-lg border border-border/60 bg-background p-3"
                  >
                    <div className="flex items-center gap-2 text-xs font-medium text-foreground">
                      {deploying ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin text-primary" />
                      ) : (
                        <CheckCircle2 className="h-3.5 w-3.5 text-muted-foreground" />
                      )}
                      {t("deploymentProgressTitle", "Deployment progress")}
                    </div>
                    <ol className="space-y-1">
                      {phases.map((phase) => (
                        <li
                          key={phase.phase}
                          data-testid="cloud-agent-deploy-phase"
                          data-phase={phase.phase}
                          data-status={phase.status}
                          className="flex items-center gap-2 text-[11px] text-muted-foreground"
                        >
                          <span
                            className={`h-1.5 w-1.5 rounded-full ${
                              phase.status === "error"
                                ? "bg-rose-500"
                                : phase.status === "ok"
                                ? "bg-emerald-500"
                                : "bg-amber-500"
                            }`}
                          />
                          <span className="font-mono text-foreground/80">
                            {phase.phase}
                          </span>
                          {phase.reason ? (
                            <span className="truncate opacity-70">
                              {phase.reason}
                            </span>
                          ) : null}
                        </li>
                      ))}
                    </ol>
                  </div>
                ) : null}

                {deployError ? (
                  <div
                    data-testid="cloud-agent-deploy-error"
                    className="rounded-lg border border-destructive/20 bg-destructive/10 p-3 text-[11px] text-destructive"
                  >
                    <div className="font-medium">
                      {t("deploymentFailedTitle", "Deployment failed")}
                      {failedPhase ? ` · ${failedPhase}` : ""}
                    </div>
                    <div className="mt-1 break-words font-mono text-[10px]">
                      {deployError}
                    </div>
                  </div>
                ) : null}
              </>
            ) : null}
          </div>
        )}

        <DialogFooter className="gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            data-testid="cloud-agent-deploy-close"
            disabled={deploying}
            onClick={handleClose}
          >
            {t("deploymentClose", "Close")}
          </Button>
          {hostOptions.length > 0 ? (
            <Button
              type="button"
              size="sm"
              data-testid="cloud-agent-deploy-submit"
              disabled={!canDeploy}
              onClick={() => void handleSubmit()}
            >
              {deploying ? (
                <>
                  <Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" />
                  {t("deploymentInProgress", "Deploying...")}
                </>
              ) : displayState === "installed" ? (
                t("deploymentInstalledAction", "Installed")
              ) : (
                t("deploymentDeployAction", "Deploy")
              )}
            </Button>
          ) : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
