import { invoke } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";
import type { MessageCreateInput, MessageSeverity, MessageTarget } from "@/lib/messages";

export type ToastKind = "info" | "success" | "warning" | "error" | "loading";
export type SensitiveActionKind =
  | "delete"
  | "restore"
  | "import"
  | "export"
  | "activate"
  | "apply"
  | "sync"
  | "cli_update"
  | "backup"
  | "security_change"
  | "rotate_token"
  | "external_launch"
  | "open";

export interface ActionContext {
  t: TFunction;
  confirm: (
    message: string,
    options?: {
      title?: string;
      okLabel?: string;
      cancelLabel?: string;
      kind?: "info" | "warning" | "error";
    },
  ) => Promise<boolean>;
  pushToast: (toast: {
    title: string;
    description?: string;
    kind?: ToastKind;
    durationMs?: number;
  }) => string;
  recordMessage: (input: MessageCreateInput) => Promise<void>;
}

export interface ActionMessageDescriptor {
  title: string;
  summary?: string | null;
  detail?: string | null;
  toastTitle?: string;
  toastDescription?: string;
  messageTitle?: string;
  metadata?: unknown;
}

export interface ActionDescriptor {
  source: string;
  category: string;
  action: string;
  target?: MessageTarget | null;
  dedupeKey?: string;
  metadata?: Record<string, unknown>;
  confirm?: {
    message: string;
    title?: string;
    okLabel?: string;
    cancelLabel?: string;
    kind?: "info" | "warning" | "error";
  };
  success?: ActionMessageDescriptor | false;
  warning?: ActionMessageDescriptor | false;
  error?: ActionMessageDescriptor | false;
}

export interface ActionResultNotifyOptions {
  toast?: boolean;
  toastKind?: ToastKind;
  durationMs?: number;
  closeToastId?: string | null;
}

export interface SystemEventDescriptor {
  source: string;
  category: string;
  action?: string;
  severity: MessageSeverity;
  target?: MessageTarget | null;
  dedupeKey?: string;
  metadata?: Record<string, unknown>;
  message: ActionMessageDescriptor;
  toast?: boolean;
  toastKind?: ToastKind;
  durationMs?: number;
}

const SENSITIVE_ACTION_PRESETS: Record<
  SensitiveActionKind,
  Required<NonNullable<ActionDescriptor["confirm"]>>
> = {
  delete: {
    message: "Delete this item?",
    title: "Delete",
    okLabel: "Delete",
    cancelLabel: "Cancel",
    kind: "error",
  },
  restore: {
    message: "Restore this item?",
    title: "Restore",
    okLabel: "Restore",
    cancelLabel: "Cancel",
    kind: "warning",
  },
  import: {
    message: "Import and overwrite current data if needed?",
    title: "Import",
    okLabel: "Import",
    cancelLabel: "Cancel",
    kind: "warning",
  },
  export: {
    message: "Export current data?",
    title: "Export",
    okLabel: "Export",
    cancelLabel: "Cancel",
    kind: "warning",
  },
  activate: {
    message: "Activate this item now?",
    title: "Activate",
    okLabel: "Activate",
    cancelLabel: "Cancel",
    kind: "warning",
  },
  apply: {
    message: "Apply this change now?",
    title: "Apply",
    okLabel: "Apply",
    cancelLabel: "Cancel",
    kind: "warning",
  },
  sync: {
    message: "Run sync now?",
    title: "Sync",
    okLabel: "Sync",
    cancelLabel: "Cancel",
    kind: "warning",
  },
  cli_update: {
    message: "Apply this CLI update now?",
    title: "CLI Update",
    okLabel: "Update",
    cancelLabel: "Cancel",
    kind: "warning",
  },
  backup: {
    message: "Create or modify backup data now?",
    title: "Backup",
    okLabel: "Continue",
    cancelLabel: "Cancel",
    kind: "warning",
  },
  security_change: {
    message: "Apply this security-related change?",
    title: "Security Change",
    okLabel: "Apply",
    cancelLabel: "Cancel",
    kind: "warning",
  },
  rotate_token: {
    message: "Rotate this token now?",
    title: "Rotate Token",
    okLabel: "Rotate",
    cancelLabel: "Cancel",
    kind: "warning",
  },
  external_launch: {
    message: "Open this external target?",
    title: "Open External Target",
    okLabel: "Open",
    cancelLabel: "Cancel",
    kind: "warning",
  },
  open: {
    message: "Open this target?",
    title: "Open",
    okLabel: "Open",
    cancelLabel: "Cancel",
    kind: "warning",
  },
};

function defaultSuccessTitle(t: TFunction) {
  return t("actionSucceeded", "Action completed");
}

function defaultErrorTitle(t: TFunction) {
  return t("actionFailed", "Action failed");
}

export function messageSummary(detail?: string | null) {
  return detail?.split("\n").find(Boolean)?.trim() || undefined;
}

