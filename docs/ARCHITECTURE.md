# Architecture and behavior

## Ownership and trust boundaries

React presents the collection; Rust owns the collection. `src-tauri/src/api.rs` defines a tagged command union, and ts-rs generates the matching TypeScript bindings. A single native store serializes database mutations. Blocking work runs outside the UI thread. The frontend has no unrestricted filesystem, shell, SQL, or network capability.

SQLite uses foreign keys, WAL, full synchronous writes, indexed queries, and transactional mutations. Versioned migration failures roll back. An unsupported future schema is refused instead of reset. Notes own content, behavior, tags, and media references. Each Basic note generates one independently scheduled card. Soft deletion retains note/card/history relationships until explicit permanent deletion.

Rich content is a bounded, validated TipTap JSON tree, not arbitrary HTML. Only supported nodes and marks survive validation. Links allow HTTP, HTTPS, and mailto and open externally after user activation. Images refer to SHA-256 filenames in Tala's media directory. Images must decode as PNG/JPEG/WebP/GIF, be no larger than 20 MB, and contain no more than 40 million pixels. The renderer supplies local asset URLs at display time; URLs are never saved into card content. KaTeX and its fonts are bundled.

The release CSP permits bundled scripts, local assets, and Tauri IPC. The asset protocol is scoped at runtime to the collection's media directory. OS file dialogs issue opaque purpose-specific grants; imports and exports accept these grants rather than arbitrary frontend paths. The optional `e2e` feature is the only source of test data-directory overrides, synthetic file selections, clock controls, and WebDriver access.

## Scheduling semantics

- FSRS 6 is supplied by the pinned `fsrs` 6.6.1 crate using its default parameters. Every grade updates stability and difficulty. The same native function calculates the displayed options and committed schedule.
- Learning and relearning steps are actual elapsed seconds. Again resets to the first step; Hard repeats an intermediate delay; Good advances; Easy graduates. Once learning finishes, FSRS determines the calendar-day interval, capped by the deck's maximum interval.
- Review timestamps are UTC. The study day is the operating system's local IANA calendar day, with midnight handling for daylight-saving transitions. A review stores its original study-day string. Historical day totals are not relabeled when the user later travels.
- Daily limits count distinct new/review cards admitted in each deck on the recorded day. Learning repetitions do not consume those limits. Undo reverses the associated count. A moved card retains its memory and history; past reviews retain their original deck attribution.
- An active session has a fixed list of card IDs. Its progress counts cards completed for that day or skipped after management changes, not individual repetitions or deck mastery. Future repetitions due later today show a wait state and resume automatically. Newly added cards enter the next session.
- Due learning/relearning cards are placed first. Each deck then applies its selected creation/random order, review due/random order, and before/after/mixed placement. Active sessions recheck eligibility and changed limits.
- Bury lasts until the next local midnight; suspension lasts until explicitly removed. These are independent flags over the underlying learning state. Repeated review lapses mark leeches and can suspend them according to the deck setting.
- Typed answer differences are informational. The user always chooses Again, Hard, Good, or Easy.
- Answer timing uses a monotonic clock, pauses when the window loses focus, and does not determine the grade. Native storage caps an individual recorded duration at 24 hours.
- Review commits are atomic and have unique operation IDs to prevent duplicate submission. A revision check rejects grading a stale card. A clock earlier than the last review prevents grading instead of corrupting memory state.

## Editing, history, and undo

Content/behavior changes and deck moves preserve scheduling and review history. Tags are normalized and belong to notes. Text editing has its own editor undo/redo. The native collection retains the most recent eligible review or bulk undo snapshot. A subsequent incompatible mutation can replace or clear it. Undo records a review reversal; it does not silently erase the historical event.

Reset to New clears current memory/learning progress while retaining cumulative review/lapse counters and historical events. Set due changes only the due date. Reschedule applies current retention and maximum interval to Review cards using their existing stability and last review. These operations do not fabricate reviews. Deck deletion either moves cards to another deck or moves notes/cards to Recently Deleted. Permanent note deletion also removes its associated cards/history.

Navigation, native window close, and native quit protect unsaved card edits. Saved notes and grades commit immediately; there is no global Save Collection step. Force termination or power failure cannot preserve an unsaved editor buffer.

## Search, transfers, and statistics

