import { useCallback, useEffect, useRef, useState } from "react";
import {
  Ban,
  CheckCircle2,
  CircleAlert,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  Save,
  Trash2,
  Workflow,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useConfirmDialog } from "./ConfirmDialogProvider";
import {
  AiWorkFlowError,
  aiWorkFlowEnvironmentCreate,
  aiWorkFlowEnvironmentDelete,
  aiWorkFlowEnvironmentList,
  aiWorkFlowEnvironmentRead,
  aiWorkFlowEnvironmentStatus,
  aiWorkFlowEnvironmentUpdate,
  aiWorkFlowEnvironmentUse,
  aiWorkFlowInstallCancel,
  aiWorkFlowInstallLogs,
  aiWorkFlowInstallOrUpdate,
  aiWorkFlowInstallStatus,
  aiWorkFlowInstallVersion,
  type AiWorkFlowEnvironmentStatus,
  type AiWorkFlowEnvironmentSummary,
  type AiWorkFlowInstallLog,
  type AiWorkFlowInstallStage,
  type AiWorkFlowInstallStatus,
  type AiWorkFlowInstallVersion,
} from "@/lib/aiWorkFlow";

const IDLE_STATUS: AiWorkFlowInstallStatus = {
  state: "idle",
  operation: null,
  stage: null,
  started_at: null,
  finished_at: null,
  version: null,
  error: null,
};

const STAGE_LABELS: Record<AiWorkFlowInstallStage, [string, string]> = {
  preparing: ["准备中", "Preparing"],
  clone: ["克隆仓库", "Cloning repository"],
  verify_repository: ["验证仓库", "Verifying repository"],
  pull: ["更新仓库", "Updating repository"],
  npm_ci: ["安装依赖", "Installing dependencies"],
  install: ["安装 AI Work Flow", "Installing AI Work Flow"],
  validate: ["验证安装", "Validating installation"],
  complete: ["完成", "Complete"],
};

function errorDetails(error: unknown) {
  if (error instanceof AiWorkFlowError) {
    return { code: error.code, message: error.message };
  }
  if (error && typeof error === "object") {
    const value = error as { code?: unknown; message?: unknown };
    if (typeof value.code === "string" && typeof value.message === "string") {
      return { code: value.code, message: value.message };
    }
  }
  return { code: "unknown", message: "AI Work Flow operation failed" };
}

