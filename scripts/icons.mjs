// Apply the macOS frame without redrawing the brand mark, then let Tauri build each format.
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const icons = path.join(root, "src-tauri/icons");
const artwork = readFileSync(path.join(root, "public/tala-mark.png"));
const template = readFileSync(path.join(icons, "app-icon.svg"), "utf8");
const reference = 'href="../../public/tala-mark.png"';
if (template.split(reference).length !== 2) {
  throw new Error(
    "The icon template must reference the original Tala mark exactly once.",
  );
}

mkdirSync(path.join(root, "artifacts"), { recursive: true });
const staging = mkdtempSync(path.join(root, "artifacts/icons-"));
try {
  // Embed the original bytes for resvg; do not depend on relative external-image loading.
  const source = path.join(staging, "app-icon.svg");
  writeFileSync(
    source,
    template.replace(
      reference,
      `href="data:image/png;base64,${artwork.toString("base64")}"`,
    ),
  );
  const output = path.join(staging, "generated");
  const result = spawnSync(
    "pnpm",
    ["exec", "tauri", "icon", source, "--output", output],
    {
      cwd: root,
      stdio: "inherit",
    },
  );
  if (result.error) throw result.error;
  if (result.status !== 0)
    throw new Error(`Tauri icon generation failed (${result.status}).`);

  // Only these assets are referenced by the desktop bundle. Other platform variants stay temporary.
  for (const name of [
    "32x32.png",
    "128x128.png",
    "128x128@2x.png",
    "icon.icns",
    "icon.ico",
  ]) {
    copyFileSync(path.join(output, name), path.join(icons, name));
  }
} finally {
  rmSync(staging, { recursive: true, force: true });
}
