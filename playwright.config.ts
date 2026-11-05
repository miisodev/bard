import { defineConfig } from "@playwright/test";

// One browser, one worker: desktop development shares memory with the apps
// the person is working in. Use an installed Chrome instead of downloading it.
export default defineConfig({
  testDir: "./tests/browser",
  workers: 1,
  fullyParallel: false,
  retries: 0,
  reporter: "list",
  use: {
    baseURL: "http://127.0.0.1:1420",
    channel: "chrome",
    headless: true,
    viewport: { width: 1280, height: 320 },
    reducedMotion: "reduce",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "bun run dev --host 127.0.0.1",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: !process.env.CI,
  },
});
