import type { ReactElement } from "react";
import { render } from "@testing-library/react";
import { ConfirmDialogProvider } from "@/components/ConfirmDialogProvider";
import { ToastProvider } from "@/components/ToastProvider";

export function renderWithProviders(ui: ReactElement) {
  return render(
    <ToastProvider>
      <ConfirmDialogProvider>{ui}</ConfirmDialogProvider>
    </ToastProvider>,
  );
}
