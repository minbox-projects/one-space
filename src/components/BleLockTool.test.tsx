import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { BleLockTool } from "@/components/BleLockTool";
import i18n from "@/i18n";
import { renderWithProviders } from "@/test/mocks/render";
import {
  bleLockDeletePasswordMock,
  bleLockGetStatusMock,
  bleLockLockNowMock,
  bleLockRequestAccessibilityMock,
  bleLockSaveConfigMock,
  bleLockScanStartMock,
  bleLockScanStopMock,
  bleLockSelectDeviceMock,
  bleLockSetPasswordMock,
  invokeMock,
  listenMock,
  resetTauriMocks,
} from "@/test/mocks/tauri";

/**
 * Exact bilingual copy for the BLE Lock tool surface. The English strings below
 * are the frozen accessible names the implementation must render; the Chinese
 * copies are covered by `src/i18n.test.ts`. Independent DTO fixtures are
 * literals written from the frozen contract.
 */
const COPY = {
  title: "BLE Lock",
  unavailable:
    "Bluetooth lock is available only in the OneSpace macOS desktop app.",
  scan: "Scan for devices",
  stopScan: "Stop scanning",
  select: "Select",
  clearDevice: "Clear device",
  unlockRssi: "Unlock RSSI (dBm)",
  lockRssi: "Lock RSSI (dBm)",
  lockDelay: "Lock delay (seconds)",
  noSignalTimeout: "No-signal timeout (seconds)",
  wakeOnProximity: "Wake on proximity",
  wakeWithoutUnlocking: "Wake without unlocking",
  pauseWhileLocked: "Pause media while locked",
  useScreensaver: "Use screensaver to lock",
  turnOffDisplay: "Turn off display",
  lockNow: "Lock now",
  password: "Login password",
  setPassword: "Save password",
  requestAccessibility: "Request Accessibility",
} as const;

type BleLockConfigLike = {
  schema_version: number;
  device_id: string | null;
  device_name: string | null;
  mode: "active" | "passive";
  unlock_rssi: number | null;
  lock_rssi: number | null;
  lock_delay_secs: number;
  no_signal_timeout_secs: number;
  wake_on_proximity: boolean;
  wake_without_unlocking: boolean;
  pause_now_playing: boolean;
  use_screensaver_to_lock: boolean;
  sleep_display: boolean;
};

type BleLockStatusLike = {
  platform_available: boolean;
  bluetooth_state: string;
  monitoring: boolean;
  scanning: boolean;
  selected_device: { id: string; name: string; rssi: number } | null;
  presence: string;
  averaged_rssi: number | null;
  screen_locked: boolean;
  password_configured: boolean;
  accessibility_trusted: boolean;
  config: BleLockConfigLike;
  last_error: { code: string; detail?: string | null } | null;
};

function configFixture(
  overrides: Partial<BleLockConfigLike> = {},
): BleLockConfigLike {
  return {
    schema_version: 1,
    device_id: null,
    device_name: null,
    mode: "active",
    unlock_rssi: null,
    lock_rssi: null,
    lock_delay_secs: 10,
    no_signal_timeout_secs: 30,
    wake_on_proximity: false,
    wake_without_unlocking: false,
    pause_now_playing: false,
    use_screensaver_to_lock: false,
    sleep_display: false,
    ...overrides,
  };
}

function statusFixture(
  overrides: Partial<BleLockStatusLike> = {},
): BleLockStatusLike {
  return {
    platform_available: true,
    bluetooth_state: "powered_on",
    monitoring: false,
    scanning: false,
    selected_device: null,
    presence: "unknown",
    averaged_rssi: null,
    screen_locked: false,
    password_configured: false,
    accessibility_trusted: false,
    config: configFixture(),
    last_error: null,
    ...overrides,
  };
}

const originalPlatform = window.navigator.platform;
let eventHandlers: Record<string, Array<(event: { payload?: unknown }) => unknown>>;

function setTauri(present: boolean) {
  if (present) {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
  } else {
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  }
}

function setPlatform(platform: string) {
  Object.defineProperty(window.navigator, "platform", {
    value: platform,
    configurable: true,
  });
}

async function fireTauriEvent(name: string, payload?: unknown) {
  const handlers = eventHandlers[name] ?? [];
  await act(async () => {
    for (const handler of handlers) {
      const result = handler({ payload });
      if (result instanceof Promise) await result;
    }
  });
}

