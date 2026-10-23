# Compact workspace and configuration implementation plan

> **For agentic workers:** Use subagent-driven-development for bounded tasks and dispatching-parallel-agents only for disjoint owned files. Do not commit shared implementation independently; root reviews/integrates and owns final Cargo/native tests.

**Goal:** Implement the user's expanded 0.6.0 requirements before publishing.

**Architecture:** Deferred layout candidates and an owned native guide preserve WebView presentation. A categorized configuration store publishes a complete generation atomically; settings UI edits a snapshot copy, and saved bindings reconfigure the persistent editor.

**Tech Stack:** Rust 1.98, GPUI Kit 0.7.3, config 0.15.27 with JSON-only features, Windows/WebView2, CodeMirror 6.

## Global constraints

- No recursive or bulk deletion. Preserve old fixtures, generations and original tracked files.
- Never stop the user's application. Root alone launches fresh isolated native tests.
- Keep `v0.6.0` untagged until the expanded implementation passes all gates.
- Explicit user authorization waives further approval checkpoints.
- Default widths: navigation 160, folders 144, notes 200. Minimum widths: navigation 128, folders 112, notes 160; existing maxima remain 320/360/440.
- Folder visibility is a serde-defaulted `LayoutSettings::folders_visible: bool`, default true.
- Result feedback is horizontally centered in the full window; existing dialogs stay centered in both axes.
- Shortcuts use `BTreeMap<String, String>`; normalized string is `Ctrl+Shift+S`, multiple alternatives use `;`, empty value unbinds. No plain text key shortcuts; allow function keys and modifier combinations, reject OS-reserved combinations and duplicates.

## Task 1: Configuration transaction backend

Owned files: new `crates/studyapp-desktop/src/preferences.rs`, core models/storage/engine settings APIs, desktop `worker.rs`, `sync_runtime.rs`, `storage_backend.rs`, desktop Cargo manifest and lockfile. Root owns `main.rs` module declaration and all app/UI files.

Interfaces:

```rust
pub struct PreferenceSnapshot {
    pub revision: String,
    pub settings: studyapp_core::Settings,
    pub shortcuts: std::collections::BTreeMap<String, String>,
    pub sync: crate::sync_runtime::SyncSettings,
}
pub struct SavePreferences {
    pub expected_revision: String,
    pub settings: studyapp_core::Settings,
    pub shortcuts: std::collections::BTreeMap<String, String>,
    pub sync: crate::sync_runtime::SyncSettings,
    pub password: Option<zeroize::Zeroizing<String>>,
}
// Add Work::SavePreferences(SavePreferences).
// Add Update.preferences: Option<PreferenceSnapshot>.
// Add Update.preferences_result: Option<Result<(), String>>.
```

Consumes `crate::shortcuts::{default_bindings, validate_bindings}`; the bindings task owns that module. `PreferenceSnapshot` is cloneable; secrets stay out of snapshots and logs.

- [x] Add backend regression tests for old DB/sync import, categorized config reads, defaults files, no-write prepare, CAS refusal, failed publication preserving previous settings, restart replay and library isolation.
- [x] Read each JSON category through `config::Config::builder().add_source(config::File::from(path)).build()?.try_deserialize()`. Bound input sizes and reject links/path traversal in generation names.
- [x] Prepare settings and protected credentials with no I/O commits. Create complete unique generation with `create_new`, flush files, atomically replace manifest. Only after publication install validated settings/runtime in memory without fallible I/O.
- [x] Retain immutable built-in/default category sources and a safe defaults accessor for the UI. No password in defaults.
- [x] Route existing learning/layout/path/sync save work through the same store so legacy native tests and non-settings layout changes cannot diverge. Preserve settings fields on scoped patches.
- [x] Separate sync baseline state from config; identity-check loaded baseline before use, preserve current compatible state and credentials.
- [x] Copy categorized config in library migration and account for its bytes. Backup restore retains current independent preferences, restoring learning data; reapply preferences before emitting a snapshot and update explanatory UI/docs in root integration.
- [x] Validate relevant core/backend tests, report exact commands/results and any compatibility changes in `.scratch/preferences-implementation.md`.

## Task 2: Directory interaction and deferred resize

Owned files: `folders_ui.rs`, `workspace_layout.rs`, new `resize_overlay.rs`, `note_actions.rs` only if directory preview needs a shared helper. Root owns app/notes_view/editor_lifecycle integration and core layout fields.

