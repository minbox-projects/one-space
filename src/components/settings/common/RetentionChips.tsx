import { useTranslation } from "react-i18next";

export interface RetentionChipsProps {
  value: number;
  onChange: (val: number) => void;
  options?: number[];
  min?: number;
  max?: number;
  unitLabel?: string;
  disabled?: boolean;
  ariaLabel?: string;
}

export function RetentionChips({
  value,
  onChange,
  options = [7, 30, 90, 180],
  min = 1,
  max = 365,
  unitLabel,
  disabled = false,
  ariaLabel,
}: RetentionChipsProps) {
  const { t } = useTranslation();
  const unit = unitLabel || t("days", "天");

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const raw = parseInt(e.target.value, 10);
    if (Number.isFinite(raw)) {
      onChange(Math.max(min, Math.min(max, raw)));
    } else if (e.target.value === "") {
      onChange(0);
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="flex items-center gap-1.5">
        <input
          type="number"
          min={min}
          max={max}
          step={1}
          aria-label={ariaLabel}
          disabled={disabled}
          value={value === 0 ? "" : value}
          onChange={handleInputChange}
          className="w-24 bg-background border rounded-xl px-3 py-2 text-sm font-mono text-center focus:outline-none focus:ring-2 focus:ring-primary/20"
        />
        <span className="text-sm text-muted-foreground">{unit}</span>
      </div>

      <div className="flex items-center gap-1.5 flex-wrap">
        {options.map((opt) => (
          <button
            key={opt}
            type="button"
            disabled={disabled}
            onClick={() => onChange(opt)}
            className={`px-2.5 py-1 rounded-lg text-xs font-medium transition-all ${
              value === opt
                ? "bg-primary text-primary-foreground shadow-sm"
                : "bg-muted text-muted-foreground hover:bg-muted/80 hover:text-foreground"
            }`}
          >
            {opt} {unit}
          </button>
        ))}
      </div>
    </div>
  );
}
