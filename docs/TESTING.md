# Testing Tala

Run commands from the repository root on the supported macOS development environment. Never point tests at a personal collection.

## Fast checks

```sh
pnpm format:check
pnpm lint
pnpm check
pnpm test
pnpm test:rust
pnpm build
```

Prettier and rustfmt check formatting. TypeScript checks strict types and unused declarations; Clippy checks all native targets with warnings denied. Vitest covers shared rich-content rendering, mathematics, typed comparisons, and duration parsing. Rust unit/integration tests exercise scheduling, migration/transaction behavior, session limits, undo, imports, backups, integrity, malformed data, and recovery.

Rust tests also regenerate the ts-rs contracts under `src/bindings/` and `src-tauri/bindings/`. Review those changes alongside their Rust definitions. The generated files are excluded from Prettier to keep regeneration deterministic.

## Native UI workflows

```sh
pnpm test:e2e:build
pnpm test:e2e
TALA_E2E_OFFLINE=1 pnpm test:e2e
```

The build uses the optional Cargo `e2e` feature and a separate Tauri identifier. It copies the executable into `artifacts/e2e/bin/` so another Cargo invocation cannot overwrite the app during a test. Run the build and test commands sequentially.

WebdriverIO drives real WKWebView windows and production Rust collection commands. Each run creates a fresh `.test-data/native-*` directory. The test-only bridge supplies a controllable clock and purpose-specific file selections in place of OS dialog interaction. It does not replace SQLite, the scheduler, or transfer validation. Normal builds exclude that bridge and WebDriver plugins.

The nine workflows cover empty launch, deck/artwork management, rich editing and preview, study/undo, typed and reversed cards, search/bulk/trash actions, images/math, statistics/resizing, import/export/restore, and unsaved-edit/native-quit protection. Screenshots and diagnostic logs go to ignored `artifacts/e2e/`.

The optional offline pass runs the test app under macOS `sandbox-exec`, denying external connections while permitting localhost for WebDriver. It does not change system networking. This tests the application process, not unrelated external applications opened by a link. An interactive desktop session is required.

## Large-collection benchmark

```sh
cargo test --release --locked --manifest-path src-tauri/Cargo.toml --test performance -- --ignored --nocapture
```

This explicit test generates 50,000 cards and 500,000 review records in temporary storage and measures bounded search, bootstrap, study, and statistics queries. It is ignored in the normal test suite because it is more expensive. Timings depend on hardware and cache state; keep them separate from functional correctness claims.

## Manual release checks

Use a disposable collection to inspect a packaged app: create/edit/study, close and reopen, check images and equations offline, restore an export, and verify the minimum window size and interface scales. Native tests supply file-picker selections, so separately confirm real OS dialogs when changing dialog integration. Check keyboard focus, accessibility labels, and reduced motion for UI changes.

Only copy deliberately reviewed screenshots into `docs/screenshots/`. Settings screens can reveal local paths; failure captures and logs are not publication assets.
