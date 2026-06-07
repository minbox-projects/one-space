import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'
import path from "path"

// https://vite.dev/config/
export default defineConfig({
  // Use relative asset paths so Tauri can load built files via file:// URL.
  base: './',
  plugins: [react()],
  build: {
    // Desktop app code is intentionally shipped as a large main bundle today.
    // Keep build warnings focused on regressions rather than the known baseline.
    chunkSizeWarningLimit: 1450,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) return;
          if (id.includes("@tauri-apps")) return "tauri";
          if (id.includes("i18next")) return "i18n";
          if (id.includes("react-markdown") || id.includes("remark-gfm") || id.includes("prismjs")) {
            return "markdown";
          }
          if (id.includes("lucide-react")) return "icons";
          return "vendor";
        },
      },
    },
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: "./src/test/setup.ts",
  },
})
