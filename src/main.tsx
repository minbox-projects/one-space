import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import './i18n'
import App from './App.tsx'
import { ThemeProvider } from './components/ThemeProvider.tsx'
import { ConfirmDialogProvider } from './components/ConfirmDialogProvider.tsx'
import { installNetworkCircuitBreaker } from './lib/networkCircuitBreaker.ts'

installNetworkCircuitBreaker()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider defaultTheme="system" storageKey="onespace-theme">
      <ConfirmDialogProvider>
        <App />
      </ConfirmDialogProvider>
    </ThemeProvider>
  </StrictMode>,
)
