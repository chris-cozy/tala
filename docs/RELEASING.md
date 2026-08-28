# Release preparation

This document describes local checks. None of the build or validation scripts commits, pushes, tags, or publishes a GitHub release.

## Version and dependencies

`package.json` supplies the frontend and Tauri bundle version. `src-tauri/tauri.conf.json` references that file; Settings imports its version. Cargo requires a literal version in `src-tauri/Cargo.toml`, and `Cargo.lock` records it. Update the package and crate versions together, refresh the Cargo lockfile, then run `pnpm check:version`. Do not change the database schema or archive-format version merely because the application version changes.

Install with `pnpm install --frozen-lockfile`, run the checks in [Testing](TESTING.md), and regenerate dependency notices with `node scripts/notices.mjs`. The notices generator reads installed package/crate license files; it needs dependencies already downloaded. Review lockfile and notice changes rather than upgrading unrelated dependencies during release preparation.

## macOS build and inspection

The original star artwork is `public/tala-mark.png`. `src-tauri/icons/app-icon.svg` adds a rounded silhouette, a 100 px transparent inset on a 1024 px canvas, and a subtle shadow for the legacy ICNS bundle. Run `pnpm icons` after editing either source; it regenerates only the five icon assets referenced by Tauri. The interface's star image stays unchanged. Rust tests check the icon padding and transparency so a full-bleed square cannot return unnoticed.

Run `pnpm tauri build` without test features or `VITE_TALA_E2E`. The v1.0.0 Apple Silicon products are `src-tauri/target/release/bundle/macos/Tala.app` and `src-tauri/target/release/bundle/dmg/Tala_1.0.0_aarch64.dmg`.

```sh
codesign --verify --deep --strict --verbose=2 src-tauri/target/release/bundle/macos/Tala.app
plutil -p src-tauri/target/release/bundle/macos/Tala.app/Contents/Info.plist
hdiutil verify src-tauri/target/release/bundle/dmg/Tala_1.0.0_aarch64.dmg
shasum -a 256 src-tauri/target/release/bundle/dmg/Tala_1.0.0_aarch64.dmg
```

Confirm the bundle version, identifier, minimum OS, and architecture; inspect the mounted DMG and ensure it contains the expected application and notices. Verify that no collection, test fixture, environment file, WebDriver endpoint, or development-server dependency is bundled. Smoke-test the packaged application, not just the debug build.

The configured signature is ad-hoc (`-`). A successful codesign verification does not mean Developer ID signing or notarization. The maintainer must separately decide how to sign and distribute public binaries. Never commit signing certificates, credentials, or notarization secrets.

## Public repository review

- Confirm the intended project license; none is currently established. Third-party notices do not license Tala's code or artwork.
- Review every file going into Git, including screenshot contents and metadata. Do not include original prompts, private planning inputs, local paths, personal collections, diagnostics, or build products.
- Keep source, lockfiles, migrations, required icons, generated IPC contracts, tests, and public documentation. The build regenerates Tauri command permissions and schemas.
- Check for secrets and local data before staging. If a file is already tracked, untrack it while preserving the local copy; `.gitignore` alone does not remove it from Git. If it has entered history, assess rotation and history cleanup before pushing.
- Run `git diff --cached --check` and inspect `git diff --cached --stat` and the full staged patch. For an initial commit, every included file is new.

Leave the prepared changes staged for final approval. The suggested commit message is `chore(release): prepare Tala v1.0.0`; creating that commit and publishing are separate, explicitly authorized steps.