FTS5 indexes normalized Front/Back text. Browse uses native filtering, bounded pages, and virtual rows; it does not load the complete collection into the browser. Search is a simple token/prefix search rather than an Anki query language.

CSV/TSV import supports column mapping, optional headers, a selected target deck and behavior, semicolon-separated tags, and three duplicate policies. A duplicate is normalized Front + selected behavior + target deck. Update preserves the matching card's schedule/history; ambiguous multiple matches are rejected. Preview covers all-row validation but displays at most 50 rows. Commit rechecks a digest of the file and configuration and applies all changes transactionally. Plain-text export contains Front, Back, Behavior, Deck, and Tags. Native exports preserve the database, preferences, media, artwork, schedules, and history together.

Statistics exclude undone reviews. Recall is Hard/Good/Easy divided by all completed reviews; it is not an automatic measure of typed-answer correctness. Cards learned counts first graduation into Review. Streaks count consecutive recorded study days and remain active through the day after the last review. Chart ranges affect historical metrics; state/deck panels describe the current collection. The forecast shows stored due dates over 14 days, including buried cards when they return, before later grades or daily caps. Retrievability is an FSRS model estimate, presented separately from observed recall.

## Backups, recovery, and diagnostics

A `.tala` file is a versioned ZIP with a manifest, SQLite snapshot, and media. Every file has a SHA-256 digest. Restore rejects unsupported versions, duplicate/unsafe paths, symlinks, oversized entries, inconsistent schemas, invalid content/schedules, and missing or invalid media before replacement.

Automatic backups snapshot SQLite and hard-link immutable media under a short collection lock. Compression and hashing then run without the lock. A job is tied to the collection instance, so completion after a restore cannot mark the replacement collection as backed up. Only automatic backups participate in automatic retention. Abandoned owned snapshot directories are cleaned at startup.

Normal restore first writes a safety backup, validates a staged replacement, and uses an atomic rename plus recovery journal. Startup completes recovery from an interrupted swap. Unreadable-database recovery preserves raw database/WAL/media files in a separate directory before replacing them with a validated archive. It never attempts to repair source note content by guessing.

Integrity checks cover SQLite, foreign keys, canonical content, media references/checksums/decoding, scheduling state, review history, metadata, and derived search fields. Safe index repair first backs up, then rebuilds only derived text, FTS, and note-media references. Cleanup refuses unhealthy collections and protects media referenced by Recently Deleted notes. Logs and exported diagnostics omit note content; local log rotation keeps the current and previous log.

## Implementation map

Paths are relative to the repository root.

| Area                             | Source                                                                                                                                                                                 |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Typed IPC and native lifecycle   | [api.rs](../src-tauri/src/api.rs), [lib.rs](../src-tauri/src/lib.rs)                                                                                                                   |
| Schema, CRUD, bulk changes, undo | [migration](../src-tauri/migrations/001_initial.sql), [store.rs](../src-tauri/src/store.rs)                                                                                            |
| FSRS and study-day rules         | [scheduler.rs](../src-tauri/src/scheduler.rs), [clock.rs](../src-tauri/src/clock.rs), [study.rs](../src-tauri/src/study.rs)                                                            |
| Transfers and recovery           | [import_export.rs](../src-tauri/src/import_export.rs), [archive.rs](../src-tauri/src/archive.rs), [integrity.rs](../src-tauri/src/integrity.rs), [files.rs](../src-tauri/src/files.rs) |
| Shared editor and renderer       | [RichContent.tsx](../src/components/RichContent.tsx)                                                                                                                                   |
| Shell and navigation             | [App.tsx](../src/App.tsx)                                                                                                                                                              |
| Primary screens                  | [pages](../src/pages/)                                                                                                                                                                 |
| Visual tokens and layouts        | [styles.css](../src/styles.css), [styles](../src/styles/)                                                                                                                              |

## Generated files and versions

Rust model and command definitions are the source for the committed ts-rs contracts in `src/bindings/` and `src-tauri/bindings/`. Normal Rust tests regenerate them; the frontend needs them before its first build. Tauri command permissions and schemas are generated by the native build and ignored.

The application version comes from `package.json` for the interface and Tauri bundle. Cargo's required crate version is checked against it by `pnpm check:version`. Database schema and archive format versions have separate compatibility meanings and do not change automatically with the application version.
