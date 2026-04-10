import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from "path"

// https://vite.dev/config/
export default defineConfig({
  // Use relative asset paths so Tauri can load built files via file:// URL.
  base: './',
  plugins: [react()],
  build: {
    // The desktop bundle is currently a single large entry; use a threshold
    // that matches the existing app size so routine builds stay signal-heavy.
    chunkSizeWarningLimit: 1800,
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
})
