# Continuous Editor and Note Lifecycle Implementation Plan

> **For agentic workers:** Use subagent-driven-development for isolated task implementation and reviews; use dispatching-parallel-agents only for independent files and research.

**Goal:** Deliver continuous rendered Markdown editing with language highlighting and reliable draft/managed-note removal.

**Architecture:** Offline CodeMirror viewport decorations in a WebView2 child of the existing GPUI window; revisioned transactions and asynchronous flush barriers preserve native persistence. Detached note metadata preserves history while excluding files from active management.

**Tech Stack:** Rust, GPUI Kit 0.7.3, Wry/WebView2, CodeMirror 6, TypeScript/esbuild, browser automation.

## Global Constraints

- No bulk deletion, no termination of the user's running application, no credential logging.
- Preserve Markdown outside explicit edits, draft/library isolation and expected-hash conflict handling.
- JavaScript assets are bundled and offline; untrusted Markdown cannot execute code or make network requests.
- Follow the exact bridge message contracts in the design document.

## Task 1: Detached note lifecycle (core and sync only)

Files: core models/state/storage/engine/notes/managed and core tests; sync planner/tests; desktop worker/sync_runtime.

- [x] Add failing tests for unmanage preserving disk bytes and historical review/session/version relations, no scans after restart, same-ID explicit reimport, backup restore skipping detached files, schema 1/2 migration, and remote same-ID exclusion.
- [x] Add serde-default `Note.detached`, `Command::UnmanageNote { note_id: String }`, schema 3 migration, and filters. Keep all note rows in Snapshot for historical references.
- [x] Add a planner entry point accepting excluded IDs and update all existing callers; filter remote/local union before planning. Enforce Worker busy-sync guard and finish-time detached guard.
- [x] Run `cargo test --locked -p studyapp-core -p studyapp-sync` and focused desktop sync tests. Record red/green evidence.

## Task 2: Continuous offline editor frontend

Files: new `crates/studyapp-desktop/editor/` source/package-lock/build/tests and generated assets.

- [x] Create browser tests requiring a single contenteditable document, direct heading/emphasis rendering, hidden inactive syntax, cross-paragraph edits/undo, source mode roundtrip and highlighted common code fences. Observe absence/failure before implementation.
- [x] Implement the specified global bridge with line/UTF-16 transactions, generation/revision checks, composition-safe flush and readonly control.
- [x] Add viewport decorations and bundled language parsers; implement toolbar commands, GFM tables/tasks and safe links/code text. Disable raw HTML execution and network resources.
- [x] Test a 250 KiB+ document with 200 edits: record median/p95 latency, bounded DOM size and complete exported text. Test selection, IME, CRLF and malformed Markdown.
- [x] Bundle deterministically with pinned dependencies and collect third-party license notices. Run browser tests against the generated bundle.

## Task 3: Native host and UI lifecycle

Files: replace markdown_editor.rs; add editor protocol/host modules; app.rs, note_editor.rs, notes_view.rs, views.rs, drafts.rs, smoke.rs, Cargo manifest/lock.

- [x] Add protocol tests for UTF-16/Unicode/mixed newline patch application, out-of-order/stale messages and flush validation.
- [x] Create the WebView outside GPUI borrow stacks, embed by canvas bounds, explicitly hide/show on navigation, and drain IPC into canonical text.
- [x] Replace synchronous flush with a pending action barrier for save/copy/keep/library switch/window close. Preserve drafts on errors and block conflicting actions.
- [x] Add tokenized draft Discard/Cleared acknowledgements and two confirmation controls. Add unmanage confirmation, active-list filters and historical association preservation.
- [x] Adapt native smoke to assert actual WebView editing/IPC/versions and capture child-view evidence separately; test draft deletion and note unmanagement end to end.

## Task 4: Review, performance and delivery

Files: workflows, package script, README, release notes, verification, screenshots and version metadata.

- [x] Review the complete implementation independently; resolve correctness/security/performance findings.
- [ ] Run frontend build/tests, Rust fmt/tests/clippy, native smoke and release-package smoke. Report measured typing latency and test boundaries.
- [x] Add frontend tests/bundle reproducibility/license validation to CI and release gates, and include editor license notices.
- [x] Document continuous editing behavior, WebView2 runtime requirement, supported highlighting and lifecycle actions; bump to 0.4.0.
- [ ] Commit/push, verify CI, publish and verify a release without rewriting the existing v0.3.0 tag.
