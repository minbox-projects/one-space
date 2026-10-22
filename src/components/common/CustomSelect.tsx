import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";

export interface CustomSelectOption<T extends string = string> {
  value: T;
  label: string;
}

export type CustomSelectSize = "default" | "sm";

export interface CustomSelectProps<T extends string = string> {
  value: T;
  onChange: (value: T) => void;
  options: CustomSelectOption<T>[];
  size?: CustomSelectSize;
  disabled?: boolean;
  className?: string;
  triggerClassName?: string;
  menuClassName?: string;
  placeholder?: string;
  ariaLabel?: string;
  selectTestId?: string;
  triggerTestId?: string;
  menuTestId?: string;
  optionTestIdPrefix?: string;
}

export function CustomSelect<T extends string = string>({
  value,
  onChange,
  options,
  size = "default",
  disabled = false,
  className = "",
  triggerClassName = "",
  menuClassName = "",
  placeholder,
  ariaLabel,
  selectTestId,
  triggerTestId,
  menuTestId,
  optionTestIdPrefix,
}: CustomSelectProps<T>) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handleClickOutside(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  const selectedOption = options.find((opt) => opt.value === value);
  const displayLabel = selectedOption?.label ?? placeholder ?? value;

  const isSmall = size === "sm";
  const heightClass = isSmall ? "h-8 px-2.5 text-xs" : "h-[38px] px-3 text-sm";
  const iconSizeClass = isSmall ? "h-3.5 w-3.5" : "h-4 w-4";

  return (
    <div className={`relative ${className}`} ref={containerRef}>
      {/* Hidden native select for accessibility and automated testing compatibility */}
      <select
        data-testid={selectTestId}
        aria-label={ariaLabel}
        value={value}
        onChange={(e) => onChange(e.target.value as T)}
        disabled={disabled}
        className="sr-only"
        tabIndex={-1}
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>

      {/* Styled custom trigger button aligned with ProviderTemplateIconPicker */}
      <button
        type="button"
        data-testid={triggerTestId}
        aria-haspopup="listbox"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen((prev) => !prev)}
        className={`flex w-full items-center justify-between gap-2 rounded-lg border border-border bg-background font-medium text-foreground transition-all hover:border-primary/60 focus:border-primary focus:ring-2 focus:ring-primary/50 disabled:cursor-not-allowed disabled:opacity-50 ${heightClass} ${triggerClassName}`}
      >
        <span className="truncate">{displayLabel}</span>
        <ChevronDown
          className={`${iconSizeClass} shrink-0 text-muted-foreground transition-transform duration-200 ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>

      {/* Dropdown listbox menu */}
      {open && (
        <div
          role="listbox"
          data-testid={menuTestId}
          className={`absolute left-0 top-full z-50 mt-1.5 max-h-60 w-full min-w-[140px] overflow-y-auto rounded-xl border border-border/80 bg-popover p-1.5 text-popover-foreground shadow-lg animate-in fade-in-0 zoom-in-95 ${menuClassName}`}
        >
          {options.map((opt) => {
            const isSelected = opt.value === value;
            return (
              <button
                key={opt.value}
                type="button"
                role="option"
                aria-selected={isSelected}
                data-testid={
                  optionTestIdPrefix
                    ? `${optionTestIdPrefix}-${opt.value}`
                    : undefined
                }
                onClick={() => {
                  onChange(opt.value);
                  setOpen(false);
                }}
                className={`flex w-full items-center justify-between rounded-lg px-2.5 py-2 text-left text-xs transition hover:bg-muted ${
                  isSelected
                    ? "bg-muted/70 font-semibold text-primary"
                    : "text-foreground"
                }`}
              >
                <span className="truncate">{opt.label}</span>
                {isSelected && (
                  <Check className={`${iconSizeClass} shrink-0 text-primary`} />
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
