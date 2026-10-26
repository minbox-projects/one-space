import { useState } from "react";
import { Eye, EyeOff } from "lucide-react";
import { useTranslation } from "react-i18next";

interface PasswordFieldProps {
  value: string;
  invalid: boolean;
  disabled?: boolean;
  onChange: (value: string) => void;
}

export function PasswordField({
  value,
  invalid,
  disabled = false,
  onChange,
}: PasswordFieldProps) {
  const { t } = useTranslation();
  const [visible, setVisible] = useState(false);

  return (
    <div className="space-y-1">
      <label htmlFor="file-sharing-password" className="text-sm font-medium">
        {t("fileSharingPassword", "Access password")}
      </label>
      <div className="relative">
        <input
          id="file-sharing-password"
          data-testid="file-sharing-password"
          type={visible ? "text" : "password"}
          value={value}
          disabled={disabled}
          autoComplete="new-password"
          placeholder={t("fileSharingPasswordPlaceholder", "Leave empty for no password")}
          aria-invalid={invalid}
          aria-describedby="file-sharing-password-hint"
          onChange={(event) => onChange(event.target.value)}
          className="h-10 w-full rounded-md border bg-background px-3 pr-10 text-sm focus:border-primary focus:outline-none focus:ring-2 focus:ring-primary/50 disabled:cursor-not-allowed disabled:opacity-50"
        />
        <button
          type="button"
          onClick={() => setVisible((current) => !current)}
          disabled={disabled}
          aria-label={
            visible
              ? t("fileSharingHidePassword", "Hide password")
              : t("fileSharingShowPassword", "Show password")
          }
          className="absolute right-1 top-1 inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted disabled:opacity-50"
        >
          {visible ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
        </button>
      </div>
      <p id="file-sharing-password-hint" className="text-xs text-muted-foreground">
        {t(
          "fileSharingPasswordHint",
          "1-128 characters with at least one non-space character. The password is kept in memory only.",
        )}
      </p>
      {invalid ? (
        <p className="text-xs text-destructive">
          {t(
            "fileSharingPasswordInvalid",
            "Password must be 1-128 characters and contain at least one non-space character.",
          )}
        </p>
      ) : null}
    </div>
  );
}
