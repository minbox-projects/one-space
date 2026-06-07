import { describe, expect, it } from "vitest";
import {
  buildSshAutoConnectFailedEvent,
  buildSshUnexpectedDisconnectEvent,
  buildSshWindowReconnectDoneEvent,
  buildUpdaterSystemEvent,
} from "@/lib/actionDescriptors/appSystemEvents";
import {
  buildCleanupBackupsActionDescriptor,
  buildCreateBackupActionDescriptor,
  buildDeleteBackupActionDescriptor,
} from "@/lib/actionDescriptors/backup";
import { buildDeleteMcpServerActionDescriptor } from "@/lib/actionDescriptors/mcpServers";
import { buildUninstallSkillActionDescriptor } from "@/lib/actionDescriptors/skills";
import { buildConnectTunnelActionDescriptor } from "@/lib/actionDescriptors/sshTunnels";
import {
  buildCopyWorkspaceActionDescriptor,
  buildDeleteWorkspaceActionDescriptor,
} from "@/lib/actionDescriptors/workspaces";
import { createMockActionContext } from "@/test/mocks/actionContext";

const { t } = createMockActionContext();

describe("actionDescriptors", () => {
  it("builds updater available event without toast", () => {
    const event = buildUpdaterSystemEvent(t, {
      version: "1.2.3",
      currentVersion: "1.2.2",
      body: "notes",
      status: "available",
      showUpdateIndicator: true,
      source: "manual",
    });

    expect(event).toMatchObject({
      key: "updater:available:1.2.3",
      descriptor: {
        severity: "info",
        toast: false,
        metadata: {
          version: "1.2.3",
          currentVersion: "1.2.2",
          source: "manual",
        },
      },
    });
  });

  it("builds updater error event with stable dedupe key", () => {
    const event = buildUpdaterSystemEvent(t, {
      status: "error",
      error: "network failed",
      showUpdateIndicator: false,
    });

    expect(event).toMatchObject({
      key: "updater:error:network failed",
      descriptor: {
        severity: "error",
        dedupeKey: "updater:error",
        toast: true,
      },
    });
  });

  it("builds ssh reconnect events for success, failure, and partial failure", () => {
    expect(
      buildSshWindowReconnectDoneEvent(t, {
        total: 2,
        succeeded: 2,
        failed: 0,
      }),
    ).toMatchObject({
      severity: "success",
      action: "window-reconnect-done",
    });

    expect(
      buildSshWindowReconnectDoneEvent(t, {
        total: 1,
        succeeded: 0,
        failed: 1,
      }),
    ).toMatchObject({
      severity: "error",
      action: "window-reconnect-failed",
    });

    expect(
      buildSshWindowReconnectDoneEvent(t, {
        total: 3,
        succeeded: 2,
        failed: 1,
      }),
    ).toMatchObject({
      severity: "error",
      action: "window-reconnect-partial",
      dedupeKey: "ssh-tunnels:window-reconnect:partial:3:1",
    });
  });

  it("builds ssh background failure events with target-specific dedupe keys", () => {
    const autoConnect = buildSshAutoConnectFailedEvent(t, {
      name: "prod",
      error: "Permission denied (publickey).",
    });
    const disconnect = buildSshUnexpectedDisconnectEvent(t, "prod");

    expect(autoConnect.dedupeKey).toBe("ssh-tunnels:auto-connect-failed:prod");
    expect(disconnect.dedupeKey).toBe(
      "ssh-tunnels:unexpected-disconnect:prod",
    );
  });

  it("builds backup action descriptors with confirmation and targets", () => {
    expect(buildCreateBackupActionDescriptor(t, "skills")).toMatchObject({
      dedupeKey: "backup:create:skills",
      confirm: { kind: "warning" },
    });
    expect(buildDeleteBackupActionDescriptor(t, "entry-1")).toMatchObject({
      dedupeKey: "backup:delete:entry-1",
      confirm: { kind: "error" },
    });
    expect(buildCleanupBackupsActionDescriptor(t, 30)).toMatchObject({
      dedupeKey: "backup:cleanup:30",
      metadata: { retention_days: 30 },
    });
  });

  it("builds workspace/module delete and connect descriptors", () => {
    expect(
      buildDeleteWorkspaceActionDescriptor(t, { id: "ws-1", name: "Main" }),
    ).toMatchObject({
      dedupeKey: "workspaces:delete:ws-1",
      confirm: { kind: "error" },
    });
    expect(
      buildCopyWorkspaceActionDescriptor(t, {
        workspaceId: "ws-1",
        targetRootPath: "/tmp/copy",
      }),
    ).toMatchObject({
      dedupeKey: "workspaces:copy:ws-1:/tmp/copy",
    });
    expect(buildDeleteMcpServerActionDescriptor(t, "mcp-1")).toMatchObject({
      dedupeKey: "mcp:delete:mcp-1",
    });
    expect(
      buildUninstallSkillActionDescriptor(t, {
        model: "claude",
        id: "skill-1",
        name: "Skill",
      }),
    ).toMatchObject({
      dedupeKey: "skills:uninstall:claude:skill-1",
    });
    expect(buildConnectTunnelActionDescriptor(t, "tun-1")).toMatchObject({
      dedupeKey: "ssh-tunnels:connect:tun-1",
    });
  });
});
