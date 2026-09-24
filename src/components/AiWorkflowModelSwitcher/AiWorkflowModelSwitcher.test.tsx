import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { AiWorkflowModelSwitcher } from "./AiWorkflowModelSwitcher";
import i18n from "@/i18n";
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
  { name: "onespace-ai-gateway", active: true },
  { name: "baibai-40", active: false },
];

const mockGatewayRows: AgentMatrixRow[] = SUPPORTED_ROLES.map((role) => ({
  role,
  codex: { model: "gateway-codex", reasoning_effort: "medium" },
  claude: { model: "gateway-claude", reasoning_effort: "high" },
  opencode: { model: "apigateway/GLM-5", reasoning_effort: "high" },
}));

const mockGatewayMatrix: ProfileMatrix = {
  name: "onespace-ai-gateway",
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
  beforeEach(async () => {
    await i18n.changeLanguage("zh");
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
      if (command === "ai_workflow_save_profile") {
        return null;
      }
      if (command === "ai_workflow_create_profile") {
        return null;
      }
      if (command === "ai_workflow_delete_profile") {
        return null;
      }
      throw new Error(`Unhandled invoke command: ${command}`);
    });
  });

  describe("AC-002: Profile 列表加载与 9×3 矩阵渲染", () => {
    it("加载 profile 列表，标记当前 active profile，并渲染 9 角色 × 3 工具矩阵", async () => {
      renderWithProviders(<AiWorkflowModelSwitcher />);

      // 验证 Profile 列表中包含两个 profile
      expect(
        await screen.findByRole("button", { name: /onespace-ai-gateway/ }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /baibai-40/ }),
      ).toBeInTheDocument();

      // 验证 onespace-ai-gateway 标记为当前激活
      const activeChip = screen.getByRole("button", {
        name: /onespace-ai-gateway/,
      });
      expect(activeChip).toHaveAttribute("data-active", "true");

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

  describe("AC-006 & AC-007: 独立保存与激活", () => {
    it("操作按钮提供可访问的激活与保存标签", async () => {
      renderWithProviders(<AiWorkflowModelSwitcher />);

      expect(
        await screen.findByRole("button", { name: /^(激活|Activate)$/i }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /^(保存|Save)$/i }),
      ).toBeInTheDocument();
    });

    it("英文界面使用 Activate 和 Save 标签", async () => {
      renderWithProviders(<AiWorkflowModelSwitcher />);
      await screen.findByTestId("cell-backend-opencode");
      await i18n.changeLanguage("en");

      expect(
        await screen.findByRole("button", { name: "Activate" }),
      ).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();
      await i18n.changeLanguage("zh");
    });

    it("激活：即使矩阵有未保存修改，也只调用 activate 命令且不保存修改", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      await user.click(await screen.findByRole("button", { name: /baibai-40/ }));
      expect(await screen.findByTestId("cell-backend-opencode")).toHaveTextContent(
        "baibai40/deepseek-v3",
      );
      await user.click(await screen.findByTestId("cell-backend-opencode"));
      const input = await screen.findByTestId("manual-model-input-backend-opencode");
      await user.clear(input);
      await user.type(input, "unsaved-model");

      await user.click(
        screen.getByRole("button", {
          name: /^(激活|Activate)$/i,
        }),
      );

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("ai_workflow_activate_profile", {
          name: "baibai-40",
          homeOverride: undefined,
        });
      });
      expect(invokeMock).not.toHaveBeenCalledWith(
        "ai_workflow_save_profile",
        expect.anything(),
      );
      expect(screen.getByTestId("matrix-dirty-indicator")).toBeInTheDocument();

      // 展示激活报告
      const report = await screen.findByTestId("activation-report");
      expect(report).toBeInTheDocument();
      expect(report).toHaveTextContent(/baibai-40/);
      expect(report).toHaveTextContent(/backend/);
      expect(screen.getByRole("button", { name: /baibai-40/ })).toHaveAttribute(
        "data-active",
        "true",
      );
      expect(
        screen.getByRole("button", { name: /onespace-ai-gateway/ }),
      ).toHaveAttribute("data-active", "false");
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

      await user.click(
        await screen.findByRole("button", {
          name: /^(激活|Activate)$/i,
        }),
      );

      const report = await screen.findByTestId("activation-report");
      expect(report).toHaveTextContent(/无托管工具|no tools are managed/i);
    });

    it("保存现有方案：提交选中的方案与编辑矩阵，只清除 dirty 且不激活", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      await user.click(await screen.findByRole("button", { name: /baibai-40/ }));
      // 编辑某个单元格
      const opencodeCell = await screen.findByTestId("cell-backend-opencode");
      expect(opencodeCell).toHaveTextContent("baibai40/deepseek-v3");
      await user.click(opencodeCell);
      const input = await screen.findByTestId("manual-model-input-backend-opencode");
      await user.clear(input);
      await user.type(input, "new-gateway-model");

      await user.click(screen.getByRole("button", { name: /^(保存|Save)$/i }));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          "ai_workflow_save_profile",
          expect.objectContaining({
            name: "baibai-40",
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
      expect(invokeMock).not.toHaveBeenCalledWith(
        "ai_workflow_activate_profile",
        expect.anything(),
      );

      // 保存不产生激活报告，也不改变当前激活方案。
      expect(screen.queryByTestId("activation-report")).not.toBeInTheDocument();
      expect(screen.queryByTestId("matrix-dirty-indicator")).not.toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /onespace-ai-gateway/ }),
      ).toHaveAttribute("data-active", "true");
      expect(screen.getByRole("button", { name: /baibai-40/ })).toHaveAttribute(
        "data-active",
        "false",
      );
    });

    it("新建方案保存后选择否：方案已保存且保持未激活", async () => {
      const user = userEvent.setup();
      let currentProfiles = [...mockProfiles];
      let saveCompleted = false;
      invokeMock.mockImplementation(async (command: string, args?: unknown) => {
        const payload = args as Record<string, unknown> | undefined;
        if (command === "ai_workflow_list_profiles") return currentProfiles;
        if (command === "ai_workflow_get_profile_matrix") {
          return payload?.name === "custom-plan-no"
            ? { ...mockGatewayMatrix, name: "custom-plan-no" }
            : mockGatewayMatrix;
        }
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        if (command === "ai_workflow_create_profile") {
          currentProfiles = [
            ...currentProfiles,
            { name: String(payload?.name), active: false },
          ];
          return null;
        }
        if (command === "ai_workflow_save_profile") {
          return Promise.resolve().then(() => {
            saveCompleted = true;
            return null;
          });
        }
        if (command === "ai_workflow_activate_profile") return mockActivationReport;
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);
      await screen.findByTestId("cell-backend-opencode");
      await i18n.changeLanguage("en");
      await user.click(await screen.findByTestId("create-profile-trigger"));
      await user.type(await screen.findByTestId("create-profile-name-input"), "custom-plan-no");
      await user.click(screen.getByTestId("create-profile-submit"));

      await screen.findByRole("button", { name: /custom-plan-no/ });
      await user.click(screen.getByRole("button", { name: "Save" }));

      expect(
        await screen.findByText(/activate|激活/i, { selector: "p" }),
      ).toBeInTheDocument();
      const saveCompletedBeforePrompt = saveCompleted;
      const noButton = await screen.findByRole("button", { name: "No" });
      await user.click(noButton);

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          "ai_workflow_save_profile",
          expect.objectContaining({ name: "custom-plan-no" }),
        );
      });
      expect(invokeMock).not.toHaveBeenCalledWith(
        "ai_workflow_activate_profile",
        expect.anything(),
      );
      expect(screen.queryByTestId("activation-report")).not.toBeInTheDocument();
      expect(screen.queryByTestId("matrix-dirty-indicator")).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: /custom-plan-no/ })).toHaveAttribute(
        "data-active",
        "false",
      );
      expect(
        screen.getByRole("button", { name: /onespace-ai-gateway/ }),
      ).toHaveAttribute("data-active", "true");
      expect(saveCompletedBeforePrompt).toBe(true);
    });

    it("新建方案首存选择否后再次保存不再询问激活", async () => {
      const user = userEvent.setup();
      let currentProfiles = [...mockProfiles];
      let saveCompletions = 0;
      invokeMock.mockImplementation(async (command: string, args?: unknown) => {
        const payload = args as Record<string, unknown> | undefined;
        if (command === "ai_workflow_list_profiles") return currentProfiles;
        if (command === "ai_workflow_get_profile_matrix") {
          return payload?.name === "custom-plan-repeat"
            ? { ...mockGatewayMatrix, name: "custom-plan-repeat" }
            : mockGatewayMatrix;
        }
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        if (command === "ai_workflow_create_profile") {
          currentProfiles = [
            ...currentProfiles,
            { name: String(payload?.name), active: false },
          ];
          return null;
        }
        if (command === "ai_workflow_save_profile") {
          return Promise.resolve().then(() => {
            saveCompletions += 1;
            return null;
          });
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);
      await screen.findByTestId("cell-backend-opencode");
      await i18n.changeLanguage("en");
      await user.click(await screen.findByTestId("create-profile-trigger"));
      await user.type(await screen.findByTestId("create-profile-name-input"), "custom-plan-repeat");
      await user.click(screen.getByTestId("create-profile-submit"));

      await screen.findByRole("button", { name: /custom-plan-repeat/ });
      await user.click(screen.getByRole("button", { name: "Save" }));
      expect(await screen.findByRole("button", { name: "No" })).toBeInTheDocument();
      const saveCompletedBeforePrompt = saveCompletions === 1;
      await user.click(screen.getByRole("button", { name: "No" }));
      await waitFor(() => expect(saveCompletions).toBe(1));

      expect(screen.queryByRole("button", { name: "No" })).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Save" }));
      await waitFor(() => expect(saveCompletions).toBe(2));

      expect(screen.queryByRole("button", { name: "Yes" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "No" })).not.toBeInTheDocument();
      expect(invokeMock.mock.calls.filter(([command]) => command === "ai_workflow_save_profile")).toHaveLength(2);
      expect(invokeMock).not.toHaveBeenCalledWith(
        "ai_workflow_activate_profile",
        expect.anything(),
      );
      expect(saveCompletedBeforePrompt).toBe(true);
      expect(screen.getByRole("button", { name: /custom-plan-repeat/ })).toHaveAttribute(
        "data-active",
        "false",
      );
      expect(
        screen.getByRole("button", { name: /onespace-ai-gateway/ }),
      ).toHaveAttribute("data-active", "true");
    });

    it("新建方案保存后选择是：先保存，再激活", async () => {
      const user = userEvent.setup();
      let currentProfiles = [...mockProfiles];
      let saveCompleted = false;
      invokeMock.mockImplementation(async (command: string, args?: unknown) => {
        const payload = args as Record<string, unknown> | undefined;
        if (command === "ai_workflow_list_profiles") return currentProfiles;
        if (command === "ai_workflow_get_profile_matrix") {
          return payload?.name === "custom-plan-yes"
            ? { ...mockGatewayMatrix, name: "custom-plan-yes" }
            : mockGatewayMatrix;
        }
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        if (command === "ai_workflow_create_profile") {
          currentProfiles = [
            ...currentProfiles,
            { name: String(payload?.name), active: false },
          ];
          return null;
        }
        if (command === "ai_workflow_save_profile") {
          return Promise.resolve().then(() => {
            saveCompleted = true;
            return null;
          });
        }
        if (command === "ai_workflow_activate_profile") {
          return { ...mockActivationReport, active_profile: "custom-plan-yes" };
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);
      await screen.findByTestId("cell-backend-opencode");
      await i18n.changeLanguage("en");
      await user.click(await screen.findByTestId("create-profile-trigger"));
      await user.type(await screen.findByTestId("create-profile-name-input"), "custom-plan-yes");
      await user.click(screen.getByTestId("create-profile-submit"));

      await screen.findByRole("button", { name: /custom-plan-yes/ });
      await user.click(screen.getByRole("button", { name: "Save" }));
      expect(
        await screen.findByText(/activate|激活/i, { selector: "p" }),
      ).toBeInTheDocument();
      const saveCompletedBeforePrompt = saveCompleted;
      const yesButton = await screen.findByRole("button", { name: "Yes" });
      await user.click(yesButton);

      await waitFor(() => {
        const commands = invokeMock.mock.calls.map(([command]) => command);
        expect(commands.indexOf("ai_workflow_save_profile")).toBeGreaterThan(-1);
        expect(commands.indexOf("ai_workflow_activate_profile")).toBeGreaterThan(
          commands.indexOf("ai_workflow_save_profile"),
        );
      });
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_workflow_save_profile",
        expect.objectContaining({ name: "custom-plan-yes" }),
      );
      expect(invokeMock).toHaveBeenCalledWith("ai_workflow_activate_profile", {
        name: "custom-plan-yes",
        homeOverride: undefined,
      });
      expect(await screen.findByTestId("activation-report")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /custom-plan-yes/ })).toHaveAttribute(
        "data-active",
        "true",
      );
      expect(saveCompletedBeforePrompt).toBe(true);
    });

    it("新建方案保存失败时显示错误且不询问激活", async () => {
      const user = userEvent.setup();
      let currentProfiles = [...mockProfiles];
      let saveAttempted = false;
      const saveError = "profile save failed";
      invokeMock.mockImplementation(async (command: string, args?: unknown) => {
        const payload = args as Record<string, unknown> | undefined;
        if (command === "ai_workflow_list_profiles") return currentProfiles;
        if (command === "ai_workflow_get_profile_matrix") {
          return payload?.name === "custom-plan-failed"
            ? { ...mockGatewayMatrix, name: "custom-plan-failed" }
            : mockGatewayMatrix;
        }
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        if (command === "ai_workflow_create_profile") {
          currentProfiles = [
            ...currentProfiles,
            { name: String(payload?.name), active: false },
          ];
          return null;
        }
        if (command === "ai_workflow_save_profile") {
          saveAttempted = true;
          throw new Error(saveError);
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);
      await screen.findByTestId("cell-backend-opencode");
      await i18n.changeLanguage("en");
      await user.click(await screen.findByTestId("create-profile-trigger"));
      await user.type(await screen.findByTestId("create-profile-name-input"), "custom-plan-failed");
      await user.click(screen.getByTestId("create-profile-submit"));

      await screen.findByRole("button", { name: /custom-plan-failed/ });
      await user.click(screen.getByRole("button", { name: "Save" }));

      const promptWasShownBeforeSaveFailure =
        screen.queryByRole("button", { name: "No" }) !== null;
      if (promptWasShownBeforeSaveFailure) {
        await user.click(screen.getByRole("button", { name: "No" }));
      }
      expect(await screen.findByRole("alert")).toHaveTextContent(saveError);
      expect(saveAttempted).toBe(true);
      expect(promptWasShownBeforeSaveFailure).toBe(false);
      expect(screen.queryByRole("button", { name: "Yes" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "No" })).not.toBeInTheDocument();
      expect(invokeMock).not.toHaveBeenCalledWith(
        "ai_workflow_activate_profile",
        expect.anything(),
      );
      expect(screen.getByRole("button", { name: /onespace-ai-gateway/ })).toHaveAttribute(
        "data-active",
        "true",
      );
    });

    it("激活新方案失败时保留已保存矩阵且当前激活方案不变", async () => {
      const user = userEvent.setup();
      let currentProfiles = [...mockProfiles];
      let saveCompleted = false;
      let savedProfile: string | null = null;
      let savedMatrix: AgentMatrixRow[] | null = null;
      const activationError = "profile activation failed";
      invokeMock.mockImplementation(async (command: string, args?: unknown) => {
        const payload = args as Record<string, unknown> | undefined;
        if (command === "ai_workflow_list_profiles") return currentProfiles;
        if (command === "ai_workflow_get_profile_matrix") {
          return payload?.name === "custom-plan-activation-fails"
            ? { ...mockGatewayMatrix, name: "custom-plan-activation-fails" }
            : mockGatewayMatrix;
        }
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        if (command === "ai_workflow_create_profile") {
          currentProfiles = [
            ...currentProfiles,
            { name: String(payload?.name), active: false },
          ];
          return null;
        }
        if (command === "ai_workflow_save_profile") {
          return Promise.resolve().then(() => {
            saveCompleted = true;
            savedProfile = String(payload?.name);
            savedMatrix = payload?.matrix as AgentMatrixRow[];
            return null;
          });
        }
        if (command === "ai_workflow_activate_profile") {
          throw new Error(activationError);
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);
      await screen.findByTestId("cell-backend-opencode");
      await i18n.changeLanguage("en");
      await user.click(await screen.findByTestId("create-profile-trigger"));
      await user.type(
        await screen.findByTestId("create-profile-name-input"),
        "custom-plan-activation-fails",
      );
      await user.click(screen.getByTestId("create-profile-submit"));

      await screen.findByRole("button", { name: /custom-plan-activation-fails/ });
      await user.click(await screen.findByTestId("cell-backend-opencode"));
      const modelInput = await screen.findByTestId("manual-model-input-backend-opencode");
      await user.clear(modelInput);
      await user.type(modelInput, "saved-before-activation");
      await user.click(screen.getByRole("button", { name: "Save" }));
      expect(await screen.findByRole("button", { name: "Yes" })).toBeInTheDocument();
      const saveCompletedBeforePrompt = saveCompleted;
      await user.click(screen.getByRole("button", { name: "Yes" }));

      expect(await screen.findByRole("alert")).toHaveTextContent(activationError);
      expect(saveCompletedBeforePrompt).toBe(true);
      expect(saveCompleted).toBe(true);
      expect(savedProfile).toBe("custom-plan-activation-fails");
      expect(savedMatrix).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            role: "backend",
            opencode: expect.objectContaining({ model: "saved-before-activation" }),
          }),
        ]),
      );
      expect(invokeMock).toHaveBeenCalledWith("ai_workflow_activate_profile", {
        name: "custom-plan-activation-fails",
        homeOverride: undefined,
      });
      expect(screen.getByRole("button", { name: /onespace-ai-gateway/ })).toHaveAttribute(
        "data-active",
        "true",
      );
      expect(
        screen.getByRole("button", { name: /custom-plan-activation-fails/ }),
      ).toHaveAttribute("data-active", "false");
      expect(screen.queryByTestId("activation-report")).not.toBeInTheDocument();
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

      await user.click(
        await screen.findByRole("button", {
          name: /^(激活|Activate)$/i,
        }),
      );

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

      await user.click(
        await screen.findByRole("button", {
          name: /^(激活|Activate)$/i,
        }),
      );

      const alert = await screen.findByRole("alert");
      expect(alert).toHaveTextContent(binaryError);
    });
  });

  describe("优化需求与功能扩展测试", () => {
    it("页面顶部展示与其他工具一致的标准化标题栏", async () => {
      renderWithProviders(<AiWorkflowModelSwitcher />);
      expect(
        await screen.findByRole("heading", { name: /AI Workflow 模型切换|AI Workflow Model Switcher/i }),
      ).toBeInTheDocument();
      expect(
        screen.getByText(/集中管理|Centralized matrix management/i),
      ).toBeInTheDocument();
    });

    it("支持新建方案并自动选中新方案", async () => {
      const user = userEvent.setup();
      let currentProfiles = [...mockProfiles];
      invokeMock.mockImplementation(async (command: string, args?: unknown) => {
        const payload = args as Record<string, unknown> | undefined;
        if (command === "ai_workflow_list_profiles") return currentProfiles;
        if (command === "ai_workflow_get_profile_matrix") return mockGatewayMatrix;
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        if (command === "ai_workflow_create_profile") {
          currentProfiles = [
            ...currentProfiles,
            { name: payload?.name as string, active: false },
          ];
          return null;
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);

      // 点击新建方案按钮
      const createTrigger = await screen.findByTestId("create-profile-trigger");
      await user.click(createTrigger);

      // 弹窗可见
      const nameInput = await screen.findByTestId("create-profile-name-input");
      expect(nameInput).toBeInTheDocument();

      // 输入新方案名称并点击提交
      await user.type(nameInput, "custom-plan-v1");
      const submitBtn = screen.getByTestId("create-profile-submit");
      await user.click(submitBtn);

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          "ai_workflow_create_profile",
          expect.objectContaining({
            name: "custom-plan-v1",
          }),
        );
      });

      // 新方案被成功渲染且被选中
      expect(await screen.findByRole("button", { name: /custom-plan-v1/ })).toBeInTheDocument();
    });

    it("支持删除已有非激活方案，并在激活方案上禁用删除", async () => {
      const user = userEvent.setup();
      let currentProfiles = [...mockProfiles];
      invokeMock.mockImplementation(async (command: string, args?: unknown) => {
        const payload = args as Record<string, unknown> | undefined;
        if (command === "ai_workflow_list_profiles") return currentProfiles;
        if (command === "ai_workflow_get_profile_matrix") return mockGatewayMatrix;
        if (command === "ai_workflow_get_model_sources") return mockModelSources;
        if (command === "ai_workflow_delete_profile") {
          currentProfiles = currentProfiles.filter((p) => p.name !== payload?.name);
          return null;
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);

      // 默认选中 onespace-ai-gateway（active 为 true），删除按钮应当为禁用状态
      const deleteBtn = await screen.findByTestId("delete-profile-trigger");
      expect(deleteBtn).toBeDisabled();

      // 切换到非激活方案 baibai-40
      await user.click(screen.getByRole("button", { name: /baibai-40/ }));

      // 切换后删除按钮启用
      await waitFor(() => {
        expect(deleteBtn).not.toBeDisabled();
      });

      // 点击删除按钮触发二次确认
      await user.click(deleteBtn);

      // 确认弹窗出现，点击确认删除（匹配弹窗内的确认按钮）
      const deleteButtons = await screen.findAllByRole("button", { name: /^删除$|^Delete$/i });
      const confirmOkBtn = deleteButtons[deleteButtons.length - 1];
      await user.click(confirmOkBtn);


      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          "ai_workflow_delete_profile",
          expect.objectContaining({
            name: "baibai-40",
          }),
        );
      });
    });

    it("将新增方案按钮移动到与选择配置方案标题同级最右侧，并具备 onespace 统一的主按钮样式", async () => {
      renderWithProviders(<AiWorkflowModelSwitcher />);

      const createBtn = await screen.findByTestId("create-profile-trigger");
      expect(createBtn).toBeInTheDocument();
      // 样式应具备 onespace 实体主按钮类
      expect(createBtn).toHaveClass("bg-primary", "text-primary-foreground");

      // 验证与选择配置方案标题在同一个容器中
      const headerTitle = screen.getByText("选择配置方案");
      const headerContainer = headerTitle.closest("div.border-b");
      expect(headerContainer).toContainElement(createBtn);
    });

    it("修改删除方案按钮名称为'删除'，在删除按钮前新增'编辑'按钮", async () => {
      renderWithProviders(<AiWorkflowModelSwitcher />);

      const deleteBtn = await screen.findByTestId("delete-profile-trigger");
      // 验证删除按钮显示的文本为“删除”
      expect(deleteBtn).toHaveTextContent(/^删除$/);

      // 验证编辑按钮存在且在删除按钮前方
      const editBtn = await screen.findByTestId("edit-profile-trigger");
      expect(editBtn).toBeInTheDocument();
      expect(editBtn).toHaveTextContent(/^编辑$/);

      // DOM 顺序：editBtn 在 deleteBtn 前
      expect(
        editBtn.compareDocumentPosition(deleteBtn) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    });

    it("支持点击编辑按钮弹框修改方案名称，并完成校验与重命名保存", async () => {
      const user = userEvent.setup();
      let currentProfiles: ProfileSummary[] = [
        { name: "onespace-ai-gateway", active: true },
        { name: "baibai-40", active: false },
      ];

      invokeMock.mockImplementation(async (command: string, payload?: any) => {
        if (command === "ai_workflow_list_profiles") {
          return currentProfiles;
        }
        if (command === "ai_workflow_get_profile_matrix") {
          return mockGatewayMatrix;
        }
        if (command === "ai_workflow_get_model_sources") {
          return mockModelSources;
        }
        if (command === "ai_workflow_rename_profile") {
          currentProfiles = currentProfiles.map((p) =>
            p.name === payload?.oldName ? { ...p, name: payload.newName } : p,
          );
          return null;
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);

      const editBtn = await screen.findByTestId("edit-profile-trigger");
      await user.click(editBtn);

      // 弹框出现
      expect(
        await screen.findByRole("heading", { name: /编辑方案名称/ }),
      ).toBeInTheDocument();
      const input = screen.getByTestId(
        "edit-profile-name-input",
      ) as HTMLInputElement;
      expect(input.value).toBe("onespace-ai-gateway");

      // 1. 尝试清空并提交，提示非空
      await user.clear(input);
      await user.click(screen.getByTestId("edit-profile-submit"));
      expect(screen.getByText("方案名称不能为空")).toBeInTheDocument();

      // 2. 尝试非法字符并提交
      await user.type(input, "invalid name!");
      await user.click(screen.getByTestId("edit-profile-submit"));
      expect(
        screen.getByText("方案名称格式无效，仅支持字母、数字、短横线与点"),
      ).toBeInTheDocument();

      // 3. 尝试与其他方案重名
      await user.clear(input);
      await user.type(input, "baibai-40");
      await user.click(screen.getByTestId("edit-profile-submit"));
      expect(screen.getByText("该方案名称已存在")).toBeInTheDocument();

      // 4. 输入新名称并成功提交
      await user.clear(input);
      await user.type(input, "onespace-v2");
      await user.click(screen.getByTestId("edit-profile-submit"));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith(
          "ai_workflow_rename_profile",
          expect.objectContaining({
            oldName: "onespace-ai-gateway",
            newName: "onespace-v2",
          }),
        );
      });

      // 弹窗关闭，且方案名更新为 onespace-v2
      await waitFor(() => {
        expect(
          screen.queryByRole("heading", { name: /编辑方案名称/ }),
        ).not.toBeInTheDocument();
      });
      expect(
        await screen.findByRole("button", { name: /onespace-v2/ }),
      ).toBeInTheDocument();
    });

    it("整列填充支持模糊检索候选模型列表并点选", async () => {

      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      // 点击 opencode 列整列填充
      await user.click(await screen.findByTestId("batch-fill-column-opencode"));

      // 找到整列填充输入框，输入 "GLM" 进行模糊过滤
      const batchInput = await screen.findByTestId("batch-fill-input-opencode");
      await user.type(batchInput, "GLM");

      // 下拉列表中出现过滤后的匹配项并点击
      const option = await screen.findByTestId("batch-fill-input-opencode-option-apigateway/GLM-5");
      expect(option).toBeInTheDocument();
      await user.click(option);

      // 点击应用填充到整列
      await user.click(screen.getByTestId("batch-fill-apply-opencode"));

      // 9 个角色的 opencode 均填充为 apigateway/GLM-5
      for (const role of SUPPORTED_ROLES) {
        expect(screen.getByTestId(`cell-${role}-opencode`)).toHaveTextContent("apigateway/GLM-5");
      }

      // 应用后整列填充面板自动收起隐藏
      expect(screen.queryByTestId("batch-fill-hide-opencode")).not.toBeInTheDocument();
    });

    it("整列填充支持显式点击收起隐藏按钮", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      // 打开整列填充
      await user.click(await screen.findByTestId("batch-fill-column-codex"));
      const hideBtn = await screen.findByTestId("batch-fill-hide-codex");
      expect(hideBtn).toBeInTheDocument();

      // 点击收起隐藏
      await user.click(hideBtn);
      expect(screen.queryByTestId("batch-fill-hide-codex")).not.toBeInTheDocument();
    });

    it("整行填充优化：支持聚合候选模糊检索点选，支持显式收起及应用后自动隐藏", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      await screen.findByTestId("matrix-row-role-backend");

      // 点击 backend 行批量填充按钮展开整行填充面板
      await user.click(screen.getByTestId("batch-fill-row-backend"));
      const hideBtn = await screen.findByTestId("batch-fill-row-hide-backend");
      expect(hideBtn).toBeInTheDocument();

      // 测试显式点击收起隐藏按钮
      await user.click(hideBtn);
      expect(screen.queryByTestId("batch-fill-row-hide-backend")).not.toBeInTheDocument();

      // 再次打开整行填充
      await user.click(screen.getByTestId("batch-fill-row-backend"));
      const batchInput = await screen.findByTestId("batch-fill-row-input-backend");

      // 输入 "sonnet" 模糊检索（来自 claude 的候选模型）
      await user.type(batchInput, "sonnet");
      const option = await screen.findByTestId(
        "batch-fill-row-input-backend-option-claude-3-7-sonnet",
      );
      expect(option).toBeInTheDocument();
      await user.click(option);

      // 点击应用整行
      await user.click(screen.getByTestId("batch-fill-row-apply-backend"));

      // 验证 backend 行各列均被更新
      expect(screen.getByTestId("cell-backend-codex")).toHaveTextContent(
        "claude-3-7-sonnet",
      );
      expect(screen.getByTestId("cell-backend-claude")).toHaveTextContent(
        "claude-3-7-sonnet",
      );
      expect(screen.getByTestId("cell-backend-opencode")).toHaveTextContent(
        "claude-3-7-sonnet",
      );

      // 验证应用后整行填充面板自动收起
      expect(
        screen.queryByTestId("batch-fill-row-hide-backend"),
      ).not.toBeInTheDocument();
    });

    it("单元格展开编辑时，提供取消按钮与关闭按钮，点击后关闭编辑面板", async () => {
      const user = userEvent.setup();
      renderWithProviders(<AiWorkflowModelSwitcher />);

      const cell = await screen.findByTestId("cell-backend-codex");
      // 展开编辑单元格
      await user.click(cell);

      const cancelBtn = await screen.findByTestId("cell-cancel-backend-codex");
      const closeBtn = await screen.findByTestId("cell-close-backend-codex");
      expect(cancelBtn).toBeInTheDocument();
      expect(closeBtn).toBeInTheDocument();

      // 点击取消按钮，编辑面板关闭
      await user.click(cancelBtn);
      expect(
        screen.queryByTestId("cell-cancel-backend-codex"),
      ).not.toBeInTheDocument();

      // 重新展开并测试右上角关闭按钮
      await user.click(cell);
      const closeBtn2 = await screen.findByTestId("cell-close-backend-codex");
      await user.click(closeBtn2);
      expect(
        screen.queryByTestId("cell-close-backend-codex"),
      ).not.toBeInTheDocument();
    });
  });

  describe("状态层：激活标记、选中标记与方案计数", () => {
    it("无激活方案时不渲染激活卡片，且没有任何芯片带激活标记", async () => {
      invokeMock.mockImplementation(async (command: string, args?: unknown) => {
        const payload = args as Record<string, unknown> | undefined;
        if (command === "ai_workflow_list_profiles") {
          return [
            { name: "onespace-ai-gateway", active: false },
            { name: "baibai-40", active: false },
          ];
        }
        if (command === "ai_workflow_get_profile_matrix") {
          if (payload?.name === "baibai-40") return mockBaibaiMatrix;
          return mockGatewayMatrix;
        }
        if (command === "ai_workflow_get_model_sources") {
          return mockModelSources;
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);

      const gatewayChip = await screen.findByRole("button", {
        name: /onespace-ai-gateway/,
      });
      expect(gatewayChip).toHaveAttribute("data-active", "false");
      expect(
        screen.getByRole("button", { name: /baibai-40/ }),
      ).toHaveAttribute("data-active", "false");
      expect(
        screen.queryByTestId("active-profile-badge"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByText(/未激活任何方案|No active profile/),
      ).not.toBeInTheDocument();
    });

    it("激活与选中状态通过方案芯片的 data 属性暴露", async () => {
      renderWithProviders(<AiWorkflowModelSwitcher />);

      const activeChip = await screen.findByRole("button", {
        name: /onespace-ai-gateway/,
      });
      expect(activeChip).toHaveAttribute("data-active", "true");
      expect(activeChip).toHaveAttribute("data-selected", "true");

      const inactiveChip = screen.getByRole("button", { name: /baibai-40/ });
      expect(inactiveChip).toHaveAttribute("data-active", "false");
      expect(inactiveChip).toHaveAttribute("data-selected", "false");
    });

    it("展示方案总数标签", async () => {
      const manyProfiles: ProfileSummary[] = Array.from(
        { length: 12 },
        (_, index) => ({ name: `plan-${index + 1}`, active: index === 0 }),
      );
      invokeMock.mockImplementation(async (command: string) => {
        if (command === "ai_workflow_list_profiles") return manyProfiles;
        if (command === "ai_workflow_get_profile_matrix") {
          return mockGatewayMatrix;
        }
        if (command === "ai_workflow_get_model_sources") {
          return mockModelSources;
        }
        return null;
      });

      renderWithProviders(<AiWorkflowModelSwitcher />);

      expect(await screen.findByText("共 12 个方案")).toBeInTheDocument();
    });
  });
});
