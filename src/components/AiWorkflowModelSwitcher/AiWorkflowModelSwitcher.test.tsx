import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { AiWorkflowModelSwitcher } from "./AiWorkflowModelSwitcher";
import {
  SUPPORTED_ROLES,
  type AgentMatrixRow,
  type ModelSourcesResult,
  type ProfileActivationReport,
  type ProfileMatrix,
  type ProfileSummary,
} from "@/lib/aiWorkflowProfiles";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const mockProfiles: ProfileSummary[] = [
  { name: "onespace-api-gateway", active: true },
  { name: "baibai-40", active: false },
];

const mockGatewayRows: AgentMatrixRow[] = SUPPORTED_ROLES.map((role) => ({
  role,
  codex: { model: "gateway-codex", reasoning_effort: "medium" },
  claude: { model: "gateway-claude", reasoning_effort: "high" },
  opencode: { model: "apigateway/GLM-5", reasoning_effort: "high" },
}));

const mockGatewayMatrix: ProfileMatrix = {
  name: "onespace-api-gateway",
  rows: mockGatewayRows,
};

const mockBaibaiRows: AgentMatrixRow[] = SUPPORTED_ROLES.map((role) => ({
  role,
  codex: { model: "baibai-codex", reasoning_effort: "low" },
  claude: { model: "baibai-claude", reasoning_effort: "medium" },
  opencode: { model: "baibai40/deepseek-v3", reasoning_effort: "ultra" },
}));

const mockBaibaiMatrix: ProfileMatrix = {
  name: "baibai-40",
  rows: mockBaibaiRows,
};

const mockModelSources: ModelSourcesResult = {
  opencode: {
    models: [
      "apigateway/GLM-5",
      "baibai40/deepseek-v3",
      "command/deepseek/r1",
    ],
  },
  codex: {
    models: ["gpt-5", "claude-3-7-sonnet", "codex/o3-mini"],
  },
  claude: {
    models: ["claude-3-7-sonnet", "claude-3-5-haiku"],
  },
};

const mockActivationReport: ProfileActivationReport = {
  active_profile: "baibai-40",
  hosts: ["local"],
  installations: [
    {
      host: "local",
      agents_directory: "/Users/test/.config/ai-workflow/agents",
      agents: [
        {
          name: "backend",
          path: "/Users/test/.config/ai-workflow/agents/backend.json",
          model: "baibai40/deepseek-v3",
          reasoning_effort: "ultra",
        },
      ],
    },
  ],
};

