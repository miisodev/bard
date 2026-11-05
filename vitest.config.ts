import { defineConfig } from "vitest/config";

// The unit suite needs none of the app's Vite setup: no React transform, no dev
// server, no multi-page build. Pure modules only. No file watcher either: on
// this machine chokidar's handles kept the run open for ten seconds after it
// had finished.
export default defineConfig({
  server: { watch: null },
  test: {
    include: ["src/**/*.test.{ts,tsx}"],
    environment: "node",
    maxWorkers: 2,
    watch: false,
  },
});
