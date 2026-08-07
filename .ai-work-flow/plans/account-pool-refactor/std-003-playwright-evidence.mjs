import { chromium } from "/Users/yuqiyu/.local/share/fnm/node-versions/v24.18.0/installation/lib/node_modules/@playwright/cli/node_modules/playwright/index.mjs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { writeFile } from "node:fs/promises";

const evidenceDir = dirname(fileURLToPath(import.meta.url));
const baseUrl = process.argv[2] ?? "http://127.0.0.1:4173";
const chromePath = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

const homepage = {
  accountCount: 3,
  availableCount: 2,
  unavailableCount: 1,
  staleCount: 0,
  today: {
    localDate: "2026-08-07",
    requestCount: 4,
    successCount: 3,
    failureCount: 1,
    usage: { inputTokens: 120, outputTokens: 80, cacheReadTokens: 20, cacheWriteTokens: 10, totalTokens: 230 },
    estimatedCostUsd: "0.42",
    costCalculable: true,
  },
  trend: Array.from({ length: 7 }, (_, index) => ({
    localDate: `2026-08-${String(index + 1).padStart(2, "0")}`,
    requestCount: index === 6 ? 4 : 0,
    successCount: index === 6 ? 3 : 0,
    failureCount: index === 6 ? 1 : 0,
    usage: { totalTokens: index === 6 ? 230 : 0 },
    estimatedCostUsd: index === 6 ? "0.42" : "0",
    costCalculable: true,
  })),
};

const models = [
  { id: "gpt-test", displayName: "GPT Test", enabled: true },
  { id: "gpt-second", displayName: "GPT Second", enabled: true },
];

const groups = [
  { id: "default", name: "默认分组", sort_order: 0, is_default: true },
  { id: "team", name: "团队分组", sort_order: 1, is_default: false },
];

const accounts = [
  {
    id: "account-default-long",
    account_type: "api_key",
    name: "默认组超长账号名称用于视觉验收",
    group_id: "default",
    sort_order: 0,
    note: "这是一段较长的备注，用来验证窄宽度下长内容仍然可读且不会挤压操作控件。",
    enabled: true,
    health_status: "healthy",
    tags: ["批量目标", "长标签用于窄宽度换行检查"],
    base_url: "https://provider.example.com/v1/very-long-compatible-endpoint/path-for-layout-check",
    auth_method: "bearer",
    upstream_protocol: "responses",
    model_mappings: [
      { account_id: "account-default-long", public_model_id: "gpt-test", upstream_model_id: "vendor-long-model-name", enabled: true },
      { account_id: "account-default-long", public_model_id: "gpt-second", upstream_model_id: "vendor-second-model-name", enabled: true },
    ],
  },
  {
    id: "account-default-disabled",
    account_type: "api_key",
    name: "默认组已禁用账号",
    group_id: "default",
    sort_order: 1,
    note: "批量工具栏的第二个目标。",
    enabled: false,
    health_status: "disabled",
    tags: ["已禁用"],
    base_url: "https://disabled.example.com/v1",
    auth_method: "api_key_header",
    upstream_protocol: "chat_completions",
    model_mappings: [
      { account_id: "account-default-disabled", public_model_id: "gpt-test", upstream_model_id: "disabled-model", enabled: false },
    ],
  },
  {
    id: "account-team",
    account_type: "api_key",
    name: "团队组账号",
    group_id: "team",
    sort_order: 0,
    note: "切换分组后才显示。",
    enabled: true,
    health_status: "healthy",
    tags: ["团队"],
    base_url: "https://team.example.com/v1",
    auth_method: "bearer",
    upstream_protocol: "responses",
    model_mappings: [
      { account_id: "account-team", public_model_id: "gpt-test", upstream_model_id: "team-model", enabled: true },
    ],
  },
];

const initialState = {
  runtime: { state: "stopped", availability: "ready", port: 17688, run_enabled: true },
  settings: { port: 17688, globalQuotaThresholdPercent: 10, logRetentionDays: 90, runEnabled: true },
  groups,
  accounts,
  models,
  keys: [],
  homepage,
  oauthReleaseBlockReason: null,
};

