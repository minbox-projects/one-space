import { ThemeProvider } from "./components/ThemeProvider";
import { QuickAiSessionBar } from "./components/QuickAiSessionBar";

/**
 * Quick-window root. Renders only the quick session bar with the minimal
 * provider stack it needs: i18n is initialized once in `main.tsx`, and the
 * ThemeProvider keeps the quick window's theme in sync with the main window.
 * It intentionally imports neither `App` nor the toolbox/registry so quick-AI
 * startup does not eagerly load the full main UI.
 */
export default function QuickAiApp() {
  return (
    <ThemeProvider defaultTheme="system" storageKey="onespace-theme">
      <QuickAiSessionBar />
    </ThemeProvider>
  );
}
