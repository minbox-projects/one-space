import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en/cloudAgent";
import zh from "@/i18n/locales/zh/cloudAgent";
import {
  CLOUD_AGENT_DEPLOY_PROGRESS_EVENT,
  cloudAgentDeploy,
  cloudAgentGetInstallationInfo,
  compareSemanticVersions,
  formatInstalledAt,
} from "@/lib/cloudAgent";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

describe("cloudAgent lib", () => {
  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "cloud_agent_get_installation_info") {
        return {
          host: args?.host ?? "b",
          state: "installed",
          local_latest_version: "0.2.0",
          remote_version: "0.1.9",
          installed_at: "2026-01-01T00:00:00Z",
        };
      }
      if (command === "cloud_agent_deploy") {
        return {
          host: args?.host ?? "b",
          operation_id: args?.operationId ?? "op",
          success: true,
          phase: "metadata",
          remote_version: "0.2.0",
          installed_at: "2026-01-02T00:00:00Z",
        };
      }
      return null;
    });
  });

  it("maps snake_case installation info and deploy results to camelCase", async () => {
    const info = await cloudAgentGetInstallationInfo("b");
    expect(invokeMock).toHaveBeenCalledWith("cloud_agent_get_installation_info", { host: "b" });
    expect(info.host).toBe("b");
    expect(info.state).toBe("installed");
    expect(info.localLatestVersion).toBe("0.2.0");
    expect(info.remoteVersion).toBe("0.1.9");
    expect(info.installedAt).toBe("2026-01-01T00:00:00Z");

    const deploy = await cloudAgentDeploy("b", "op");
    expect(invokeMock).toHaveBeenCalledWith("cloud_agent_deploy", { host: "b", operationId: "op" });
    expect(deploy.host).toBe("b");
    expect(deploy.operationId).toBe("op");
    expect(deploy.success).toBe(true);
    expect(deploy.remoteVersion).toBe("0.2.0");
    expect(deploy.installedAt).toBe("2026-01-02T00:00:00Z");
  });

  it("exports event constant", () => {
    expect(CLOUD_AGENT_DEPLOY_PROGRESS_EVENT).toBe("cloud_agent_deploy_progress");
  });

  describe("semver", () => {
    it("0.1.9 < 0.1.10", () => expect(compareSemanticVersions("0.1.9", "0.1.10")).toBe("older"));
    it("newer/equal", () => {
      expect(compareSemanticVersions("1.0.0", "0.9.0")).toBe("newer");
      expect(compareSemanticVersions("1.0.0", "1.0.0")).toBe("equal");
    });
    it("v prefix", () => expect(compareSemanticVersions("v1.0.0", "1.0.0")).toBe("equal"));
    it("build metadata equality", () => expect(compareSemanticVersions("1.0.0+build", "1.0.0+other")).toBe("equal"));
    it("prerelease before release", () => expect(compareSemanticVersions("1.0.0-rc.1", "1.0.0")).toBe("older"));
    it("1.2.3-beta.2 < 1.2.3-beta.10", () => expect(compareSemanticVersions("1.2.3-beta.2", "1.2.3-beta.10")).toBe("older"));
    it("null/undefined/empty/malformed -> unknown", () => {
      for (const v of [null, undefined, "", "not-a-version"]) {
        expect(compareSemanticVersions(v, "1.0.0")).toBe("unknown");
      }
    });
  });

  describe("formatInstalledAt", () => {
    it("en valid contains 2026 and not unknown", () => {
      const out = formatInstalledAt("2026-01-01T00:00:00Z", "en");
      expect(out).toContain("2026");
      expect(out).not.toBe("Not recorded");
    });
    it("zh valid contains 2026", () => {
      const out = formatInstalledAt("2026-01-01T00:00:00Z", "zh");
      expect(out).toContain("2026");
    });
    it("invalid/empty/null/0 -> Not recorded / 未记录", () => {
      for (const v of ["", null, "0", "bad"]) {
        expect(formatInstalledAt(v, "en")).toBe("Not recorded");
        expect(formatInstalledAt(v, "zh")).toBe("未记录");
      }
    });
  });

  it("i18n parity", () => {
    const enKeys = Object.keys(en);
    const zhKeys = Object.keys(zh);
    expect(new Set(enKeys)).toEqual(new Set(zhKeys));
    const required = [
      "deployServer","deployServerHint","deploymentDialogTitle","deploymentTargetLabel","deploymentHostEmpty","deploymentStateInstalled","deploymentStateNotInstalled","deploymentStateIncomplete","deploymentStateProbeError","deploymentStateChecking","deploymentRequirements","deploymentDeployAction","deploymentInstalledAction","deploymentProgressTitle","deploymentFailedTitle","deploymentRetry","deploymentClose","deploymentInProgress","deploymentConflictHelp","installationRemoteVersion","installationLocalVersion","installationInstalledAt","installationUnknown","installationNotRecorded","retest",
    ];
    for (const k of required) {
      expect(enKeys).toContain(k);
      expect(zhKeys).toContain(k);
    }
  });
});