function bleLockCommands() {
  return invokeMock.mock.calls
    .map(([command]) => String(command))
    .filter((command) => command.startsWith("ble_lock_"));
}

function lastConfigArg(): BleLockConfigLike {
  const call = bleLockSaveConfigMock.mock.calls.at(-1);
  if (!call) throw new Error("ble_lock_save_config was not called");
  return call[0] as BleLockConfigLike;
}

describe("BleLockTool", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    resetTauriMocks();
    eventHandlers = {};
    listenMock.mockImplementation(
      async (
        eventName: string,
        handler: (event: { payload?: unknown }) => unknown,
      ) => {
        (eventHandlers[eventName] ??= []).push(handler);
        return () => {
          eventHandlers[eventName] = eventHandlers[eventName].filter(
            (registered) => registered !== handler,
          );
        };
      },
    );
  });

  afterEach(() => {
    setTauri(true);
    setPlatform(originalPlatform);
  });

  describe("platform guard", () => {
    it("renders the unavailable state in a browser preview without invoking native commands", async () => {
      setTauri(false);
      setPlatform("MacIntel");

      renderWithProviders(<BleLockTool />);

      expect(
        await screen.findByText(new RegExp(COPY.unavailable)),
      ).toBeInTheDocument();
      expect(screen.getByRole("heading", { level: 2 })).toBeInTheDocument();
      expect(bleLockCommands()).toEqual([]);
    });

    it("renders the unavailable state on a non-macOS Tauri host without invoking native commands", async () => {
      setTauri(true);
      setPlatform("Win32");

      renderWithProviders(<BleLockTool />);

      expect(
        await screen.findByText(new RegExp(COPY.unavailable)),
      ).toBeInTheDocument();
      expect(bleLockCommands()).toEqual([]);
    });
  });

  describe("macOS Tauri session", () => {
    beforeEach(() => {
      setTauri(true);
      setPlatform("MacIntel");
      bleLockGetStatusMock.mockImplementation(async () => statusFixture());
      bleLockSaveConfigMock.mockImplementation(async (config: BleLockConfigLike) =>
        statusFixture({ config }),
      );
      bleLockScanStartMock.mockImplementation(async () =>
        statusFixture({ scanning: true }),
      );
      bleLockScanStopMock.mockImplementation(async () =>
        statusFixture({ scanning: false }),
      );
      bleLockSelectDeviceMock.mockImplementation(
        async (id: string, name?: string) =>
          statusFixture({
            monitoring: true,
            selected_device: { id, name: name ?? id, rssi: -55 },
          }),
      );
      bleLockSetPasswordMock.mockImplementation(async () =>
        statusFixture({ password_configured: true }),
      );
      bleLockDeletePasswordMock.mockImplementation(async () => statusFixture());
      bleLockRequestAccessibilityMock.mockImplementation(async () =>
        statusFixture({ accessibility_trusted: true }),
      );
      bleLockLockNowMock.mockImplementation(async () =>
        statusFixture({ screen_locked: true }),
      );
    });

    it("renders the selected device and saved configuration from the shared snapshot", async () => {
      bleLockGetStatusMock.mockImplementation(async () =>
        statusFixture({
          monitoring: true,
          selected_device: { id: "uuid-a", name: "Key Fob", rssi: -55 },
          averaged_rssi: -55,
          presence: "near",
          config: configFixture({
            device_id: "uuid-a",
            device_name: "Key Fob",
            lock_delay_secs: 42,
            no_signal_timeout_secs: 25,
          }),
        }),
      );

      renderWithProviders(<BleLockTool />);

      expect(
        await screen.findByRole("heading", { level: 2, name: COPY.title }),
      ).toBeInTheDocument();
      expect((await screen.findAllByText("Key Fob")).length).toBeGreaterThan(0);
      expect((await screen.findAllByText("uuid-a")).length).toBeGreaterThan(0);
      expect((await screen.findAllByText(/-55/)).length).toBeGreaterThan(0);
      expect(screen.getByLabelText(COPY.lockDelay)).toHaveValue(42);
      expect(screen.getByLabelText(COPY.noSignalTimeout)).toHaveValue(25);

      const command = bleLockCommands();
      expect(command).toContain("ble_lock_get_status");
    });

    it("renders localized Chinese status labels and values with no raw enum leakage", async () => {
      await i18n.changeLanguage("zh");
      bleLockGetStatusMock.mockImplementation(async () =>
        statusFixture({
          platform_available: true,
          bluetooth_state: "powered_on",
          monitoring: true,
          presence: "near",
          screen_locked: true,
          selected_device: { id: "uuid-a", name: "Key Fob", rssi: -55 },
          averaged_rssi: -55,
          config: configFixture({ device_id: "uuid-a", device_name: "Key Fob" }),
        }),
      );

      renderWithProviders(<BleLockTool />);

      await screen.findByRole("heading", { level: 2, name: "蓝牙锁" });

      // Independent expected user-facing labels for the observed-state panel.
      expect(screen.getByText("蓝牙状态")).toBeInTheDocument();
      expect(screen.getByText("接近状态")).toBeInTheDocument();
      expect(screen.getByText("屏幕状态")).toBeInTheDocument();

      // Localized values for powered_on / near / locked (awaited in case the
      // shared store still holds the previous test's snapshot).
      expect(await screen.findByText("已开启")).toBeInTheDocument();
      expect(await screen.findByText("附近")).toBeInTheDocument();
      expect(await screen.findByText("已锁定")).toBeInTheDocument();

      // No raw English labels or snake_case enum values reach the user.
      expect(screen.queryByText("Presence")).not.toBeInTheDocument();
      expect(screen.queryByText("Screen")).not.toBeInTheDocument();
      expect(screen.queryByText("powered_on")).not.toBeInTheDocument();
      expect(screen.queryByText("near")).not.toBeInTheDocument();
      expect(screen.queryByText("locked")).not.toBeInTheDocument();
    });

    it("lists devices from the devices event and selects one through the wrapper", async () => {
      const user = userEvent.setup();
      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });

      // Opening the device chooser is now an explicit user action; the device
      // event only updates the already-open list.
      await user.click(await screen.findByRole("button", { name: COPY.scan }));
      await waitFor(() => expect(bleLockScanStartMock).toHaveBeenCalled());

      await fireTauriEvent("ble-lock-devices-updated", [
        { id: "uuid-a", name: "Key Fob", rssi: -55 },
        { id: "uuid-b", name: "Headset", rssi: -70 },
      ]);

      expect((await screen.findAllByText("Headset")).length).toBeGreaterThan(0);
      const selectButtons = screen.getAllByRole("button", {
        name: new RegExp(COPY.select),
      });
      expect(selectButtons.length).toBeGreaterThanOrEqual(2);

      await user.click(selectButtons[0]!);

      await waitFor(() =>
        expect(bleLockSelectDeviceMock).toHaveBeenCalledWith("uuid-a", "Key Fob"),
      );
    });

    it("saves the unlock threshold independently of the lock threshold", async () => {
      const user = userEvent.setup();
      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });

      await user.selectOptions(
        screen.getByLabelText(COPY.unlockRssi),
        "-60",
      );

      await waitFor(() => expect(bleLockSaveConfigMock).toHaveBeenCalled());
      const saved = lastConfigArg();
      expect(saved.unlock_rssi).toBe(-60);
      expect(saved.lock_rssi).toBeNull();
    });

    it("clears the threshold when Disable is selected", async () => {
      bleLockGetStatusMock.mockImplementation(async () =>
        statusFixture({ config: configFixture({ unlock_rssi: -60 }) }),
      );
      const user = userEvent.setup();
      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });

      await user.selectOptions(
        screen.getByLabelText(COPY.unlockRssi),
        "disable",
      );

      await waitFor(() => expect(bleLockSaveConfigMock).toHaveBeenCalled());
      expect(lastConfigArg().unlock_rssi).toBeNull();
    });

    it("prevents a native save for an out-of-range lock delay and saves a valid one", async () => {
      const user = userEvent.setup();
      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });

      const delay = screen.getByLabelText(COPY.lockDelay);
      await user.clear(delay);
      await user.type(delay, "0");
      await user.tab();

      await act(async () => {
        await Promise.resolve();
      });
      expect(bleLockSaveConfigMock).not.toHaveBeenCalled();

      await user.clear(delay);
      await user.type(delay, "30");
      await user.tab();

      await waitFor(() => expect(bleLockSaveConfigMock).toHaveBeenCalled());
      expect(lastConfigArg().lock_delay_secs).toBe(30);
    });

    it("stores the password, clears the input, and never persists it locally", async () => {
      const password = "correct horse battery staple";
      const user = userEvent.setup();
      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });

      await user.click(
        screen.getByRole("button", { name: COPY.password }),
      );
      const input = await screen.findByLabelText(COPY.password);
      await user.type(input, password);
      await user.click(screen.getByRole("button", { name: COPY.setPassword }));

      await waitFor(() =>
        expect(bleLockSetPasswordMock).toHaveBeenCalledWith(password),
      );
      await waitFor(() => expect(input).toHaveValue(""));

      for (let index = 0; index < window.localStorage.length; index += 1) {
        const key = window.localStorage.key(index);
        expect(key).not.toBeNull();
        expect(window.localStorage.getItem(key as string)).not.toContain(
          password,
        );
      }
      expect(JSON.stringify(bleLockSaveConfigMock.mock.calls)).not.toContain(
        password,
      );
    });

    it("requests Accessibility trust through the wrapper", async () => {
      const user = userEvent.setup();
      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });

      await user.click(
        screen.getByRole("button", { name: COPY.requestAccessibility }),
      );

      await waitFor(() =>
        expect(bleLockRequestAccessibilityMock).toHaveBeenCalled(),
      );
    });

    it("keeps screensaver locking and display-off mutually exclusive", async () => {
      const user = userEvent.setup();
      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });

      await user.click(
        screen.getByRole("switch", { name: COPY.useScreensaver }),
      );
      await waitFor(() => expect(bleLockSaveConfigMock).toHaveBeenCalled());
      expect(lastConfigArg().use_screensaver_to_lock).toBe(true);
      expect(lastConfigArg().sleep_display).toBe(false);

      const displaySwitch = screen.queryByRole("switch", {
        name: COPY.turnOffDisplay,
      });
      if (displaySwitch) {
        await user.click(displaySwitch);
        await waitFor(() => {
          for (const [config] of bleLockSaveConfigMock.mock.calls) {
            const typed = config as BleLockConfigLike;
            expect(typed.use_screensaver_to_lock && typed.sleep_display).toBe(
              false,
            );
          }
        });
      }
    });

    it("locks now through the wrapper without requiring proximity", async () => {
      const user = userEvent.setup();
      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });

      await user.click(screen.getByRole("button", { name: COPY.lockNow }));

      await waitFor(() => expect(bleLockLockNowMock).toHaveBeenCalled());
    });

    it("starts and stops scanning through the wrappers", async () => {
      const user = userEvent.setup();
      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });

      await user.click(
        await screen.findByRole("button", { name: COPY.scan }),
      );
      await waitFor(() => expect(bleLockScanStartMock).toHaveBeenCalled());

      // Results arrive while scanning; the chooser lists them for selection.
      await fireTauriEvent("ble-lock-devices-updated", [
        { id: "uuid-a", name: "Key Fob", rssi: -55 },
      ]);

      // The in-dialog Stop action is the only accessible Stop button (the
      // header affordance is aria-hidden behind the modal).
      await user.click(
        await screen.findByRole("button", { name: COPY.stopScan }),
      );
      await waitFor(() => expect(bleLockScanStopMock).toHaveBeenCalled());

      // Stopping scanning must NOT dismiss the results chooser: it stays open
      // and its discovered devices remain selectable.
      const dialog = await screen.findByRole("dialog");
      expect(
        within(dialog).getByRole("button", { name: COPY.scan }),
      ).toBeInTheDocument();
      expect(
        within(dialog).getByRole("button", { name: `${COPY.select} Key Fob` }),
      ).toBeInTheDocument();

      // An explicit close dismisses the chooser without another scan_stop.
      await user.click(within(dialog).getByRole("button", { name: "关闭" }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      expect(bleLockScanStopMock).toHaveBeenCalledTimes(1);
    });

    it("shows a stable localized error for a backend error code", async () => {
      bleLockGetStatusMock.mockImplementation(async () =>
        statusFixture({
          last_error: { code: "bluetooth_unauthorized", detail: undefined },
        }),
      );

      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });

      const expected = String(i18n.t("bleLockError_bluetooth_unauthorized"));
      expect(await screen.findByText(expected)).toBeInTheDocument();
    });

    // Regression (BLE-chooser-close): choosing a new device is an explicit user
    // action that starts discovery and opens the chooser. Dismissing the chooser
    // must cancel user discovery exactly once, and a later/background device
    // list update must only refresh the list, never reopen the chooser.
    const chooserDismissals: ReadonlyArray<
      [string, (user: ReturnType<typeof userEvent.setup>) => Promise<void>]
    > = [
      [
        "the footer close button",
        async (user) => {
          await user.click(screen.getByRole("button", { name: "关闭" }));
        },
      ],
      [
        "the built-in close button",
        async (user) => {
          await user.click(screen.getByRole("button", { name: "Close" }));
        },
      ],
      [
        "the Escape key",
        async (user) => {
          await user.keyboard("{Escape}");
        },
      ],
    ];

    it.each(chooserDismissals)(
      "cancels user discovery once and ignores later device updates when the chooser is dismissed via %s",
      async (_dismissLabel, dismiss) => {
        bleLockGetStatusMock.mockImplementation(async () =>
          statusFixture({
            monitoring: true,
            selected_device: { id: "uuid-a", name: "Key Fob", rssi: -55 },
            averaged_rssi: -55,
            config: configFixture({
              device_id: "uuid-a",
              device_name: "Key Fob",
            }),
          }),
        );
        const user = userEvent.setup();
        renderWithProviders(<BleLockTool />);
        await screen.findByRole("heading", { level: 2, name: COPY.title });

        // The chooser starts closed and no discovery is running, so the header
        // scan affordance is present before the explicit user action.
        await screen.findByRole("button", { name: COPY.scan });
        expect(screen.queryByRole("dialog")).toBeNull();

        // Explicit user action: "change device" starts discovery and opens the
        // chooser for the already-selected device.
        await user.click(
          await screen.findByRole("button", { name: "更换设备" }),
        );
        await waitFor(() =>
          expect(bleLockScanStartMock).toHaveBeenCalledTimes(1),
        );
        expect(await screen.findByRole("dialog")).toBeInTheDocument();

        // Dismissing the chooser cancels user discovery exactly once and the
        // dialog disappears.
        await dismiss(user);
        await waitFor(() =>
          expect(bleLockScanStopMock).toHaveBeenCalledTimes(1),
        );
        await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

        // A later/background device update only refreshes the list; it must not
        // reopen the chooser or restart discovery.
        await fireTauriEvent("ble-lock-devices-updated", [
          { id: "uuid-c", name: "Watch", rssi: -60 },
        ]);
        expect(screen.queryByRole("dialog")).toBeNull();
        expect(bleLockScanStopMock).toHaveBeenCalledTimes(1);
        expect(bleLockScanStartMock).toHaveBeenCalledTimes(1);
      },
    );

    it("ignores a late scan-start settlement after the chooser closes before it resolves", async () => {
      let resolveStart: (value: BleLockStatusLike) => void = () => {};
      const pendingStart = new Promise<BleLockStatusLike>((resolve) => {
        resolveStart = resolve;
      });
      bleLockScanStartMock.mockImplementationOnce(() => pendingStart);

      const user = userEvent.setup();
      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });
      await screen.findByRole("button", { name: COPY.scan });

      await user.click(await screen.findByRole("button", { name: COPY.scan }));
      await waitFor(() => expect(bleLockScanStartMock).toHaveBeenCalledTimes(1));
      expect(await screen.findByRole("dialog")).toBeInTheDocument();

      // Close while the start request is still pending.
      await user.click(screen.getByRole("button", { name: "关闭" }));
      await waitFor(() => expect(bleLockScanStopMock).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

      // The late settlement must not reopen the chooser or re-enter user
      // discovery, and a subsequent device update must not either.
      await act(async () => {
        resolveStart(statusFixture({ monitoring: true, scanning: true }));
        await pendingStart;
      });
      await fireTauriEvent("ble-lock-devices-updated", [
        { id: "uuid-c", name: "Watch", rssi: -60 },
      ]);
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(screen.queryByRole("button", { name: COPY.stopScan })).toBeNull();
      expect(screen.getByRole("button", { name: COPY.scan })).toBeInTheDocument();
      expect(bleLockScanStopMock).toHaveBeenCalledTimes(1);
      expect(bleLockScanStartMock).toHaveBeenCalledTimes(1);
    });

    // Regression (passive background survives user discovery): the native
    // monitor keeps `status.scanning` true in passive mode, so the chooser must
    // key off user intent, not that flag. An explicit change-device action must
    // still start user discovery, and selecting a device must stop it exactly
    // once while leaving the passive monitor's scanning status intact.
    it("keeps passive monitoring intact across user discovery from change device to selection", async () => {
      const passiveConfig = configFixture({
        device_id: "uuid-a",
        device_name: "Key Fob",
        mode: "passive",
      });
      const passiveSelected = { id: "uuid-a", name: "Key Fob", rssi: -55 };

      bleLockGetStatusMock.mockImplementation(async () =>
        statusFixture({
          monitoring: true,
          scanning: true,
          selected_device: passiveSelected,
          averaged_rssi: -55,
          presence: "near",
          config: passiveConfig,
        }),
      );
      bleLockScanStartMock.mockImplementation(async () =>
        statusFixture({
          monitoring: true,
          scanning: true,
          selected_device: passiveSelected,
          averaged_rssi: -55,
          presence: "near",
          config: passiveConfig,
        }),
      );
      bleLockSelectDeviceMock.mockImplementation(
        async (id: string, name?: string) =>
          statusFixture({
            monitoring: true,
            scanning: true,
            selected_device: { id, name: name ?? id, rssi: -70 },
            averaged_rssi: -70,
            presence: "near",
            config: configFixture({
              device_id: id,
              device_name: name ?? id,
              mode: "passive",
            }),
          }),
      );
      // User scan_stop returns while the passive monitor keeps scanning.
      bleLockScanStopMock.mockImplementation(async () =>
        statusFixture({
          monitoring: true,
          scanning: true,
          selected_device: { id: "uuid-b", name: "Headset", rssi: -70 },
          averaged_rssi: -70,
          presence: "near",
          config: configFixture({
            device_id: "uuid-b",
            device_name: "Headset",
            mode: "passive",
          }),
        }),
      );

      const user = userEvent.setup();
      renderWithProviders(<BleLockTool />);
      await screen.findByRole("heading", { level: 2, name: COPY.title });

      // Passive scanning is already active, but no user discovery is running:
      // the header offers the explicit Scan affordance, not a user-scan Stop.
      await screen.findByRole("button", { name: COPY.scan });
      expect(screen.queryByRole("button", { name: COPY.stopScan })).toBeNull();
      expect(screen.queryByRole("dialog")).toBeNull();

      // A background device update must not auto-open the chooser.
      await fireTauriEvent("ble-lock-devices-updated", [
        { id: "uuid-c", name: "Watch", rssi: -60 },
      ]);
      expect(screen.queryByRole("dialog")).toBeNull();

      // Explicit change-device starts user discovery even though the passive
      // monitor already reports scanning.
      await user.click(await screen.findByRole("button", { name: "更换设备" }));
      await waitFor(() => expect(bleLockScanStartMock).toHaveBeenCalledTimes(1));
      expect(await screen.findByRole("dialog")).toBeInTheDocument();

      // Device results arrive while the chooser is open.
      await fireTauriEvent("ble-lock-devices-updated", [
        { id: "uuid-b", name: "Headset", rssi: -70 },
      ]);
      await user.click(
        await screen.findByRole("button", { name: `${COPY.select} Headset` }),
      );

      // Selection selects the new device, ends user discovery exactly once, and
      // closes the chooser.
      await waitFor(() =>
        expect(bleLockSelectDeviceMock).toHaveBeenCalledWith(
          "uuid-b",
          "Headset",
        ),
      );
      await waitFor(() => expect(bleLockScanStopMock).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

      // The stop result still reports passive scanning, yet the UI is idle: the
      // header shows the explicit Scan action, not a lingering user-scan Stop.
      expect(
        await screen.findByRole("button", { name: COPY.scan }),
      ).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: COPY.stopScan })).toBeNull();

      // A later background update cannot reopen the chooser.
      await fireTauriEvent("ble-lock-devices-updated", [
        { id: "uuid-d", name: "Tag", rssi: -65 },
      ]);
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(bleLockScanStopMock).toHaveBeenCalledTimes(1);

      // An explicit reopen still starts a fresh user discovery.
      await user.click(await screen.findByRole("button", { name: "更换设备" }));
      await waitFor(() => expect(bleLockScanStartMock).toHaveBeenCalledTimes(2));
      expect(await screen.findByRole("dialog")).toBeInTheDocument();
    });
  });
});
