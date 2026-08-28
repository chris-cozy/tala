import { execFileSync } from "node:child_process";
import {
  readFileSync,
  writeFileSync,
  readdirSync,
  existsSync,
  statSync,
} from "node:fs";
import path from "node:path";

// Rebuild notices from the exact installed/locked dependencies, without downloads.
const js = JSON.parse(
  execFileSync("pnpm", ["licenses", "list", "--prod", "--json"], {
    encoding: "utf8",
  }),
);
const cargo = JSON.parse(
  execFileSync(
    "cargo",
    [
      "metadata",
      "--locked",
      "--offline",
      "--format-version",
      "1",
      "--manifest-path",
      "src-tauri/Cargo.toml",
    ],
    { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 },
  ),
);
const nodes = new Map(cargo.resolve.nodes.map((node) => [node.id, node]));
const used = new Set();
function visit(id) {
  if (used.has(id)) return;
  used.add(id);
  for (const dependency of nodes.get(id)?.deps ?? []) {
    if (dependency.dep_kinds.some((kind) => kind.kind !== "dev"))
      visit(dependency.pkg);
  }
}
visit(cargo.resolve.root);
const packages = [
  ...Object.values(js)
    .flat()
    .map((pkg) => ({
      name: pkg.name,
      version: pkg.versions.join(", "),
      license: pkg.license,
      folder: pkg.paths[0],
      source: pkg.homepage ?? `https://www.npmjs.com/package/${pkg.name}`,
    })),
  ...cargo.packages
    .filter((pkg) => used.has(pkg.id) && pkg.name !== "tala")
    .map((pkg) => ({
      name: pkg.name,
      version: pkg.version,
      license: pkg.license,
      folder: path.dirname(pkg.manifest_path),
      source: `https://crates.io/crates/${pkg.name}/${pkg.version}`,
      explicit: pkg.license_file,
    })),
].sort(
  (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
);
const sections = [
  "Tala — third-party notices\n\nThis file lists the installed production dependency graph, including platform/build dependencies. The Tala application uses these packages under their respective licenses. Source code for each unmodified dependency is available from the source link listed below; dependency versions are pinned in the accompanying lockfiles.\n",
];
for (const pkg of packages) {
  sections.push(
    `\n${"=".repeat(72)}\n${pkg.name} ${pkg.version}\nLicense: ${pkg.license ?? "See package license"}\nSource: ${pkg.source}\n`,
  );
  const files = readdirSync(pkg.folder, { withFileTypes: true })
    .filter(
      (entry) =>
        entry.isFile() &&
        /^(licen[cs]e|copying|copyright|notice)([._-].*)?$/i.test(entry.name),
    )
    .map((entry) => path.join(pkg.folder, entry.name));
  if (pkg.explicit && existsSync(path.resolve(pkg.folder, pkg.explicit)))
    files.push(path.resolve(pkg.folder, pkg.explicit));
  for (const sub of ["fonts", "contrib", "license", "licenses"]) {
    const directory = path.join(pkg.folder, sub);
    if (existsSync(directory) && statSync(directory).isDirectory())
      for (const entry of readdirSync(directory, { withFileTypes: true })) {
        if (
          entry.isFile() &&
          /^(licen[cs]e|copying|copyright|notice|ofl)([._-].*)?$/i.test(
            entry.name,
          )
        )
          files.push(path.join(directory, entry.name));
      }
  }
  for (const file of new Set(files))
    sections.push(
      `\n--- ${path.relative(pkg.folder, file)} ---\n${readFileSync(file, "utf8")}\n`,
    );
}
writeFileSync("THIRD-PARTY-NOTICES.txt", sections.join("\n"));
console.log(`Wrote notices for ${packages.length} dependency versions.`);
