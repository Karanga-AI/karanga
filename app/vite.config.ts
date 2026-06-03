import { defineConfig } from "vite";

// Vite config for the Tauri webview frontend.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "dist",
    target: "esnext",
  },
});