export function AiWorkFlowTool() {
  const { i18n } = useTranslation();
  const confirm = useConfirmDialog();
  const zh = i18n.language === "zh";
  const [status, setStatus] = useState(IDLE_STATUS);
  const [version, setVersion] = useState<AiWorkFlowInstallVersion | null>(null);
  const [logs, setLogs] = useState<AiWorkFlowInstallLog[]>([]);
  const [installLoading, setInstallLoading] = useState(true);
  const [installError, setInstallError] = useState<ReturnType<typeof errorDetails> | null>(null);
  const installActionRef = useRef(false);

  const [environments, setEnvironments] = useState<AiWorkFlowEnvironmentSummary[]>([]);
  const [environmentStatus, setEnvironmentStatus] =
    useState<AiWorkFlowEnvironmentStatus | null>(null);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [editorContent, setEditorContent] = useState("");
  const [newName, setNewName] = useState("");
  const [environmentLoading, setEnvironmentLoading] = useState(true);
  const [environmentBusy, setEnvironmentBusy] = useState(false);
  const [environmentError, setEnvironmentError] = useState<ReturnType<typeof errorDetails> | null>(null);
  const environmentActionRef = useRef(false);
  const documentRequestRef = useRef(0);

  const refreshInstall = useCallback(async () => {
    const [nextStatus, nextVersion, nextLogs] = await Promise.all([
      aiWorkFlowInstallStatus(),
      aiWorkFlowInstallVersion(),
      aiWorkFlowInstallLogs(),
    ]);
    setStatus(nextStatus);
    setVersion(nextVersion);
    setLogs([...nextLogs].sort((left, right) => left.sequence - right.sequence));
    setInstallError(nextStatus.error ?? nextVersion.error ?? null);
  }, []);

  const readEnvironment = useCallback(async (name: string) => {
    const request = ++documentRequestRef.current;
    setEnvironmentBusy(true);
    setEnvironmentError(null);
    try {
      const document = await aiWorkFlowEnvironmentRead(name);
      if (request !== documentRequestRef.current) return;
      setSelectedName(document.name);
      setEditorContent(document.content);
      setEnvironmentError(document.validation_error);
    } catch (error) {
      if (request === documentRequestRef.current) setEnvironmentError(errorDetails(error));
    } finally {
      if (request === documentRequestRef.current) setEnvironmentBusy(false);
    }
  }, []);

  const refreshEnvironments = useCallback(async (preferredName?: string | null) => {
    const [list, current] = await Promise.all([
      aiWorkFlowEnvironmentList(),
      aiWorkFlowEnvironmentStatus(),
    ]);
    setEnvironments(list);
    setEnvironmentStatus(current);
    const preferred = preferredName ?? (current.current === "default" ? null : current.current);
    const nextName =
      (preferred && list.some((item) => item.name === preferred) ? preferred : null) ??
      list[0]?.name ??
      null;
    if (nextName) await readEnvironment(nextName);
    else {
      documentRequestRef.current += 1;
      setSelectedName(null);
      setEditorContent("");
    }
  }, [readEnvironment]);

  useEffect(() => {
    let active = true;
    void refreshInstall()
      .catch((error) => active && setInstallError(errorDetails(error)))
      .finally(() => active && setInstallLoading(false));
    void refreshEnvironments()
      .catch((error) => active && setEnvironmentError(errorDetails(error)))
      .finally(() => active && setEnvironmentLoading(false));
    return () => {
      active = false;
      documentRequestRef.current += 1;
    };
  }, [refreshEnvironments, refreshInstall]);

  useEffect(() => {
    if (status.state !== "running") return;
    let active = true;
    const poll = () => {
      void refreshInstall().catch((error) => {
        if (active) setInstallError(errorDetails(error));
      });
    };
    const timer = window.setInterval(poll, 500);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [refreshInstall, status.state]);

  const runInstall = async () => {
    if (installActionRef.current || status.state === "running") return;
    installActionRef.current = true;
    setInstallError(null);
    setStatus((current) => ({
      ...current,
      state: "running",
      operation: version?.installed ? "update" : "install",
      stage: "preparing",
      error: null,
    }));
    try {
      setStatus(await aiWorkFlowInstallOrUpdate());
      await refreshInstall();
    } catch (error) {
      setInstallError(errorDetails(error));
    } finally {
      installActionRef.current = false;
    }
  };

  const cancelInstall = async () => {
    if (status.state !== "running") return;
    try {
      const result = await aiWorkFlowInstallCancel();
      setStatus(result.status);
    } catch (error) {
      setInstallError(errorDetails(error));
    }
  };

  const withEnvironmentAction = async (action: () => Promise<void>) => {
    if (environmentActionRef.current) return;
    environmentActionRef.current = true;
    setEnvironmentBusy(true);
    setEnvironmentError(null);
    try {
      await action();
    } catch (error) {
      setEnvironmentError(errorDetails(error));
    } finally {
      environmentActionRef.current = false;
      setEnvironmentBusy(false);
    }
  };

  const createEnvironment = () =>
    withEnvironmentAction(async () => {
      const name = newName.trim();
      if (!name) {
        setEnvironmentError({
          code: "invalid_environment_name",
          message: zh ? "请输入环境名称" : "Enter an environment name",
        });
        return;
      }
      const document = await aiWorkFlowEnvironmentCreate(name, "{\n}\n");
      setNewName("");
      await refreshEnvironments(document.name);
    });

  const saveEnvironment = () => {
    if (!selectedName) return;
    void withEnvironmentAction(async () => {
      const document = await aiWorkFlowEnvironmentUpdate(selectedName, editorContent);
      setEditorContent(document.content);
      await refreshEnvironments(document.name);
    });
  };

  const useEnvironment = () => {
    if (!selectedName) return;
    void withEnvironmentAction(async () => {
      setEnvironmentStatus(await aiWorkFlowEnvironmentUse(selectedName));
      await refreshEnvironments(selectedName);
    });
  };

  const deleteEnvironment = async () => {
    if (!selectedName) return;
    const name = selectedName;
    const accepted = await confirm(
      zh ? `删除环境“${name}”？` : `Delete environment “${name}”?`,
      {
        title: zh ? "删除环境" : "Delete environment",
        okLabel: zh ? "删除" : "Delete",
        kind: "error",
      },
    );
    if (!accepted) return;
    void withEnvironmentAction(async () => {
      setEnvironmentStatus(await aiWorkFlowEnvironmentDelete(name));
      setSelectedName(null);
      setEditorContent("");
      await refreshEnvironments(null);
    });
  };

  const running = status.state === "running";
  const stateLabel = {
    idle: zh ? "未运行" : "Idle",
    running: zh ? "运行中" : "Running",
    succeeded: zh ? "成功" : "Succeeded",
    failed: zh ? "失败" : "Failed",
    cancelled: zh ? "已取消" : "Cancelled",
  }[status.state];
  const versionLabel = installLoading
    ? zh ? "加载中..." : "Loading..."
    : version?.installed
      ? version.version ?? (zh ? "已安装（版本未知）" : "Installed (version unknown)")
      : zh ? "未安装" : "Not installed";

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-8 pb-8">
      <section aria-labelledby="ai-work-flow-title" className="space-y-5">
        <div className="flex flex-wrap items-start justify-between gap-4 border-b pb-5">
          <div className="min-w-0 max-w-2xl">
            <div className="flex items-center gap-3">
              <Workflow className="h-6 w-6 text-violet-600" aria-hidden="true" />
              <h2 id="ai-work-flow-title" className="text-xl font-semibold">AI Work Flow</h2>
            </div>
            <p className="mt-2 text-sm leading-6 text-muted-foreground">
              {zh
                ? "管理 AI Work Flow 的受管安装、更新与独立环境配置。"
                : "Manage the controlled AI Work Flow installation, updates, and isolated environment configurations."}
            </p>
          </div>
          <button type="button" onClick={() => void refreshInstall().catch((error) => setInstallError(errorDetails(error)))} disabled={installLoading || running} className="inline-flex h-9 w-9 items-center justify-center rounded-md border hover:bg-muted disabled:opacity-50" aria-label={zh ? "刷新安装状态" : "Refresh installation status"} title={zh ? "刷新安装状态" : "Refresh installation status"}>
            <RefreshCw className="h-4 w-4" />
          </button>
        </div>

        <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_auto] md:items-end">
          <div className="grid gap-3 sm:grid-cols-3">
            <div><div className="text-xs font-medium text-muted-foreground">{zh ? "版本" : "Version"}</div><div className="mt-1 break-words text-sm font-semibold">{versionLabel}</div></div>
            <div>
              <div className="text-xs font-medium text-muted-foreground">{zh ? "结果" : "Result"}</div>
              <div className="mt-1 flex items-center gap-2 text-sm font-semibold">
                {running ? <Loader2 className="h-4 w-4 animate-spin" /> : status.state === "succeeded" ? <CheckCircle2 className="h-4 w-4 text-emerald-600" /> : status.state === "failed" || status.state === "cancelled" ? <CircleAlert className="h-4 w-4 text-destructive" /> : null}
                {stateLabel}
              </div>
            </div>
            <div><div className="text-xs font-medium text-muted-foreground">{zh ? "阶段" : "Stage"}</div><div className="mt-1 break-words text-sm font-semibold">{status.stage ? STAGE_LABELS[status.stage][zh ? 0 : 1] : zh ? "等待操作" : "Waiting"}</div></div>
          </div>
          <div className="flex flex-wrap gap-2">
            <button type="button" onClick={() => void runInstall()} disabled={installLoading || running} className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground disabled:opacity-50">
              {running ? <Loader2 className="h-4 w-4 animate-spin" /> : <Play className="h-4 w-4" />}
              {version?.installed ? (zh ? "更新" : "Update") : zh ? "安装" : "Install"}
            </button>
            <button type="button" onClick={() => void cancelInstall()} disabled={!running} className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm font-medium hover:bg-muted disabled:opacity-50"><Ban className="h-4 w-4" />{zh ? "取消" : "Cancel"}</button>
          </div>
        </div>

        {installError ? <div role="alert" className="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm"><span className="font-mono text-xs font-semibold">{installError.code}</span><p className="mt-1 break-words text-destructive">{installError.message}</p></div> : null}

        <div className="overflow-hidden rounded-md border">
          <div className="border-b bg-muted/30 px-3 py-2 text-xs font-semibold uppercase text-muted-foreground">{zh ? "结构化日志" : "Structured logs"}</div>
          <div className="max-h-56 min-h-28 overflow-auto bg-background p-3 font-mono text-xs" aria-label={zh ? "安装日志" : "Installation logs"}>
            {logs.length === 0 ? <div className="text-muted-foreground">{zh ? "暂无日志" : "No logs yet"}</div> : logs.map((entry) => (
              <div key={entry.sequence} className="grid grid-cols-[3rem_5rem_minmax(0,1fr)] gap-2 py-1">
                <span className="text-muted-foreground">#{entry.sequence}</span>
                <span className={entry.source === "stderr" ? "text-destructive" : "text-muted-foreground"}>{entry.source}</span>
                <span className="whitespace-pre-wrap break-words">{entry.message}</span>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section aria-labelledby="ai-work-flow-environments" className="space-y-5 border-t pt-7">
        <div className="flex flex-wrap items-end justify-between gap-4">
          <div><h2 id="ai-work-flow-environments" className="text-lg font-semibold">{zh ? "环境" : "Environments"}</h2><p className="mt-1 text-sm text-muted-foreground">{zh ? "当前环境：" : "Current environment: "}<span className="font-mono font-medium text-foreground">{environmentStatus?.current ?? (zh ? "加载中..." : "Loading...")}</span></p></div>
          <div className="flex min-w-0 flex-1 gap-2 sm:max-w-md">
            <input value={newName} onChange={(event) => setNewName(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void createEnvironment(); }} disabled={environmentBusy} placeholder={zh ? "新环境名称" : "New environment name"} aria-label={zh ? "新环境名称" : "New environment name"} className="h-9 min-w-0 flex-1 rounded-md border bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring" />
            <button type="button" onClick={() => void createEnvironment()} disabled={environmentBusy} className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm font-medium hover:bg-muted disabled:opacity-50"><Plus className="h-4 w-4" />{zh ? "创建" : "Create"}</button>
          </div>
        </div>

        {environmentError ? <div role="alert" className="rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm"><span className="font-mono text-xs font-semibold">{environmentError.code}</span><p className="mt-1 break-words text-destructive">{environmentError.message}</p></div> : null}

        <div className="grid min-h-[24rem] gap-5 lg:grid-cols-[15rem_minmax(0,1fr)]">
          <div className="min-w-0 border-r-0 lg:border-r lg:pr-5">
            <div className="mb-2 text-xs font-semibold uppercase text-muted-foreground">{zh ? "环境列表" : "Environment list"}</div>
            {environmentLoading ? <div className="flex items-center gap-2 py-3 text-sm text-muted-foreground"><Loader2 className="h-4 w-4 animate-spin" />{zh ? "加载中..." : "Loading..."}</div> : environments.length === 0 ? <p className="py-3 text-sm text-muted-foreground">{zh ? "还没有已保存环境" : "No saved environments"}</p> : (
              <div className="space-y-1">{environments.map((item) => (
                <button key={item.name} type="button" onClick={() => void readEnvironment(item.name)} disabled={environmentBusy} className={`flex w-full min-w-0 items-center justify-between gap-2 rounded-md px-3 py-2 text-left text-sm ${selectedName === item.name ? "bg-primary text-primary-foreground" : "hover:bg-muted"}`}>
                  <span className="truncate font-mono">{item.name}</span><span className="shrink-0 text-xs">{item.current ? (zh ? "当前" : "Current") : item.valid ? "" : zh ? "无效" : "Invalid"}</span>
                </button>
              ))}</div>
            )}
          </div>

          <div className="flex min-w-0 flex-col gap-3">
            <div className="flex min-h-9 flex-wrap items-center justify-between gap-2">
              <div className="min-w-0 truncate font-mono text-sm font-semibold">{selectedName ?? (zh ? "选择一个环境" : "Select an environment")}</div>
              <div className="flex flex-wrap gap-2">
                <button type="button" onClick={useEnvironment} disabled={!selectedName || environmentBusy || environmentStatus?.current === selectedName} className="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm font-medium hover:bg-muted disabled:opacity-50"><Play className="h-4 w-4" />{zh ? "使用" : "Use"}</button>
                <button type="button" onClick={saveEnvironment} disabled={!selectedName || environmentBusy} className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground disabled:opacity-50">{environmentBusy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}{zh ? "保存" : "Save"}</button>
                <button type="button" onClick={() => void deleteEnvironment()} disabled={!selectedName || environmentBusy} className="inline-flex h-9 w-9 items-center justify-center rounded-md border text-destructive hover:bg-destructive/10 disabled:opacity-50" aria-label={zh ? "删除环境" : "Delete environment"} title={zh ? "删除环境" : "Delete environment"}><Trash2 className="h-4 w-4" /></button>
              </div>
            </div>
            <textarea value={editorContent} onChange={(event) => setEditorContent(event.target.value)} disabled={!selectedName || environmentBusy} spellCheck={false} aria-label={zh ? "完整环境 JSON" : "Complete environment JSON"} className="min-h-80 w-full flex-1 resize-y rounded-md border bg-background p-3 font-mono text-sm leading-6 outline-none focus:ring-2 focus:ring-ring disabled:opacity-60" placeholder={selectedName ? "{}" : zh ? "创建或选择环境后编辑" : "Create or select an environment to edit"} />
          </div>
        </div>
      </section>
    </div>
  );
}
