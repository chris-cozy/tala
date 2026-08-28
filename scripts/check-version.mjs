// Tauri and the UI read package.json; Cargo also requires a literal crate version.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const read = (name) =>
  readFileSync(new URL(`../${name}`, import.meta.url), "utf8");
const { version } = JSON.parse(read("package.json"));
const config = JSON.parse(read("src-tauri/tauri.conf.json"));
const cargo = read("src-tauri/Cargo.toml");
const lock = read("src-tauri/Cargo.lock");
const manifestVersion = cargo.match(
  /\[package\][\s\S]*?^version = "([^"]+)"/m,
)?.[1];
const lockVersion = lock.match(
  /\[\[package\]\]\nname = "tala"\nversion = "([^"]+)"/,
)?.[1];

assert.equal(
  config.version,
  "../package.json",
  "Tauri must use the package version",
);
assert.equal(
  manifestVersion,
  version,
  "Cargo.toml version differs from package.json",
);
assert.equal(
  lockVersion,
  version,
  "Cargo.lock version differs from package.json",
);
console.log(
  `Tala v${version}: package, native crate, and bundle versions agree.`,
);
