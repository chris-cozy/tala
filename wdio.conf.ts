import { mkdirSync, mkdtempSync } from "node:fs";
import path from "node:path";

// Never open the user's collection, even if a test is interrupted.
mkdirSync(".test-data", { recursive: true });
process.env.TALA_TEST_DATA_DIR ??= mkdtempSync(
  path.resolve(".test-data/native-"),
);
mkdirSync("artifacts/e2e", { recursive: true });
const application = path.resolve(
  process.env.TALA_E2E_OFFLINE === "1"
    ? "scripts/e2e-offline.sh"
    : "artifacts/e2e/bin/tala",
);

export const config = {
  runner: "local",
  specs: ["./tests/e2e/**/*.spec.ts"],
  maxInstances: 1,
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: application,
        driverProvider: "embedded",
        captureFrontendLogs: true,
        captureBackendLogs: true,
      },
    ],
  ],
  capabilities: [{ browserName: "tauri", "tauri:options": { application } }],
  logLevel: "warn",
  outputDir: "./artifacts/e2e/logs",
  waitforTimeout: 15000,
  connectionRetryTimeout: 60000,
  connectionRetryCount: 1,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { ui: "bdd", timeout: 90000, bail: true },
  afterTest: async function (
    _test: unknown,
    _context: unknown,
    { passed }: { passed: boolean },
  ) {
    if (!passed) {
      console.log(
        "Native frontend errors:",
        await browser.execute(() => (window as any).__talaErrors),
      );
      await browser.saveScreenshot(`artifacts/e2e/failure-${Date.now()}.png`);
    }
  },
};
