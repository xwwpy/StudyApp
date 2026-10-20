# WW-StudyApp note interactions implementation plan

> **For agentic workers:** Use subagent-driven-development for independent modules; root integrates and verifies. Steps use checkbox tracking.

**Goal:** Make notes visibly readable/editable, add native note organization actions, contain storage scrolling and rename the visible product.
**Architecture:** Preserve the existing GPUI shell, worker commands, logical folders and offline WebView document protocol. Add native drag/menu events and a durable display preference. Fix native editor presentation from reproduction evidence.
**Tech Stack:** Rust 1.98, GPUI Kit 0.7.3, Wry/WebView2, CodeMirror 6, PowerShell.

## Global constraints

- No bulk file/folder deletion; only explicitly named single-file deletion when needed.
- Do not terminate the user's running application or use their library for tests.
- Existing data/config identifiers remain compatible; visible name becomes WW-StudyApp.
- Path display defaults to false for both new and old saved settings.
- Moves alter folder metadata only, exclude current destination, and reject stale-library payloads.
- Native testing uses a distinct EXE and fresh retained evidence directory.

## Task 1: Folder and note interactions

Owner: folder subagent. Files: `folders_ui.rs`, new `note_actions.rs`; root integrates `notes_view.rs`.

- [x] Inspect GPUI drag, context menu and InputEvent::PressEnter implementations in the pinned dependency.
- [x] Add destination tests: Unclassified, nested full paths, current folder exclusion, folder self/descendant exclusion.
- [x] Implement a typed drag payload carrying note ID/library identity, drop targets and visible feedback.
- [x] Implement context-menu event API for MoveNote, Reveal and CopyPath; root routes worker/OS actions.
- [x] Subscribe to Enter for Create/Rename; preserve validation and busy guards.
- [x] Report exact integration API and focused test results.

## Task 2: Blank editor diagnosis and repair

Owner: editor subagent. Files: `markdown_editor.rs`, `web_editor_host.rs`, editor frontend if proven necessary. Root owns outer layout and native smoke integration.

- [x] Trace host creation, ready, visibility, bounds and read/edit mode changes.
- [x] Reproduce with a visible native viewport assertion and user-style input; report failing evidence before patching.
- [x] Fix proven cause; add targeted regression assertions for first-open/page/history/resize.
- [x] Keep raw content, readonly semantics and async flush intact; run relevant protocol/browser tests.

## Task 3: Display setting, contained storage scrolling and branding

Owner: root (may delegate an independent focused implementation). Files: core Settings model, `app.rs`, `views.rs`, `storage_ui.rs`, `notes_view.rs`, `note_editor.rs`, `main.rs`.

- [x] Add failing settings deserialization/default/persistence tests; add `#[serde(default)] pub show_file_paths: bool` initialized false.
- [x] Add a persistent settings checkbox; propagate the setting through note/detail/edit/storage path rendering.
- [x] Reproduce nested wheel propagation; bind inner scroll handle and stop bubbling without swallowing its own scroll.
- [x] Replace visible titles/notifications/README branding, keeping storage IDs unchanged.
- [x] Wire note menu actions to explicit Explorer selection and clipboard, handling missing files visibly.

## Task 4: Native regression, review and release

Owner: root and independent reviewer.

- [x] Extend native smoke with actual drag/drop, right click, Enter, path toggling/restart and nested scroll assertions.
- [x] Verify real visible reading/editing after navigation and save actual input.
- [x] Review integrated diff for stale actions, data-loss paths and user-visible layout failures.
- [x] Run format, Clippy, workspace tests, browser tests and fresh native smoke; bump to 0.5.0 and update docs.
- [ ] Commit/push, verify CI, release and verify the actual published package.
