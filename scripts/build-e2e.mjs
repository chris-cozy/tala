import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, renameSync } from "node:fs";

const result = spawnSync(
  "pnpm",
  [
    "tauri",
    "build",
    "--debug",
    "--features",
    "e2e",
    "--config",
    "src-tauri/tauri.e2e.conf.json",
    "--no-bundle",
  ],
  { stdio: "inherit", env: { ...process.env, VITE_TALA_E2E: "1" } },
);
if (result.status !== 0) process.exit(result.status ?? 1);
// cargo test also emits a normal binary. Preserve the test binary before other checks run.
mkdirSync("artifacts/e2e/bin", { recursive: true });
copyFileSync("src-tauri/target/debug/tala", "artifacts/e2e/bin/.tala-next");
renameSync("artifacts/e2e/bin/.tala-next", "artifacts/e2e/bin/tala");
