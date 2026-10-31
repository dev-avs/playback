import { defineConfig } from "vite";

// The installer's page, served to Tauri's WebView2 window (src-tauri).
export default defineConfig({
  clearScreen: false,
  base: "./",
  build: { outDir: "dist", target: "es2022" },
});
