# Storage, folders, and history usability extension

The user added these requirements while 0.4.0 was being verified. Preserve the continuous editor and lifecycle work; deliver the additions in the same unreleased version.

## Product behavior

- Settings can choose an existing independent library, or migrate the current library to a new empty directory and remember it for the next normal launch. Migration copies data, validates the target, then publishes the startup location atomically. Original data is retained. Explicit `--data-dir` sessions remain isolated from global startup preferences.
- Flush the active editor and wait for draft durability before migration. Reject migration during sync. Failures retain the original library and all source files.
- Display Windows verbatim paths as ordinary drive/UNC paths without changing canonical paths used for file IO or equality.
- A storage panel shows database, app-managed note copies, retained data, and auxiliary usage. Each owned managed copy has a separate confirmation. Delete one exact file only, after stopping management. Never delete a tracked external source, junction target, directory, backup, or unknown orphan. Keep history and learning records. Failure leaves the copy available for retry.
- Notebook directories are logical folders, independent of physical source-file locations. Support nesting, create, rename, moving folders without cycles, moving notes, and removing empty logical folders. All notes, root notes and individual folders can be selected. Moving a tracked note between folders never moves its source.
- History timelines/diffs have their own bounded scroll surface; reading and editing modes have clear ownership. Opening an old snapshot can create an editable copy of that snapshot. Editing the current managed note resets history mode and activates the single continuous editor.

## Implementation ownership and validation

Use parallel agents for independent modules: core folder metadata/commands, desktop storage backend/worker, and folder UI. Root owns app/settings integration, display paths and history scroll fixes. Do not edit another owner's files without coordination.

Keep schema 3 for this still-unreleased version; add serde defaults so existing schema 1/2/3 fixtures remain readable. Old released applications reject schema 3.

Test folder identity/persistence/cycles/backup validation; storage migration restart location, failure preservation, external-source deletion rejection, symlink/junction rejection and usage counts; UI note-folder moves, history scrolling, edit-after-history, disk cleanup and migration flush. Run full existing Rust/browser/native/release gates before publication. No bulk filesystem deletion is allowed.

## Verified implementation

- Core folders: 9 new regression cases, 70 core tests total.
- Storage backend and worker: 21 tests, including Windows junction rejection, failed deletion retry, migration publication failure, restart preferences and explicit-directory isolation.
- Full workspace: 165 tests; format and all-target/all-feature Clippy pass. The additional sync regression covers delayed request bytes in the Windows WebDAV fixture.
- Native evidence: `.scratch/library-v040-67c714e7892448379d7ca03ebdccd84d`, including actual wheel movement to the last history row at 1000x700, 200-character title, native DOM input after history and saved bytes, draft-folder deletion fallback, single-copy deletion, source byte preservation and migration of a durable draft. Reviews corrected storage-busy save deadlock and orphaned draft folders before this run.
- Space totals are logical file lengths inside the selected library; hard links can count more than once. Install files and shared WebView cache are outside this statistic. Original libraries and failed migration destinations are retained.
