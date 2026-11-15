import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // WebView2 is Chromium; Bard runs nowhere else.
    target: ["es2021", "chrome100"],
    // Vite 8's own minifier. "esbuild" is deprecated there and needs esbuild
    // installed separately, which nothing declares.
    minify: true,
    sourcemap: false,
    rollupOptions: {
      input: {
        main: resolve(root, "index.html"),
        pill: resolve(root, "pill.html"),
        draw: resolve(root, "draw.html"),
        settings: resolve(root, "settings.html"),
        guide: resolve(root, "guide.html"),
      },
    },
  },
});