const clone = (value) => JSON.parse(JSON.stringify(value));

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function installTauriMock(page) {
  await page.evaluate(({ state }) => {
    window.__installEvidenceTauriMock = () => {
      if (window.__evidenceState) return;
      const current = JSON.parse(JSON.stringify(state));
      const calls = [];
      const callbacks = new Map();
      let callbackId = 0;
      let eventId = 0;
      const copy = (value) => JSON.parse(JSON.stringify(value));
      const record = (command, args) => {
        calls.push({ command, args: copy(args ?? {}) });
        window.__evidenceCalls = calls;
      };
      const bootstrap = () => copy({ ...current, accounts: current.accounts, homepage: current.homepage });
      const invoke = async (command, args = {}) => {
        record(command, args);
        if (command === "plugin:event|listen") return ++eventId;
        if (command === "plugin:event|unlisten") return undefined;
        if (command === "should_show_onboarding") return false;
        if (command === "get_storage_config") return {
          language: "zh",
          storage_type: "local",
          auto_update_enabled: false,
          skills_sync_enabled: false,
          ai_news_enabled: false,
          subagents_sync_enabled: false,
        };
        if (command === "dashboard_counts") return { ok: true, data: { launcher: 0, workspaces: 0, sessions: 0, ssh: 0, snippets: 0, bookmarks: 0, notes: 0, ai_news: 0, environments: 0, skills: 0, subagents: 0, mcp_servers: 0, storage_type: "local" }, meta: { schema_version: 1, revision: 1 } };
        if (command === "ssh_tunnels_snapshot") return { groups: [], tunnels: [], runtime: [] };
        if (command === "ai_routing_gateway_bootstrap") return bootstrap();
        if (command === "ai_routing_gateway_runtime_status") return copy(current.runtime);
        if (command === "ai_routing_gateway_logs_query") return { items: [], nextCursor: null };
        if (command === "ai_routing_gateway_prices_list") return [];
        if (command === "ai_routing_gateway_mapping_list") {
          const accountId = args.accountId;
          return copy(current.accounts.find((account) => account.id === accountId)?.model_mappings ?? []);
        }
        if (command === "ai_routing_gateway_quota_list") return [];
        if (command === "ai_routing_gateway_account_create_api_key_with_configuration") {
          const input = args.input ?? {};
          const id = "account-created";
          const created = {
            id,
            account_type: "api_key",
            name: input.name,
            group_id: input.groupId ?? "default",
            sort_order: current.accounts.filter((account) => account.group_id === (input.groupId ?? "default")).length,
            note: input.note ?? "",
            enabled: true,
            health_status: "healthy",
            tags: input.tags ?? [],
            quota_threshold_override_percent: input.quotaThresholdOverridePercent ?? null,
            base_url: input.baseUrl,
            auth_method: input.authMethod,
            upstream_protocol: input.upstreamProtocol,
            model_mappings: (input.mappings ?? []).map((mapping) => ({ account_id: id, public_model_id: mapping.publicModelId, upstream_model_id: mapping.upstreamModelId, enabled: mapping.enabled })),
          };
          current.accounts = [...current.accounts.filter((account) => account.id !== id), created];
          current.homepage = { ...current.homepage, accountCount: current.accounts.length, availableCount: current.accounts.filter((account) => account.enabled).length };
          return copy(created);
        }
        if (command === "ai_routing_gateway_accounts_delete_confirmation") return "evidence-token";
        if (command === "ai_routing_gateway_accounts_delete") return undefined;
        if (command === "ai_routing_gateway_accounts_disable") return undefined;
        if (command === "ai_routing_gateway_group_create") return undefined;
        if (command === "ai_routing_gateway_group_rename") return undefined;
        if (command === "ai_routing_gateway_group_delete") return undefined;
        return undefined;
      };
      window.__evidenceState = current;
      window.__evidenceCalls = calls;
      window.__TAURI_INTERNALS__ = {
        invoke,
        transformCallback: (callback) => {
          const id = ++callbackId;
          callbacks.set(id, callback);
          return id;
        },
        unregisterCallback: (id) => callbacks.delete(id),
        convertFileSrc: (path) => path,
        metadata: { currentWindow: { label: "main" } },
      };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => undefined };
    };
  }, { state: initialState });
  await page.waitForFunction(() => typeof window.setActiveTab === "function");
  await page.evaluate(() => {
    window.__installEvidenceTauriMock();
    window.setActiveTab("ai-routing-gateway");
  });
}

