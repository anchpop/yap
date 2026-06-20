import { defineConfig } from "@playwright/test";

// Screenshot / E2E harness. One foreground command (`pnpm screenshots`) boots
// the Vite dev server, drives a real Chromium against the real WASM core, and
// captures PNGs into ./screenshots-out. No background processes to manage.
export default defineConfig({
  testDir: "./e2e",
  outputDir: "./e2e/.playwright-artifacts",
  fullyParallel: false,
  // Fresh contexts pay a one-time ~8s language-pack download+deserialize.
  timeout: 120_000,
  expect: { timeout: 60_000 },
  reporter: [["list"]],
  use: {
    baseURL: "http://localhost:5173",
    // Mobile-first UI — capture at a phone viewport (iPhone 15 Pro Max-ish) so
    // the shots match how the app is actually used. @2x for crisp output.
    viewport: { width: 430, height: 932 },
    deviceScaleFactor: 2,
    isMobile: true,
    hasTouch: true,
  },
  webServer: {
    command: "pnpm dev",
    url: "http://localhost:5173",
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