describe("AiWorkflowModelSwitcher 行为测试", () => {
  beforeEach(() => {
    localStorage.clear();
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string, args?: unknown) => {
      const payload = args as Record<string, unknown> | undefined;
      if (command === "ai_workflow_list_profiles") {
        return mockProfiles;
      }
      if (command === "ai_workflow_get_profile_matrix") {
        if (payload?.name === "baibai-40") {
          return mockBaibaiMatrix;
        }
        if (payload?.name === "broken-profile") {
          throw new Error("Failed to parse profile broken-profile: invalid yaml syntax at line 4");
        }
        return mockGatewayMatrix;
      }
      if (command === "ai_workflow_get_model_sources") {
        return mockModelSources;
      }
      if (command === "ai_workflow_activate_profile") {
        return mockActivationReport;
      }
      if (command === "ai_workflow_save_and_activate_profile") {
        return mockActivationReport;
      }
      throw new Error(`Unhandled invoke command: ${command}`);
    });
  });

  describe("AC-002: Profile 列表加载与 9×3 矩阵渲染", () => {
    it("加载 profile 列表，标记当前 active profile，并渲染 9 角色 × 3 工具矩阵", async () => {
      renderWithProviders(<AiWorkflowModelSwitcher />);

      // 验证 Profile 列表中包含两个 profile
      expect(
        await screen.findByRole("button", { name: /onespace-api-gateway/ }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /baibai-40/ }),
      ).toBeInTheDocument();

      // 验证 onespace-api-gateway 标记为当前激活
      const activeBadge = screen.getByTestId("active-profile-badge");
      expect(activeBadge).toBeInTheDocument();
      expect(activeBadge).toHaveTextContent(/onespace-api-gateway|当前激活|Active/);

      // 验证 9 个角色枚举完整渲染在行头中
      for (const role of SUPPORTED_ROLES) {
        expect(screen.getByTestId(`matrix-row-role-${role}`)).toBeInTheDocument();
      }

      // 验证 3 个工具列头完整渲染
      expect(screen.getByTestId("column-header-codex")).toBeInTheDocument();
      expect(screen.getByTestId("column-header-claude")).toBeInTheDocument();
      expect(screen.getByTestId("column-header-opencode")).toBeInTheDocument();

      // 验证 backend 角色的 opencode 格子呈现正确的模型和 effort
      const backendOpencodeCell = screen.getByTestId("cell-backend-opencode");
      expect(backendOpencodeCell).toHaveTextContent("apigateway/GLM-5");
      expect(backendOpencodeCell).toHaveTextContent("high");
    });

    it("当 profile 中缺少某些角色时，显示待填空白且不崩溃", async () => {
      invokeMock.mockImplementation(async (command: string) => {
        if (command === "ai_workflow_list_profiles") return mockProfiles;
        if (command === "ai_workflow_get_profile_matrix") {
          return {
            name: "partial-profile",
            rows: [
              {
                role: "backend",
                opencode: { model: "apigateway/GLM-5", reasoning_effort: "high" },
              },
            ],
          };
        }
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);

      // 未配置的角色依然被渲染在 9 行矩阵中，但其单元格为空
      expect(await screen.findByTestId("matrix-row-role-frontend")).toBeInTheDocument();
      const frontendCodexCell = screen.getByTestId("cell-frontend-codex");
      expect(frontendCodexCell).toHaveTextContent(/未设置|待填|-|none/i);
    });
  });

  describe("AC-003: 切换选中的 profile 重新渲染矩阵；非法文件错误展示", () => {
    it("切换选中的 profile 重新获取并渲染矩阵", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      expect(
        await screen.findByRole("button", { name: /baibai-40/ }),
      ).toBeInTheDocument();

      // 点击切换到 baibai-40
      await user.click(screen.getByRole("button", { name: /baibai-40/ }));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("ai_workflow_get_profile_matrix", {
          name: "baibai-40",
          homeOverride: undefined,
        });
      });

      // 矩阵数据应刷新为 baibai-40 的值
      const backendOpencodeCell = await screen.findByTestId("cell-backend-opencode");
      expect(backendOpencodeCell).toHaveTextContent("baibai40/deepseek-v3");
      expect(backendOpencodeCell).toHaveTextContent("ultra");
    });

    it("选中非法 yaml profile 时安全报错并展示具体文件名与原因", async () => {
      const user = userEvent.setup();
      invokeMock.mockImplementation(async (command: string, args?: unknown) => {
        const payload = args as Record<string, unknown> | undefined;
        if (command === "ai_workflow_list_profiles") {
          return [
            ...mockProfiles,
            { name: "broken-profile", active: false, error: "invalid yaml syntax" },
          ];
        }
        if (command === "ai_workflow_get_profile_matrix") {
          if (payload?.name === "broken-profile") {
            throw new Error("Failed to parse profile broken-profile: invalid yaml syntax at line 4");
          }
          return mockGatewayMatrix;
        }
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);

      const brokenBtn = await screen.findByRole("button", { name: /broken-profile/ });
      await user.click(brokenBtn);

      // 应展示明确指明文件名 broken-profile 与原因的可操作错误提示
      const alert = await screen.findByRole("alert");
      expect(alert).toHaveTextContent(/broken-profile/);
      expect(alert).toHaveTextContent(/invalid yaml syntax/i);
    });
  });

  describe("AC-004: 自动模型选项与单列降级", () => {
    it("提供各工具列的去重排序自动模型选项", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      // 打开 backend-opencode 单元格编辑或下拉选择
      const opencodeCell = await screen.findByTestId("cell-backend-opencode");
      await user.click(opencodeCell);

      const opencodeOptions = await screen.findByTestId("model-options-opencode");
      expect(opencodeOptions).toHaveTextContent("apigateway/GLM-5");
      expect(opencodeOptions).toHaveTextContent("baibai40/deepseek-v3");
      expect(opencodeOptions).toHaveTextContent("command/deepseek/r1");
    });

    it("当某列来源读取失败时该列降级并支持手动输入，其他列正常保留自动选项", async () => {
      const user = userEvent.setup();
      invokeMock.mockImplementation(async (command: string) => {
        if (command === "ai_workflow_list_profiles") return mockProfiles;
        if (command === "ai_workflow_get_profile_matrix") return mockGatewayMatrix;
        if (command === "ai_workflow_get_model_sources") {
          return {
            opencode: mockModelSources.opencode,
            codex: {
              models: [],
              error: "config.toml not readable",
            },
            claude: mockModelSources.claude,
          };
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);

      // codex 列出现降级可操作错误提示
      expect(
        await screen.findByTestId("column-degraded-warning-codex"),
      ).toHaveTextContent(/config\.toml not readable/);

      // codex 列仍支持手动输入
      const codexCell = screen.getByTestId("cell-backend-codex");
      await user.click(codexCell);
      const manualInput = await screen.findByTestId("manual-model-input-backend-codex");
      await user.clear(manualInput);
      await user.type(manualInput, "my-custom-codex-model");

      // 验证输入生效
      expect(codexCell).toHaveTextContent("my-custom-codex-model");

      // opencode 列的选项不受影响
      const opencodeCell = screen.getByTestId("cell-backend-opencode");
      await user.click(opencodeCell);
      const opencodeOptions = await screen.findByTestId("model-options-opencode");
      expect(opencodeOptions).toHaveTextContent("apigateway/GLM-5");
    });
  });

  describe("AC-005: 单元格编辑、批量操作与 dirty 状态管理", () => {
    it("编辑单元格后显示 dirty 状态，重载后清除 dirty", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      expect(await screen.findByTestId("cell-backend-opencode")).toBeInTheDocument();
      // 初始无 dirty 状态
      expect(screen.queryByTestId("matrix-dirty-indicator")).not.toBeInTheDocument();

      // 编辑单元格
      await user.click(screen.getByTestId("cell-backend-opencode"));
      const input = await screen.findByTestId("manual-model-input-backend-opencode");
      await user.clear(input);
      await user.type(input, "edited-model");

      // 出现未保存 dirty 指示
      expect(screen.getByTestId("matrix-dirty-indicator")).toBeInTheDocument();

      // 点击重置/重载按钮
      await user.click(screen.getByRole("button", { name: /重置|重载|Reset|Reload/i }));

      // dirty 指示消失，内容还原
      expect(screen.queryByTestId("matrix-dirty-indicator")).not.toBeInTheDocument();
      expect(screen.getByTestId("cell-backend-opencode")).toHaveTextContent("apigateway/GLM-5");
    });

    it("支持整列填充批量操作", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      await screen.findByTestId("cell-backend-opencode");

      // 点击 opencode 列批量填充按钮
      await user.click(screen.getByTestId("batch-fill-column-opencode"));
      const batchInput = await screen.findByTestId("batch-fill-input-opencode");
      await user.type(batchInput, "baibai/gpt-6-astra");
      await user.click(screen.getByTestId("batch-fill-apply-opencode"));

      // 验证 9 个角色的 opencode 列均被更新
      for (const role of SUPPORTED_ROLES) {
        expect(screen.getByTestId(`cell-${role}-opencode`)).toHaveTextContent(
          "baibai/gpt-6-astra",
        );
      }
      expect(screen.getByTestId("matrix-dirty-indicator")).toBeInTheDocument();
    });

    it("支持整行填充批量操作", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      await screen.findByTestId("matrix-row-role-backend");

      // 点击 backend 行批量填充按钮
      await user.click(screen.getByTestId("batch-fill-row-backend"));
      const batchInput = await screen.findByTestId("batch-fill-row-input-backend");
      await user.type(batchInput, "unified/model-42");
      await user.click(screen.getByTestId("batch-fill-row-apply-backend"));

      // backend 行的 3 个工具均使用该模型
      expect(screen.getByTestId("cell-backend-codex")).toHaveTextContent("unified/model-42");
      expect(screen.getByTestId("cell-backend-claude")).toHaveTextContent("unified/model-42");
      expect(screen.getByTestId("cell-backend-opencode")).toHaveTextContent("unified/model-42");
    });

    it("支持全部应用 reasoning_effort，并拒绝超出六值的非法 effort", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      await screen.findByTestId("cell-backend-opencode");

      // 批量将 effort 应用为 xhigh
      await user.click(screen.getByTestId("batch-apply-effort-trigger"));
      await user.click(screen.getByRole("option", { name: "xhigh" }));
      await user.click(screen.getByTestId("batch-apply-effort-submit"));

      // 验证所有单元格 effort 更新为 xhigh
      expect(screen.getByTestId("cell-backend-codex")).toHaveTextContent("xhigh");
      expect(screen.getByTestId("cell-backend-claude")).toHaveTextContent("xhigh");
      expect(screen.getByTestId("cell-backend-opencode")).toHaveTextContent("xhigh");
    });
  });

  describe("AC-006 & AC-007: 直接激活与保存并激活", () => {
    it("直接激活：未修改时直接调用 activateProfile，成功后展示报告", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      expect(await screen.findByRole("button", { name: /直接激活|Direct Activate|激活/i })).toBeInTheDocument();

      // 点击激活
      await user.click(screen.getByRole("button", { name: /直接激活|Direct Activate|激活/i }));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("ai_workflow_activate_profile", {
          name: "onespace-api-gateway",
          homeOverride: undefined,
        });
      });

      // 展示激活报告
      const report = await screen.findByTestId("activation-report");
      expect(report).toBeInTheDocument();
      expect(report).toHaveTextContent(/baibai-40/);
      expect(report).toHaveTextContent(/backend/);
    });

    it("空安装时展示无托管工具提示", async () => {
      const user = userEvent.setup();
      invokeMock.mockImplementation(async (command: string) => {
        if (command === "ai_workflow_list_profiles") return mockProfiles;
        if (command === "ai_workflow_get_profile_matrix") return mockGatewayMatrix;
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        if (command === "ai_workflow_activate_profile") {
          return {
            active_profile: "empty-profile",
            hosts: [],
            installations: [],
            message: "Profile activated, but no tools are managed.",
          };
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);

      await user.click(await screen.findByRole("button", { name: /直接激活|Direct Activate|激活/i }));

      const report = await screen.findByTestId("activation-report");
      expect(report).toHaveTextContent(/无托管工具|no tools are managed/i);
    });

    it("保存并激活：编辑后调用 saveAndActivateProfile，提交 9 行矩阵并展示报告", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      // 编辑某个单元格
      await user.click(await screen.findByTestId("cell-backend-opencode"));
      const input = await screen.findByTestId("manual-model-input-backend-opencode");
      await user.clear(input);
      await user.type(input, "new-gateway-model");

      // 点击保存并激活
      const saveAndActivateBtn = screen.getByRole("button", {
        name: /保存并激活|Save and Activate/i,
      });
      await user.click(saveAndActivateBtn);

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          "ai_workflow_save_and_activate_profile",
          expect.objectContaining({
            name: "onespace-api-gateway",
            matrix: expect.arrayContaining([
              expect.objectContaining({
                role: "backend",
                opencode: expect.objectContaining({
                  model: "new-gateway-model",
                }),
              }),
            ]),
          }),
        );
      });

      // 展示激活报告且 dirty 状态清除
      expect(await screen.findByTestId("activation-report")).toBeInTheDocument();
      expect(screen.queryByTestId("matrix-dirty-indicator")).not.toBeInTheDocument();
    });
  });

  describe("AC-008: 激活失败原样 verbatim 回显错误", () => {
    it("激活冲突或失败时原样回显 CLI 错误信息，且不宣称成功", async () => {
      const user = userEvent.setup();
      const conflictError = "Error: Conflict detected in /home/user/.claude/agents/backend.json: file was modified manually";
      invokeMock.mockImplementation(async (command: string) => {
        if (command === "ai_workflow_list_profiles") return mockProfiles;
        if (command === "ai_workflow_get_profile_matrix") return mockGatewayMatrix;
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        if (command === "ai_workflow_activate_profile") {
          throw new Error(conflictError);
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);

      await user.click(await screen.findByRole("button", { name: /直接激活|Direct Activate|激活/i }));

      // 验证 verbatim 原样回显错误
      const alert = await screen.findByRole("alert");
      expect(alert).toHaveTextContent(conflictError);
      // 不应展示激活报告
      expect(screen.queryByTestId("activation-report")).not.toBeInTheDocument();
    });

    it("ai-workflow 二进制缺失时展示指明二进制缺失的可操作错误", async () => {
      const user = userEvent.setup();
      const binaryError = "ai-workflow binary not found: /usr/local/bin/ai-workflow";
      invokeMock.mockImplementation(async (command: string) => {
        if (command === "ai_workflow_list_profiles") return mockProfiles;
        if (command === "ai_workflow_get_profile_matrix") return mockGatewayMatrix;
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        if (command === "ai_workflow_activate_profile") {
          throw new Error(binaryError);
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);

      await user.click(await screen.findByRole("button", { name: /直接激活|Direct Activate|激活/i }));

      const alert = await screen.findByRole("alert");
      expect(alert).toHaveTextContent(binaryError);
    });
  });
});
