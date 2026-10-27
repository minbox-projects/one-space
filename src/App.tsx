import { useTranslation } from "react-i18next";
import { useTheme } from "./components/ThemeProvider";
import { useToast } from "./components/ToastProvider";
import { useTemplateAutoRefresh } from "./components/AiGateway/useTemplateAutoRefresh";
import { OnboardingWizard } from "./components/OnboardingWizard";
import { TrayBleLockBridge } from "./app/TrayBleLockBridge";
import { AppShell } from "./app/AppShell";
import { useAppController } from "./app/useAppController";

function App() {
  const { t, i18n } = useTranslation();
  const { theme, setTheme } = useTheme();
  const { pushToast, dismissToast } = useToast();

  // Provider-template auto refresh: runs regardless of which view is mounted.
  useTemplateAutoRefresh();

  const app = useAppController({
    t,
    i18n,
    theme,
    setTheme,
    pushToast,
    dismissToast,
  });

  if (app.view.onboardingStatus === "checking") {
    return (
      <>
        <TrayBleLockBridge
          enabled={app.runtimeEnabled}
          onStatusChange={app.runtime.handleTrayBleLockStatus}
        />
        <div className="h-screen w-screen bg-background text-foreground flex items-center justify-center">
          <div className="text-sm text-muted-foreground">
            {t("loading", "Loading...")}
          </div>
        </div>
      </>
    );
  }

  if (app.view.onboardingStatus === "required") {
    return (
      <>
        <TrayBleLockBridge
          enabled={app.runtimeEnabled}
          onStatusChange={app.runtime.handleTrayBleLockStatus}
        />
        <OnboardingWizard
          onComplete={(nextStorageType) => {
            app.view.setStorageType(nextStorageType);
            app.view.setOnboardingStatus("done");
          }}
        />
      </>
    );
  }

  return <AppShell {...app} />;
}

export default App;
