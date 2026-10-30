import { useEffect, useRef, useState } from "react";
import type { TFunction } from "i18next";
import {
  AlertCircle,
  CheckCircle2,
  FolderGit2,
  GitBranch,
  Loader2,
  Shield,
  Upload,
} from "lucide-react";
import { CustomSelect } from "@/components/common/CustomSelect";
import {
  workspacesList,
  cloudAgentPlanContext,
  cloudAgentPlanSubmit,
} from "@/lib/cloudAgent";
import type {
  CloudPlanRepository,
  CloudPlanSubmitResult,
  LocalPlanContext,
  LocalPlanSummary,
} from "@/lib/cloudAgent";

interface LocalPlanFormProps {
  t: TFunction;
  host?: string;
  onSubmitted: (result: CloudPlanSubmitResult) => void;
}

interface WorkspaceOption {
  id: string;
  name: string;
  rootPath: string;
}

function normalizeWorkspaces(items: unknown): WorkspaceOption[] {
  if (!Array.isArray(items)) return [];
  return items
    .map((item) => {
      const candidate =
        item && typeof item === "object" && "workspace" in item
          ? (item as { workspace?: unknown }).workspace
          : item;
      if (!candidate || typeof candidate !== "object") return null;
      const record = candidate as Record<string, unknown>;
      const id = typeof record.id === "string" ? record.id : "";
      if (!id) return null;
      const name =
        typeof record.name === "string" && record.name.trim()
          ? record.name
          : id;
      const rootPath =
        typeof record.root_path === "string" ? record.root_path : "";
      return { id, name, rootPath };
    })
    .filter((item): item is WorkspaceOption => item !== null);
}

function planErrorText(plan: LocalPlanSummary): string {
  return plan.error ? String(plan.error) : plan.status;
}

function repositoryLabel(repository: CloudPlanRepository): string {
  return (
    repository.canonical ||
    repository.repo ||
    repository.remote_url ||
    repository.host ||
    ""
  );
}