- [x] Add tests proving selected tree dimensions do not change, only one selected background is used, directory drop rejects self/descendants/current parent/stale library and depth overflow.
- [x] Remove bottom folder action panel. Add row context menu with create/rename/move/delete, reuse existing modal actions and validation, preserve keyboard accessibility. Directory drag payload includes library identity and folder ID; drop emits `Command::MoveFolder` only on release.
- [x] Introduce a candidate `ResizeState`; `move_divider` only updates candidate and guide position. `finish_resize` applies once if changed; cancel restores untouched original. Expose `toggle_folders` like `toggle_navigation`; collapse each pane at its own left edge and retain previous restored width.
- [x] Keep WebView visible during resize. Use an owned, no-activate, input-transparent native guide overlay for a dashed vertical line, DPI-aware placement, lifecycle teardown and bounds clamp; do not draw over unrelated windows or steal focus.
- [x] Reduce visual separator thickness while retaining hit size. Test candidate immutability and fitting for hidden folders/navigation.
- [x] Report root integration APIs and new native test assertions in `.scratch/workspace-implementation.md`. Do not run Cargo/native concurrently.

## Task 3: Shortcut registry and editor remapping

Owned files: new `crates/studyapp-desktop/src/shortcuts.rs`, editor source/tests/dist. Root declares module and builds settings controls/native dispatch/bridge transport.

Interfaces:

```rust
pub struct ShortcutAction {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub default: &'static str,
}
pub const SHORTCUT_ACTIONS: &[ShortcutAction];
pub fn default_bindings() -> std::collections::BTreeMap<String, String>;
pub fn validate_bindings(bindings: &std::collections::BTreeMap<String, String>) -> anyhow::Result<()>;
pub fn normalize_binding(binding: &str) -> anyhow::Result<String>;
```

Editor bridge: `setShortcuts(bindings: Record<string, string>): void`, and for app actions post `{type:"shortcut", action:string, generation:number}`. Action IDs prefixed `app.` for native commands; editor action IDs map to a fixed allowlist.

- [x] Registry covers all application shortcut commands and every existing editor command binding. Include editor save/bold/italic/heading/code/list/task/quote/source/undo/redo; app new/import/settings/library/navigation-toggle/folders-toggle/search. Preserve standard text-navigation key semantics.
- [x] Implement normalization, duplicate/reserved/unknown action validation and clear-binding/default behavior with focused tests.
- [x] Replace fixed shortcut handlers and history bindings with a reconfigurable compartment; remove old binding when changed. Native app shortcuts work while WebView focused through generation-scoped messages. Never evaluate config strings as code.
- [x] Preserve text/revision/selection/undo across remap. Respect IME composition, read-only mode, pending flush and modal state in host. Update toolbar tooltips from current bindings.
- [x] Browser tests verify custom save once, old binding inactive, custom bold, empty binding, default reset, reconfiguration preserves history, app routing and malformed input handling.
- [x] Run editor verify once edits settle; write `.scratch/shortcuts-implementation.md`.

## Task 4: Root UI integration and settings draft

Owned files: app/main/views/editor_lifecycle/notes_view/markdown_editor and new `settings_ui.rs`; sync_ui/storage_ui integration, docs, smoke tests.

- [x] Add configuration snapshot/result handling without overwriting dirty UI inputs during background polling. Separate current committed values, entered input strings and initial defaults.
- [x] Settings controls edit the draft, including path display, layout and sync; replace independent saves with unified save/reset actions and visible dirty indicator. Expose bindings by group with editable combinations and validation.
- [x] Route page changes/new note/import/library operations/automatic reminder navigation/close through one dirty guard. Modal offers save-and-continue, discard-and-continue, continue editing; request IDs/library/revision prevent stale success continuing an unrelated action. Failure preserves inputs.
- [x] Reset populates defaults into draft after confirmation; does not delete notes, reset data directory or write config until saved.
- [x] Replace navigation text toggle with fixed visible icon; add persistent folder icon toggle; conditional folder pane sizing. Center floating notification on full viewport.
- [x] Wire deferred resize and keep original WebView bounds visible during pointer movement. Verify selection/viewport/content unaffected.
- [x] Resolve existing CI stale Win32 cursor issue by tracing native WM_SETCURSOR/WM_GPUI_CURSOR_STYLE_CHANGED; fix actual cursor update if confirmed, preserving vendor provenance and packaging.
- [x] Extend native tests for configuration guards, no-write cancel, release-only resize and visible editor, directory right-click/drag, centered feedback and shortcut persistence.
- [ ] Independent review, full Rust/editor/native tests, Clippy/fmt/security, package/startup, push/CI, tag/release, published ZIP verification. Update ledger/docs with actual evidence only.
