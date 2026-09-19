import { useState } from "react";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { MappingPriceEditor } from "@/components/ApiGateway/MappingPriceEditor";
import type { GatewayPriceDraft } from "@/lib/apiGateway";
import { renderWithProviders } from "@/test/mocks/render";

function blankDraft(
  overrides: Partial<GatewayPriceDraft> = {},
): GatewayPriceDraft {
  return {
    id: "draft-0",
    upstream_model: "remote-a",
    input: "",
    cache_read: "",
    cache_write: "",
    output: "",
    enable_off_peak: false,
    off_peaks: [],
    ...overrides,
  };
}

type OnChangeSpy = ReturnType<typeof vi.fn>;

function lastOffPeaks(onChange: OnChangeSpy): GatewayPriceDraft["off_peaks"] {
  const calls = onChange.mock.calls as Array<[Partial<GatewayPriceDraft>]>;
  for (let index = calls.length - 1; index >= 0; index -= 1) {
    const patch = calls[index][0];
    if (patch.off_peaks) return patch.off_peaks;
  }
  throw new Error("onChange was never called with off_peaks");
}

function valueOf(testId: string): string {
  return (screen.getByTestId(testId) as HTMLInputElement).value;
}

function ControlledEditor({
  initial,
  onChange,
}: {
  initial: GatewayPriceDraft;
  onChange: OnChangeSpy;
}) {
  const [draft, setDraft] = useState(initial);
  return (
    <MappingPriceEditor
      draft={draft}
      index={0}
      onChange={(patch) => {
        onChange(patch);
        setDraft((previous) => ({ ...previous, ...patch }));
      }}
    />
  );
}

describe("MappingPriceEditor 映射行价格编辑器", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("mapping_price_editor_edits_tiers_and_reports_patches", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(<ControlledEditor initial={blankDraft()} onChange={onChange} />);

    await user.type(screen.getByTestId("api-gateway-price-0-input"), "1.5");
    await user.type(screen.getByTestId("api-gateway-price-0-cache-read"), "0.2");
    await user.type(screen.getByTestId("api-gateway-price-0-cache-write"), "0.3");
    await user.type(screen.getByTestId("api-gateway-price-0-output"), "3");

    expect(onChange).toHaveBeenCalledWith({ input: "1.5" });
    expect(onChange).toHaveBeenCalledWith({ cache_read: "0.2" });
    expect(onChange).toHaveBeenCalledWith({ cache_write: "0.3" });
    expect(onChange).toHaveBeenCalledWith({ output: "3" });

    expect(valueOf("api-gateway-price-0-input")).toBe("1.5");
    expect(valueOf("api-gateway-price-0-cache-read")).toBe("0.2");
    expect(valueOf("api-gateway-price-0-cache-write")).toBe("0.3");
    expect(valueOf("api-gateway-price-0-output")).toBe("3");
  });

  it("mapping_price_editor_toggles_off_peak_and_adds_default_window", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(<ControlledEditor initial={blankDraft()} onChange={onChange} />);

    expect(
      screen.queryByTestId("api-gateway-off-peak-0-start-0"),
    ).not.toBeInTheDocument();

    await user.click(screen.getByTestId("api-gateway-price-0-off-peak"));

    const enabledPatch = (
      onChange.mock.calls as Array<[Partial<GatewayPriceDraft>]>
    )
      .map(([patch]) => patch)
      .find((patch) => patch.enable_off_peak === true);
    expect(enabledPatch).toBeDefined();
    expect(enabledPatch!.off_peaks).toHaveLength(1);
    expect(enabledPatch!.off_peaks![0]).toMatchObject({
      start_time: "00:30",
      end_time: "08:30",
      input: "",
      cache_read: "",
      cache_write: "",
      output: "",
      days: [],
    });

    expect(screen.getByTestId("api-gateway-off-peak-0-start-0")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-off-peak-0-end-0")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-off-peak-0-input-0")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-off-peak-0-cache-read-0")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-off-peak-0-cache-write-0")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-off-peak-0-output-0")).toBeInTheDocument();

    await user.click(screen.getByTestId("api-gateway-off-peak-0-add"));
    expect(lastOffPeaks(onChange)).toHaveLength(2);

    await user.click(screen.getByTestId("api-gateway-off-peak-0-remove-1"));
    expect(lastOffPeaks(onChange)).toHaveLength(1);

    await user.click(screen.getByTestId("api-gateway-price-0-off-peak"));
    const disabledPatch = (
      onChange.mock.calls as Array<[Partial<GatewayPriceDraft>]>
    )[onChange.mock.calls.length - 1][0];
    expect(disabledPatch).toMatchObject({ enable_off_peak: false });
    expect(disabledPatch.off_peaks).toHaveLength(1);
    expect(
      screen.queryByTestId("api-gateway-off-peak-0-start-0"),
    ).not.toBeInTheDocument();
  });

  it("mapping_price_editor_weekday_chips_and_every_day", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const draft = blankDraft({
      enable_off_peak: true,
      off_peaks: [
        {
          id: "op-0",
          start_time: "00:30",
          end_time: "08:30",
          input: "",
          cache_read: "",
          cache_write: "",
          output: "",
          days: [],
        },
      ],
    });
    renderWithProviders(<ControlledEditor initial={draft} onChange={onChange} />);

    const dayChip = (day: number) =>
      screen.getByTestId(`api-gateway-off-peak-0-day-0-${day}`);
    for (let day = 0; day <= 6; day += 1) {
      expect(dayChip(day)).toHaveAttribute("aria-pressed", "false");
    }

    await user.click(dayChip(3));
    expect(dayChip(3)).toHaveAttribute("aria-pressed", "true");

    await user.click(dayChip(1));
    expect(lastOffPeaks(onChange)[0].days).toEqual([1, 3]);

    await user.click(dayChip(3));
    expect(lastOffPeaks(onChange)[0].days).toEqual([1]);

    await user.click(screen.getByTestId("api-gateway-off-peak-0-everyday-0"));
    expect(lastOffPeaks(onChange)[0].days).toEqual([]);
    expect(dayChip(1)).toHaveAttribute("aria-pressed", "false");
  });

  it("mapping_price_editor_echoes_old_data", () => {
    const onChange = vi.fn();
    const initial = blankDraft({
      input: "0",
      cache_read: "0",
      cache_write: "0",
      output: "0",
      enable_off_peak: true,
      off_peaks: [
        {
          id: "op-0",
          start_time: "23:00",
          end_time: "07:00",
          input: "0.5",
          cache_read: "0",
          cache_write: "0",
          output: "2",
          days: [1, 2],
        },
      ],
    });
    renderWithProviders(<ControlledEditor initial={initial} onChange={onChange} />);

    expect(valueOf("api-gateway-price-0-input")).toBe("0");
    expect(valueOf("api-gateway-price-0-cache-read")).toBe("0");
    expect(valueOf("api-gateway-price-0-cache-write")).toBe("0");
    expect(valueOf("api-gateway-price-0-output")).toBe("0");
    expect(valueOf("api-gateway-off-peak-0-start-0")).toBe("23:00");
    expect(valueOf("api-gateway-off-peak-0-end-0")).toBe("07:00");
    expect(valueOf("api-gateway-off-peak-0-input-0")).toBe("0.5");
    expect(valueOf("api-gateway-off-peak-0-output-0")).toBe("2");
    expect(
      screen.getByTestId("api-gateway-off-peak-0-day-0-1"),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.getByTestId("api-gateway-off-peak-0-day-0-2"),
    ).toHaveAttribute("aria-pressed", "true");
  });
});