function newRequestId(): string {
  const cryptoApi = globalThis.crypto;
  if (cryptoApi && typeof cryptoApi.randomUUID === "function") {
    return cryptoApi.randomUUID();
  }
  return `plan-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function LocalPlanForm({ t, host, onSubmitted }: LocalPlanFormProps) {
  const [workspaces, setWorkspaces] = useState<WorkspaceOption[]>([]);
  const [workspaceId, setWorkspaceId] = useState("");
  const [workspacesLoading, setWorkspacesLoading] = useState(true);
  const [workspacesError, setWorkspacesError] = useState<string | null>(null);
  const [context, setContext] = useState<LocalPlanContext | null>(null);
  const [contextLoading, setContextLoading] = useState(false);
  const [contextError, setContextError] = useState<string | null>(null);
  const [selectedPlan, setSelectedPlan] = useState<LocalPlanSummary | null>(
    null
  );
  const [baseBranch, setBaseBranch] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [result, setResult] = useState<CloudPlanSubmitResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const hostRef = useRef(host);
  const requestSeqRef = useRef(0);
  const submittingRef = useRef(false);
  const completedKeyRef = useRef<string | null>(null);
  const requestIdRef = useRef<{ key: string; id: string } | null>(null);

  useEffect(() => {
    let cancelled = false;
    setWorkspacesLoading(true);
    workspacesList()
      .then((items) => {
        if (cancelled) return;
        const next = normalizeWorkspaces(items);
        setWorkspaces(next);
        setWorkspacesError(null);
        setWorkspaceId((previous) => previous || next[0]?.id || "");
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setWorkspacesError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setWorkspacesLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (hostRef.current === host) return;
    hostRef.current = host;
    requestSeqRef.current += 1;
    setContext(null);
    setSelectedPlan(null);
    setBaseBranch("");
    setResult(null);
    setError(null);
    setContextError(null);
    setContextLoading(false);
    completedKeyRef.current = null;
    requestIdRef.current = null;
  }, [host]);

  useEffect(() => {
    if (!workspaceId) {
      setContext(null);
      return;
    }

    const seq = ++requestSeqRef.current;
    const requestHost = hostRef.current;
    setContextLoading(true);
    setContextError(null);
    setContext(null);
    setSelectedPlan(null);
    setBaseBranch("");
    setResult(null);
    setError(null);

    cloudAgentPlanContext(workspaceId)
      .then((next) => {
        if (seq !== requestSeqRef.current || requestHost !== hostRef.current) {
          return;
        }
        setContext(next);
        setBaseBranch(
          next.repository?.base_branch || next.repository?.branch || ""
        );
      })
      .catch((err: unknown) => {
        if (seq !== requestSeqRef.current || requestHost !== hostRef.current) {
          return;
        }
        setContext(null);
        setContextError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (seq === requestSeqRef.current && requestHost === hostRef.current) {
          setContextLoading(false);
        }
      });
  }, [workspaceId]);

  const handleWorkspaceChange = (nextWorkspaceId: string) => {
    if (nextWorkspaceId === workspaceId) return;
    requestSeqRef.current += 1;
    setWorkspaceId(nextWorkspaceId);
    setContext(null);
    setSelectedPlan(null);
    setBaseBranch("");
    setResult(null);
    setError(null);
    setContextError(null);
    setContextLoading(false);
    completedKeyRef.current = null;
  };

  const handleSubmit = async () => {
    if (!workspaceId || !selectedPlan) return;
    if (submittingRef.current) return;

    const key = `${workspaceId}:${selectedPlan.plan_id}`;
    if (completedKeyRef.current === key) return;

    if (!requestIdRef.current || requestIdRef.current.key !== key) {
      requestIdRef.current = { key, id: newRequestId() };
    }
    const requestId = requestIdRef.current.id;

    submittingRef.current = true;
    setSubmitting(true);
    setError(null);
    setResult(null);

    try {
      const submitted = await cloudAgentPlanSubmit({
        workspace_id: workspaceId,
        plan_directory: selectedPlan.directory || selectedPlan.plan_id,
        base_branch: baseBranch.trim() || undefined,
        request_id: requestId,
        host,
      });
      completedKeyRef.current = key;
      setResult(submitted);
      onSubmitted(submitted);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  };

  const plans = context?.plans ?? [];
  const repository = context?.repository ?? null;
  const selectedId = selectedPlan?.plan_id ?? "";
  const submitDisabled = !selectedPlan || submitting;

  return (
    <div className="space-y-4 pt-1">
      <div className="space-y-1.5">
        <div className="text-xs font-medium text-foreground">
          {t("localPlanWorkspaceLabel", "保存的工作区")}
        </div>
        <CustomSelect
          value={workspaceId}
          onChange={handleWorkspaceChange}
          options={workspaces.map((workspace) => ({
            value: workspace.id,
            label: workspace.name,
          }))}
          selectTestId="local-plan-workspace-select"
          ariaLabel={t("localPlanWorkspaceLabel", "保存的工作区")}
          placeholder={t("localPlanWorkspacePlaceholder", "选择工作区")}
          disabled={workspacesLoading}
        />
        {workspacesLoading ? (
          <p className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
            <Loader2 className="h-3 w-3 animate-spin" />
            {t("localPlanWorkspacesLoading", "正在加载工作区...")}
          </p>
        ) : null}
        {workspacesError ? (
          <p className="text-[11px] text-destructive">
            {t("localPlanWorkspaceLoadFailed", "加载工作区失败: {{message}}", {
              message: workspacesError,
            })}
          </p>
        ) : null}
        {!workspacesLoading && !workspacesError && workspaces.length === 0 ? (
          <p className="text-[11px] text-muted-foreground">
            {t(
              "localPlanNoWorkspaces",
              "暂无保存的工作区，请先在「Workspaces」中创建。"
            )}
          </p>
        ) : null}
      </div>

      {contextLoading ? (
        <div className="flex items-center gap-2 rounded-lg border border-border bg-muted/30 p-3 text-xs text-muted-foreground">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t("localPlanContextLoading", "正在读取仓库与本地计划...")}
        </div>
      ) : null}

      {contextError ? (
        <div
          className="rounded-lg border border-destructive/20 bg-destructive/10 p-3 text-xs text-destructive"
          role="alert"
        >
          {contextError}
        </div>
      ) : null}

      {repository ? (
        <div
          className="space-y-2 rounded-lg border border-border bg-muted/30 p-3"
          data-testid="local-plan-repository"
        >
          <div className="flex items-center gap-1.5 text-xs font-medium text-foreground">
            <FolderGit2 className="h-3.5 w-3.5 text-muted-foreground" />
            {t("localPlanRepositoryLabel", "远端仓库")}
            <span className="font-mono text-[11px] text-muted-foreground">
              {repositoryLabel(repository)}
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
            <span className="inline-flex items-center gap-1">
              <GitBranch className="h-3 w-3" />
              {t("localPlanBaseBranchValue", "默认基线")}:{" "}
              <span className="font-mono">
                {repository.base_branch || repository.branch || "—"}
              </span>
            </span>
            <span>
              {t("localPlanSourceLabel", "来源")}:{" "}
              <span className="font-mono">{repository.source || "remote"}</span>
            </span>
            {repository.head_sha ? (
              <span className="font-mono">
                {repository.head_sha.slice(0, 7)}
              </span>
            ) : null}
          </div>
          <p className="text-[11px] text-muted-foreground">
            {t(
              "localPlanRemoteOnlyNote",
              "云端只使用远端仓库的最新代码；本地未提交修改与未推送提交不会上传，仅上传计划文件。"
            )}
          </p>
          <div className="space-y-1.5">
            <label
              htmlFor="local-plan-base-branch"
              className="text-[11px] font-medium text-foreground"
            >
              {t("localPlanBaseBranchLabel", "目标基线分支")}
            </label>
            <input
              id="local-plan-base-branch"
              data-testid="local-plan-base-branch"
              type="text"
              value={baseBranch}
              onChange={(event) => setBaseBranch(event.target.value)}
              placeholder="main"
              className="w-full rounded-lg border border-input bg-background px-3 py-2 font-mono text-xs text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            />
          </div>
        </div>
      ) : null}

      {context ? (
        <div className="space-y-1.5">
          <div className="text-xs font-medium text-foreground">
            {t("localPlanListLabel", "本地计划")}
          </div>
          {context.plans_error ? (
            <p className="rounded-lg border border-destructive/20 bg-destructive/10 p-2 text-[11px] text-destructive">
              {context.plans_error}
            </p>
          ) : plans.length === 0 ? (
            <p className="text-[11px] text-muted-foreground">
              {t("localPlanNoPlans", "该工作区暂无本地计划。")}
            </p>
          ) : (
            <div
              className="space-y-1.5"
              role="group"
              aria-label={t("localPlanListLabel", "本地计划")}
            >
              {plans.map((plan) => {
                const selectable = Boolean(plan.valid) && plan.status === "valid";
                const isSelected = selectedId === plan.plan_id;
                return (
                  <button
                    key={plan.plan_id}
                    type="button"
                    aria-pressed={isSelected}
                    disabled={!selectable}
                    data-testid={`local-plan-option-${plan.plan_id}`}
                    onClick={() => {
                      if (!selectable) return;
                      setSelectedPlan(plan);
                      setResult(null);
                      setError(null);
                    }}
                    className={`flex w-full items-start justify-between gap-2 rounded-lg border p-2.5 text-left transition-colors ${
                      isSelected
                        ? "border-primary/60 bg-primary/10"
                        : "border-border bg-background hover:bg-muted/50"
                    } disabled:cursor-not-allowed disabled:opacity-60`}
                  >
                    <span className="min-w-0 space-y-0.5">
                      <span className="block truncate text-xs font-medium text-foreground">
                        {plan.title || plan.plan_id}
                      </span>
                      <span className="block font-mono text-[10.5px] text-muted-foreground">
                        {plan.plan_id}
                      </span>
                      {!selectable ? (
                        <span className="block text-[10.5px] text-destructive">
                          {planErrorText(plan)}
                        </span>
                      ) : null}
                    </span>
                    {isSelected ? (
                      <CheckCircle2 className="mt-0.5 h-3.5 w-3.5 shrink-0 text-primary" />
                    ) : null}
                  </button>
                );
              })}
            </div>
          )}
        </div>
      ) : null}

      <div
        className="space-y-1.5 rounded-lg border border-border bg-muted/20 p-3"
        data-testid="local-plan-consent"
      >
        <div className="flex items-center gap-1.5 text-xs font-medium text-foreground">
          <Shield className="h-3.5 w-3.5 text-primary" />
          {t("localPlanConsentTitle", "上传前确认")}
        </div>
        <ul className="list-disc space-y-1 pl-4 text-[11px] text-muted-foreground">
          <li>
            {t(
              "localPlanConsentTool",
              "工具调用按单次审批处理，不会获得长期授权。"
            )}
          </li>
          <li>
            {t(
              "localPlanConsentRecommendation",
              "仅对带有唯一明确推荐答案的问题代为选择该答案，其余问题等待人工接管。"
            )}
          </li>
          <li>
            {t(
              "localPlanConsentPr",
              "完成后会创建 PR 供人工审查，但不会自动合并。"
            )}
          </li>
        </ul>
      </div>

      {result?.reused ? (
        <div
          className="rounded-lg border border-sky-500/30 bg-sky-500/10 p-3 text-xs text-sky-700 dark:text-sky-300"
          data-testid="local-plan-reused"
          role="status"
        >
          {t(
            "localPlanReused",
            "该计划已存在云端任务，已返回原有任务/PR，未启动新的运行。"
          )}
        </div>
      ) : null}

      {error ? (
        <div
          className="flex items-start gap-2 rounded-lg border border-destructive/20 bg-destructive/10 p-3 text-xs text-destructive"
          data-testid="local-plan-error"
          role="alert"
        >
          <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <span className="min-w-0 break-words">{error}</span>
        </div>
      ) : null}

      <div className="flex items-center justify-end gap-2 border-t border-border/60 pt-2">
        <button
          type="button"
          data-testid="local-plan-submit"
          disabled={submitDisabled}
          onClick={() => void handleSubmit()}
          className="inline-flex items-center gap-1.5 rounded-lg bg-primary px-3.5 py-1.5 text-xs font-medium text-primary-foreground shadow-2xs transition-all hover:bg-primary/90 active:scale-98 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {submitting ? (
            <>
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t("localPlanSubmitting", "正在上传并启动...")}
            </>
          ) : (
            <>
              <Upload className="h-3.5 w-3.5" />
              {t("localPlanSubmit", "上传并执行")}
            </>
          )}
        </button>
      </div>
    </div>
  );
}