export function buildMessageInput(
  descriptor: ActionDescriptor,
  severity: MessageSeverity,
  message: ActionMessageDescriptor,
): MessageCreateInput {
  return {
    source: descriptor.source,
    category: descriptor.category,
    severity,
    title: message.messageTitle || message.title,
    summary: message.summary ?? null,
    detail: message.detail ?? null,
    dedupe_key:
      descriptor.dedupeKey || `${descriptor.source}:${descriptor.category}:${descriptor.action}:${severity}`,
    target: descriptor.target,
    metadata: {
      action: descriptor.action,
      ...(descriptor.metadata || {}),
      ...(message.metadata && typeof message.metadata === "object"
        ? (message.metadata as Record<string, unknown>)
        : message.metadata
          ? { value: message.metadata }
          : {}),
    },
  };
}

export async function confirmSensitiveAction(
  context: Pick<ActionContext, "confirm">,
  kind: SensitiveActionKind,
  overrides?: Partial<NonNullable<ActionDescriptor["confirm"]>>,
) {
  const preset = SENSITIVE_ACTION_PRESETS[kind];
  return context.confirm(overrides?.message || preset.message, {
    title: overrides?.title || preset.title,
    okLabel: overrides?.okLabel || preset.okLabel,
    cancelLabel: overrides?.cancelLabel || preset.cancelLabel,
    kind: overrides?.kind || preset.kind,
  });
}

export async function notifyActionResult(
  context: Pick<ActionContext, "pushToast" | "recordMessage">,
  descriptor: ActionDescriptor,
  severity: MessageSeverity,
  message: ActionMessageDescriptor,
  options?: ActionResultNotifyOptions,
) {
  await context.recordMessage(buildMessageInput(descriptor, severity, message));
  if (options?.toast === false) {
    return;
  }
  if (options?.closeToastId) {
    const maybeDismiss = context as Pick<ActionContext, "pushToast"> & {
      dismissToast?: (id: string) => void;
    };
    maybeDismiss.dismissToast?.(options.closeToastId);
  }
  context.pushToast({
    title: message.toastTitle || message.title,
    description:
      message.toastDescription ||
      message.summary ||
      messageSummary(message.detail || undefined),
    kind:
      options?.toastKind ||
      (severity === "warning"
        ? "warning"
        : severity === "error"
          ? "error"
          : severity === "success"
            ? "success"
            : "info"),
    durationMs: options?.durationMs,
  });
}

export async function notifySystemEvent(
  context: Pick<ActionContext, "pushToast" | "recordMessage"> & {
    dismissToast?: (id: string) => void;
  },
  descriptor: SystemEventDescriptor,
) {
  await notifyActionResult(
    context,
    {
      source: descriptor.source,
      category: descriptor.category,
      action: descriptor.action || descriptor.category,
      target: descriptor.target,
      dedupeKey: descriptor.dedupeKey,
      metadata: descriptor.metadata,
    },
    descriptor.severity,
    descriptor.message,
    {
      toast: descriptor.toast,
      toastKind: descriptor.toastKind,
      durationMs: descriptor.durationMs,
    },
  );
}

export async function invokeTyped<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(command, args);
}

export async function runUserAction<T>(
  context: ActionContext & { dismissToast?: (id: string) => void },
  descriptor: ActionDescriptor,
  execute: () => Promise<T>,
): Promise<T | null> {
  const { t, confirm, pushToast, recordMessage } = context;

  if (descriptor.confirm) {
    const confirmed = await confirm(descriptor.confirm.message, {
      title: descriptor.confirm.title,
      okLabel: descriptor.confirm.okLabel,
      cancelLabel: descriptor.confirm.cancelLabel,
      kind: descriptor.confirm.kind,
    });
    if (!confirmed) {
      return null;
    }
  }

  try {
      const result = await execute();
    if (descriptor.success !== false) {
      const success: ActionMessageDescriptor = descriptor.success || {
        title: defaultSuccessTitle(t),
      };
      await notifyActionResult(
        { pushToast, recordMessage },
        descriptor,
        "success",
        success,
        { toast: true, toastKind: "success" },
      );
    }
    return result;
  } catch (error) {
    const detail =
      error instanceof Error
        ? error.stack || error.message
        : typeof error === "string"
          ? error
          : JSON.stringify(error, null, 2);
    const failure: ActionMessageDescriptor =
      descriptor.error || {
        title: defaultErrorTitle(t),
        summary: messageSummary(detail),
        detail,
      };
    await notifyActionResult(
      { pushToast, recordMessage },
      descriptor,
      "error",
      {
        ...failure,
        summary: failure.summary ?? messageSummary(detail) ?? null,
        detail: failure.detail ?? detail,
      },
      { toast: true, toastKind: "error" },
    );
    throw error;
  }
}
