# Contributing to Tala

Keep changes focused on a dependable, local-first study workflow. For substantial features, open an issue describing the problem and proposed behavior before starting implementation.

## Before changing code

Read the [README](README.md), [architecture](docs/ARCHITECTURE.md), and [testing guide](docs/TESTING.md). Set up the tools listed in the README and install the locked dependencies. Normal development opens the normal Tala data directory; export a backup first. Use the isolated native test harness for automated workflows.

Rust owns collection mutations and validation. UI-only validation is not sufficient for imported content or saved data. Preserve scheduling/history when editing or moving notes, keep multi-step mutations transactional, and verify recovery paths when touching persistence.

Generated TypeScript bindings are committed so the frontend can build before Cargo runs. Change the Rust model/command definitions, run `pnpm test:rust`, and include the resulting bindings. Do not hand-edit or format generated bindings. Tauri's generated schemas and command permissions are rebuilt and are not committed.

## Validation and pull requests

Run `pnpm format`, `pnpm lint`, `pnpm check`, `pnpm test`, `pnpm test:rust`, and `pnpm build`. Run native workflows when changing interaction, IPC, storage, or media behavior. Extend existing tests for meaningful regressions; avoid snapshot churn unrelated to the change.

Explain what changed, why, how it was verified, and any data-compatibility risk. Include screenshots for visual changes using a disposable collection. Update documentation and third-party notices when applicable. Keep unrelated cleanup and dependency upgrades separate.

## Privacy and reports

Never commit API keys, environment overrides, databases, `.tala` backups, private images, logs, or personal card text. Test fixtures should be synthetic and generated in temporary directories. The `.gitignore` helps prevent accidents; it is not a substitute for reviewing staged files.

For bugs, include the Tala version, macOS version, architecture, steps, expected result, and actual result. Redact paths and identifiers in diagnostics. Do not post a suspected credential or private collection in a public issue; use a private repository security report if the maintainer has enabled one. There is no dedicated security contact configured in this project yet.

## Licensing

The repository does not yet establish a project license or contributor agreement. Discuss intended licensing with the maintainer before contributing substantial reusable code. Preserve third-party copyright and license notices.
