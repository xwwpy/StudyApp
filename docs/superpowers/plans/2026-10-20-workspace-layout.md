# Workspace layout and popup feedback implementation plan

> **For agentic workers:** Use subagent-driven-development for independent implementation tasks, followed by focused and whole-change review. Root integrates and publishes. Steps use checkbox tracking.

**Goal:** Ship an adjustable, clearly divided note workspace with accurate drag previews and consistent popup feedback.

**Architecture:** Keep GPUI shell, logical note folders and persistent native editor. Add validated layout settings, window-level divider handling, bounded folder/list panes, and centralized popup presentation. Preserve the document protocol.

**Tech Stack:** Rust 1.98, GPUI Kit 0.7.3, offline WebView2, PowerShell.

## Global constraints

- No bulk deletion, no user-library testing, no stopping user processes.
- 0.5.0 source/tag stays intact; this feature version is 0.6.0.
- Root owns `app.rs`, `main.rs`, new `workspace_layout.rs`, core settings, integration smoke and release files.
- Library implementer owns `folders_ui.rs`, `notes_view.rs`, `note_actions.rs`.
- Feedback implementer owns new `feedback.rs`, `editor_lifecycle.rs`, `note_editor.rs`, `storage_ui.rs`, `sync_ui.rs`. Coordinate folder dialogs with the library implementer; do not edit their files.
- No simultaneous Cargo/native runs; root runs integrated builds/tests. Agents may inspect dependencies and write focused tests, then request a run.

## Task 1: Persisted panel layout (root)

- [x] Add `LayoutSettings` with integer widths and defaults:
  ```rust
  pub struct LayoutSettings {
      pub navigation_width: u32, // 184, valid 144..=320
      pub folders_width: u32,    // 176, valid 144..=360
      pub notes_width: u32,      // 240, valid 180..=440
      pub navigation_visible: bool, // true
  }
  ```
  Add `#[serde(default)] pub layout: LayoutSettings` and `Command::SaveLayout(LayoutSettings)`. Merge only layout from current settings. Reject out-of-range widths atomically; leave timer and other settings intact.
- [x] Add tests in `crates/studyapp-core/tests/settings.rs`: legacy defaults, reopen equality, invalid command leaves state unchanged, interleaved layout/learning/path commands preserve all fields.
- [x] Add `workspace_layout.rs` with `Pane::{Navigation,Folders,Notes}`, `EffectiveLayout { navigation: Pixels, folders: Pixels, notes: Pixels }`, and `StudyApp::effective_layout(&Window)`, `StudyApp::resize_handle(Pane, &mut Context<Self>) -> Stateful<Div>`, `StudyApp::is_layout_resizing()`.
- [x] Root fields hold current preferred settings, resize origin/state and dirty persistence. Use observed dividers `resize-navigation`, `resize-folders`, `resize-notes`; reserve 280 pixels for document. Header `toggle-navigation` is always available.
- [x] Finish pointer capture/release and left-edge collapse; save only on drag completion/toggle. Restore WebView after resize without reloading content.

## Task 2: Library panes and drag preview (library agent)

- [x] Inspect GPUI drag callback cursor-offset semantics; add a regression from a note's bottom-right corner asserting preview distance stays small.
- [x] Implement compact bounded preview near the pointer, preserving `NoteDrag` logical identity and drop validation.
- [x] Change `StudyApp::notes` to accept `window: &mut Window`. Read `self.effective_layout(window)`; render folder, list, detail as independent adjacent panes. Insert `self.resize_handle(Pane::Folders, cx)` and `Pane::Notes`.
- [x] Preserve IDs used by native smoke. Add section headers, tree indentation/guide/disclosure, folder and note icons, selected row background/accent and bounded title text. Keep folder and note scroll containers independent.
- [x] Convert folder action forms into modal presentation using the feedback agent's agreed API; emit feedback/overlay state to the root so native editor visibility is coordinated. Preserve Enter submit and all busy/stale validation.
- [x] Report changed APIs and targeted test cases to root; do not commit shared workspace changes independently.

## Task 3: Dialogs and floating notifications (feedback agent)

- [x] Inspect GPUI dialog/overlay and focus APIs. Design a reusable modal shell with bounded scroll content, cancel/Escape and action focus. It must block background events.
- [x] Move note confirmation and managed-copy confirmation out of inline page flow into that shell. Keep existing confirmation IDs and single-item command semantics.
- [x] Add a floating result view with stable `dismiss-notice`, bounded long text and automatic success expiry; root integrates it in header space above the native WebView and removes inline notice layout.
- [x] Route sync action errors/results through floating feedback while retaining static sync status labels. Coordinate folder validation/confirmation presentation with library agent.
- [x] Provide root integration instructions: fields/events, render call and native visibility predicate. Do not overwrite `app.rs` or `notes_view.rs`.

## Task 4: Pointer feedback and native verification (root)

- [x] Introduce a consistent enabled-button cursor helper after inspecting Button style precedence. Apply throughout application controls and WebView toolbar. Disabled controls must remain non-action cursors.
- [x] Verify actual OS cursor states for button, tree row, disabled control and divider; verify controlled checkbox cursor/activation states in component tests. Add meaningful native assertions rather than source-text checks.
- [x] Extend native smoke for all three dividers, navigation collapse/restore, bottom-right drag, popup interaction and native editor visible after resize/dialog. Validate core persistence and constrained small-window layout.
- [x] Run `cargo fmt --all -- --check`, `cargo test --locked --workspace --all-features`, `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`, frontend checks if changed, and fresh native smoke.
- [ ] Independently review implementation and actual screenshots. Resolve findings, update docs/images/version, build/package, commit/push, verify CI, release 0.6.0 and verify published ZIP.