async function measure(page, label) {
  return { label, ...(await page.evaluate(() => {
    const documentElement = document.documentElement;
    const task = document.querySelector('[data-testid="ai-routing-gateway"]');
    return {
      viewport: { width: window.innerWidth, height: window.innerHeight },
      documentWidth: documentElement.scrollWidth,
      clientWidth: documentElement.clientWidth,
      horizontalOverflow: documentElement.scrollWidth > documentElement.clientWidth,
      taskWidth: task?.scrollWidth ?? null,
      taskClientWidth: task?.clientWidth ?? null,
      taskHorizontalOverflow: task ? task.scrollWidth > task.clientWidth : null,
    };
  })) };
}

async function captureTask(page, name) {
  await page.locator('[data-testid="ai-routing-gateway"]').screenshot({ path: join(evidenceDir, name) });
}

async function runViewport(browser, viewport) {
  const context = await browser.newContext({ viewport: { width: viewport.width, height: viewport.height }, locale: "zh-CN", colorScheme: "dark" });
  const page = await context.newPage();
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error.message}`));
  page.on("console", async (message) => {
    if (message.type() === "error") {
      const argumentsText = await Promise.all(message.args().map((argument) => argument.jsonValue().catch(() => undefined)));
      browserErrors.push(`console.error: ${message.text()}${argumentsText.length ? ` | args=${JSON.stringify(argumentsText)}` : ""}`);
    }
  });
  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await installTauriMock(page);
  await page.getByTestId("ai-routing-gateway").waitFor();
  await page.getByTestId("ai-gateway-tab-home").waitFor();

  const interactions = [];
  const measurements = [await measure(page, "home")];
  const gateway = page.locator('[data-testid="ai-routing-gateway"]');

  for (const tabName of ["首页", "账号池", "网关密钥", "请求日志", "设置"]) {
    await gateway.getByRole("button", { name: tabName, exact: true }).click();
    await page.getByTestId(`ai-gateway-tab-${tabName === "首页" ? "home" : tabName === "账号池" ? "accounts" : tabName === "网关密钥" ? "keys" : tabName === "请求日志" ? "logs" : "settings"}`).waitFor();
  }
  interactions.push("五个网关页签均可真实点击并展示对应面板");
  await page.getByRole("button", { name: "账号池", exact: true }).click();
  await page.getByRole("tab", { name: "默认分组", exact: true }).waitFor();
  await page.getByText(accounts[0].name, { exact: true }).waitFor();
  await page.getByText(accounts[0].base_url, { exact: true }).waitFor();
  await page.getByText("gpt-test → vendor-long-model-name", { exact: true }).waitFor();
  interactions.push("默认分组中长名称、长 API 地址、长标签和模型映射均可读");
  measurements.push(await measure(page, "accounts-list"));

  await captureTask(page, `std-003-${viewport.name}-accounts-list.png`);
  await page.getByRole("checkbox", { name: "全选当前可见账号", exact: true }).check();
  await page.getByRole("button", { name: "批量禁用", exact: true }).waitFor();
  await page.getByRole("button", { name: "批量删除", exact: true }).waitFor();
  await captureTask(page, `std-003-${viewport.name}-batch-toolbar.png`);
  interactions.push("全选当前分组可见账号后批量禁用/删除工具栏可见且控件可操作");

  let confirmationMessage = "";
  const dialogHandled = new Promise((resolve) => {
    page.once("dialog", async (browserDialog) => {
      confirmationMessage = browserDialog.message();
      await browserDialog.dismiss();
      resolve();
    });
  });
  await page.getByRole("button", { name: "批量删除", exact: true }).click();
  await dialogHandled;
  assert(confirmationMessage.includes("永久删除选中的 2 个账号"), "批量删除确认文案未绑定当前选择集合");
  assert(await page.getByTestId("ai-gateway-tab-accounts").getAttribute("data-selected-count") === "2", "取消确认后选择集合没有保留");
  interactions.push(`批量删除真实确认弹窗已触发并取消，文案为“${confirmationMessage}”，选择仍为 2 个`);

  await page.getByTitle("管理分组").click();
  const groupDialog = page.getByRole("dialog");
  await groupDialog.waitFor();
  await groupDialog.screenshot({ path: join(evidenceDir, `std-003-${viewport.name}-group-dialog.png`) });
  interactions.push("分组管理确认相关弹层真实打开，内容和关闭控件可见");
  await page.getByRole("dialog").getByRole("button", { name: "关闭", exact: true }).click();
  await page.getByRole("dialog").waitFor({ state: "detached" });

  await page.getByRole("button", { name: "添加第三方账号", exact: true }).click();
  const createDetail = page.getByTestId("account-create-detail");
  await createDetail.waitFor();
  await createDetail.getByLabel("账号名称").fill("Playwright 创建后的新账号");
  await createDetail.getByLabel("账号所属分组").selectOption("team");
  await createDetail.getByLabel("自定义标签").fill("视觉验收, 原子创建, 长标签");
  await createDetail.getByLabel("账号额度阈值（%）").fill("75");
  await createDetail.getByLabel("备注").fill("完整创建表单通过真实页面提交并由后续 bootstrap 返回。");
  await createDetail.getByLabel("API 地址").fill("https://created.example.com/v1/long-path");
  await createDetail.getByLabel("第三方 API Key").fill("PLAYWRIGHT_REDACTED_KEY");
  await createDetail.getByLabel("认证方式").selectOption("api_key_header");
  await createDetail.getByLabel("上游协议").selectOption("chat_completions");
  for (const model of models) {
    await createDetail.getByLabel(`${model.displayName} 上游模型`).fill(`${model.id}-upstream-long-name`);
    await createDetail.getByLabel(`${model.displayName} 输入价格`).fill("1");
    await createDetail.getByLabel(`${model.displayName} 输出价格`).fill("2");
    await createDetail.getByLabel(`${model.displayName} 缓存读取价格`).fill("0.1");
    await createDetail.getByLabel(`${model.displayName} 缓存写入价格`).fill("0.2");
  }
  await createDetail.getByLabel("切换 GPT Test 的映射").uncheck();
  await gateway.evaluate((element) => { element.scrollTop = 0; });
  await page.waitForTimeout(100);
  await captureTask(page, `std-003-${viewport.name}-create-top.png`);
  await gateway.evaluate((element) => { element.scrollTop = element.scrollHeight; });
  await page.waitForTimeout(100);
  await captureTask(page, `std-003-${viewport.name}-create-bottom.png`);
  measurements.push(await measure(page, "create-form"));
  interactions.push("完整创建表单的连接、认证、分组、标签、阈值、备注、两个模型映射和四类价格均可填写");

  await createDetail.getByRole("button", { name: "保存", exact: true }).click();
  await page.getByTestId("ai-gateway-tab-accounts").waitFor();
  await page.getByRole("tab", { name: "团队分组", exact: true }).click();
  await page.getByText("Playwright 创建后的新账号", { exact: true }).waitFor();
  const createCalls = await page.evaluate(() => window.__evidenceCalls.filter(({ command }) => command === "ai_routing_gateway_account_create_api_key_with_configuration"));
  const bootstrapCalls = await page.evaluate(() => window.__evidenceCalls.filter(({ command }) => command === "ai_routing_gateway_bootstrap"));
  assert(createCalls.length === 1, `原子创建调用次数为 ${createCalls.length}`);
  assert(bootstrapCalls.length >= 2, `创建后的 bootstrap 调用次数为 ${bootstrapCalls.length}`);
  interactions.push(`保存只发出 1 次原子创建调用，后续 bootstrap 返回新账号并在团队分组列表显示`);
  measurements.push(await measure(page, "created-account-list"));
  await captureTask(page, `std-003-${viewport.name}-created-account.png`);

  const calls = await page.evaluate(() => window.__evidenceCalls.map(({ command }) => command));
  await context.close();
  return { viewport, measurements, interactions, confirmationMessage, browserErrors, commandCounts: Object.fromEntries([...new Set(calls)].map((command) => [command, calls.filter((value) => value === command).length])) };
}

const browser = await chromium.launch({ headless: true, executablePath: chromePath, args: ["--disable-gpu"] });
const results = [];
try {
  for (const viewport of [{ name: "desktop", width: 1440, height: 1000 }, { name: "narrow", width: 390, height: 844 }]) {
    results.push(await runViewport(browser, viewport));
  }
} finally {
  await browser.close();
}

const report = {
  command: `node .ai-work-flow/plans/account-pool-refactor/std-003-playwright-evidence.mjs ${baseUrl}`,
  browser: { engine: "Playwright Chromium", executable: chromePath, headless: true },
  results,
  conclusion: results.every((result) => result.browserErrors.length === 0 && result.measurements.every((measurement) => !measurement.horizontalOverflow))
    ? "两组视口均无浏览器错误和文档级水平溢出，指定交互全部通过。"
    : "视觉验收存在浏览器错误或文档级水平溢出，需要继续处理。",
};
await writeFile(join(evidenceDir, "std-003-playwright-report.json"), `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));
