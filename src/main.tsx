import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import './i18n'
import { installNetworkCircuitBreaker } from './lib/networkCircuitBreaker.ts'

installNetworkCircuitBreaker()

export type EntryKind = 'quick-ai' | 'main'

/**
 * Selects the application entry from the URL query before any app module is
 * loaded, so a quick-AI window never pulls in the full main UI or toolbox.
 */
export function resolveEntryKind(search: string): EntryKind {
  return new URLSearchParams(search).get('view') === 'quick-ai' ? 'quick-ai' : 'main'
}

async function renderEntry(rootElement: HTMLElement): Promise<void> {
  const root = createRoot(rootElement)

  if (resolveEntryKind(window.location.search) === 'quick-ai') {
    const { default: QuickAiApp } = await import('./QuickAiApp.tsx')
    root.render(
      <StrictMode>
        <QuickAiApp />
      </StrictMode>,
    )
    return
  }

  const [
    { default: App },
    { ThemeProvider },
    { ToastProvider },
    { ConfirmDialogProvider },
  ] = await Promise.all([
    import('./App.tsx'),
    import('./components/ThemeProvider.tsx'),
    import('./components/ToastProvider.tsx'),
    import('./components/ConfirmDialogProvider.tsx'),
  ])

  root.render(
    <StrictMode>
      <ThemeProvider defaultTheme="system" storageKey="onespace-theme">
        <ToastProvider>
          <ConfirmDialogProvider>
            <App />
          </ConfirmDialogProvider>
        </ToastProvider>
      </ThemeProvider>
    </StrictMode>,
  )
}

const rootElement = document.getElementById('root')
if (rootElement) {
  void renderEntry(rootElement).catch((error) => {
    console.error('Failed to render OneSpace', error)
  })
}
