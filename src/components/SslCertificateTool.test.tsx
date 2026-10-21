import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SslCertificateTool } from "@/components/SslCertificateTool";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const { pushToastMock } = vi.hoisted(() => ({ pushToastMock: vi.fn() }));

vi.mock("@/components/ToastProvider", async () => {
  const actual = await vi.importActual<
    typeof import("@/components/ToastProvider")
  >("@/components/ToastProvider");
  return {
    ...actual,
    useToast: () => ({ pushToast: pushToastMock, dismissToast: vi.fn() }),
  };
});

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

describe("SslCertificateTool", () => {
  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "ssl_watchlist_list") {
        return [
          {
            id: "watch-1",
            domain: "example.com",
            port: 443,
            alias: "生产主站",
            enabled: true,
            alertDaysThreshold: 30,
            lastDaysRemaining: 45,
            lastStatus: "valid",
            createdAt: 1700000000,
          },
        ];
      }
      if (command === "ssl_certificate_list") {
        return [
          {
            id: "cert-1",
            primaryDomain: "example.com",
            sanDomains: ["www.example.com"],
            issuer: "Let's Encrypt",
            issuedAt: 1700000000,
            expiresAt: 1707777777,
            daysRemaining: 80,
            keyType: "ecc_p256",
            certPem: "---CERT---",
            chainPem: "---CHAIN---",
            fullchainPem: "---FULLCHAIN---",
            privkeyPem: "---PRIVKEY---",
            nginxConfig: "server { listen 443; }",
            createdAt: 1700000000,
          },
        ];
      }
      if (command === "ssl_check_certificate") {
        return {
          domain: "example.com",
          port: 443,
          ipAddress: "93.184.216.34",
          handshakeSuccess: true,
          tlsVersion: "TLSv1.3",
          cipherSuite: "TLS_AES_256_GCM_SHA384",
          isValid: true,
          validationError: null,
          daysRemaining: 75,
          isExpired: false,
          isExpiringSoon: false,
          domainMatches: true,
          certificates: [
            {
              subject: "CN=example.com",
              subjectCn: "example.com",
              issuer: "CN=R3,O=Let's Encrypt",
              issuerCn: "R3",
              issuerOrg: "Let's Encrypt",
              notBefore: 1700000000,
              notAfter: 1707777777,
              daysRemaining: 75,
              sans: ["example.com", "www.example.com"],
              serialNumber: "03:AA:BB:CC",
              signatureAlgorithm: "sha256WithRSAEncryption",
              publicKeyAlgorithm: "id-ecPublicKey",
              sha256Fingerprint: "11:22:33:44:55:66",
              isRoot: false,
              isCa: false,
            },
          ],
        };
      }
      return null;
    });
  });

  it("renders header and tabs properly", async () => {
    await act(async () => {
      renderWithProviders(<SslCertificateTool />);
    });

    expect(screen.getByText("SSL 证书助手")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /健康检测与监控/ })
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /免费申请与 Nginx 部署/ })
    ).toBeInTheDocument();
  });

  it("renders loaded watchlist and displays domain status", async () => {
    await act(async () => {
      renderWithProviders(<SslCertificateTool />);
    });

    expect(screen.getByText("example.com:443")).toBeInTheDocument();
    expect(screen.getByText("生产主站")).toBeInTheDocument();
    expect(screen.getByText("剩余 45 天")).toBeInTheDocument();
  });

  it("can switch tabs to issue mode and view issued certificate list", async () => {
    const user = userEvent.setup();
    await act(async () => {
      renderWithProviders(<SslCertificateTool />);
    });

    const issueTabBtn = screen.getByRole("button", {
      name: /免费申请与 Nginx 部署/,
    });
    await user.click(issueTabBtn);

    expect(screen.getByText("本地已申请证书库 (1)")).toBeInTheDocument();
    expect(screen.getByText("查看与复制配置")).toBeInTheDocument();
    expect(screen.getByText("导出 Nginx 文件包")).toBeInTheDocument();
  });

  it("records check history, allows re-testing by clicking history pill, and clears history", async () => {
    const user = userEvent.setup();
    window.localStorage.clear();
    await act(async () => {
      renderWithProviders(<SslCertificateTool />);
    });

    const input = screen.getByPlaceholderText(/输入域名/);
    await user.type(input, "huanweiyun.com");
    const checkBtn = screen.getByRole("button", { name: "立即检测" });
    await user.click(checkBtn);

    // Verify history pill is displayed
    expect(await screen.findByTestId("ssl-history-item-huanweiyun.com")).toBeInTheDocument();

    // Type a second domain and check
    await user.clear(input);
    await user.type(input, "saas.legutech.com");
    await user.click(checkBtn);

    expect(await screen.findByTestId("ssl-history-item-saas.legutech.com")).toBeInTheDocument();

    // Click the first history pill to re-test
    const huanweiPill = screen.getByTestId("ssl-history-item-huanweiyun.com");
    await user.click(huanweiPill);
    expect(input).toHaveValue("huanweiyun.com");

    // Clear history
    const clearBtn = screen.getByTestId("ssl-history-clear");
    await user.click(clearBtn);
    expect(screen.queryByTestId("ssl-check-history-list")).not.toBeInTheDocument();
  });
});
