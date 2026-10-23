# Product reliability implementation plan

> **For agentic workers:** Use subagent-driven-development for scoped tasks and independent review. The user authorized autonomous execution.

**Goal:** Resolve the reported document, storage, draft and interaction failures, and add verified user-triggered updates.

**Architecture:** Preserve GPUI/WebView and exact Markdown text while extracting document styles, draft collection, folder picker and pointer/overlay primitives. Background workers handle persistence, media, synchronization and update staging with explicit completion acknowledgement.

**Tech Stack:** Rust, GPUI Kit, CodeMirror 6, WebView2, SQLite, config crate, PowerShell release pipeline.

## Global constraints

- Preserve all user data, external tracked files, history, existing .scratch evidence and unrelated changes.
- No bulk deletion or recursive deletion; delete only one explicitly selected file when required.
- Keep work on codex/studyapp-local; no release tag before fresh verification.
- Never expose credentials, low-level OS errors or internal paths as primary user-facing failures.
- Local native keyboard fallback remains explicitly labelled; CI retains strict WIN32_SENDINPUT.
- Do not weaken concurrency, exact-text, native-pixel, input-focus or source-preservation assertions to pass tests.

## Execution map

Independent read-only investigations are first recorded in `.scratch/product-editor-audit.md`, `.scratch/product-storage-audit.md`, `.scratch/product-drafts-audit.md`. Implementers receive one task brief, report to one file and avoid shared-file edits without a handoff. Coordinate Cargo and native runs; only one owner at a time. Each task requires regression evidence and a focused review before final integration review.

### 1. Markdown styling and themes

Files: `editor/src/`, `editor/tests/`, `src/theme.rs`, appearance portions of `src/settings_ui.rs`, core appearance model and preferences round-trip tests.

- [ ] Reproduce computed foreground/background contrast, shared table geometry, code block boundaries and nested list depth with mixed Markdown fixtures under every preset and a custom dark/light theme.
- [ ] Add failing browser assertions for table columns across unequal-length rows, nested list continuation, highlighted tokens contrast and exact editable source/undo; add Rust preset round-trip and invalid style tests.
- [ ] Implement resolved Markdown style tokens and coherent decoration/layout, complete preset editing UI, neutral ordinary control colors; document the token contract for root.
- [ ] Run frontend type/build/Playwright, relevant Rust tests and capture light/dark/custom rendered screenshots; review scoped diff.

### 2. Shared overlays, save feedback and pointer surfaces

Files: `src/components.rs`, `src/feedback.rs`, `src/notifications.rs`, `src/note_actions.rs`, `src/folders_ui.rs`, `src/notes_view.rs`, `src/editor_lifecycle.rs`, native host region code, minimal vendor popup bounds hook.

- [ ] Add a popup bounds event covering nested menus; implement a named-source overlay registry so one surface cannot clear another's occlusion.
- [ ] Replace whole-editor hiding for menus with actual clipped popup bounds and preserve focus/text/selection. Test native visible=true and unchanged bounds during More menu, submenu, dismissal.
- [ ] Implement pointer-following hover surface and common row-height drag preview; test two mouse positions and viewport edges.
- [ ] Move notices to the top, eliminate shadow-padding cutouts and image-progress success messages; assert an unchanged save creates neither a version nor a result notice.
- [ ] Verify actual desktop pixels, menu hit testing, notification fade/duplicate queue, source row/drag heights and no per-frame metadata rebuild.
- [ ] Audit settings/history/library/statistics and dialogs for missing scrollbars or partial card clipping; use bounded scroll bodies with fixed headers/footers and visible scroll thumbs, then test both inner boundaries and outer scrolling.

### 3. Storage configuration and management

Files: `src/settings_ui.rs`, `src/sync_ui.rs`, `src/storage_ui.rs`, `src/storage_backend.rs`, new directory picker component, core image settings, sync provider models/clients and secret storage.

- [ ] Add regression for successful deletion followed by refresh with a cleaned-copy state and no OS error, preserving tracked originals and version history.
- [ ] Add common folder selector with consistent display, background picker and stale library/settings generation rejection; migrate existing directory actions.
- [ ] Expand Cloud/Both selection to show provider/address/account fields; reject incomplete settings; make connection test consume current draft values.
- [ ] Implement generic S3-compatible object storage configuration/provider with endpoint, bucket, region, key pair, prefix and addressing choice. Alibaba OSS is a compatibility example, not a vendor-specific requirement. Test signing, bounded reads, immutable asset writes, conflict responses, invalid configuration and credential redaction. Keep asset configuration independent from existing Markdown sync unless the user explicitly selects reuse.
- [ ] Redesign usage sections and individual cleanup actions, include drafts/images and safe missing-file handling; run core/sync/storage tests and native settings checks.

### 4. Multiple automatic drafts and native image paste

Files: `src/drafts.rs`, `src/editor_lifecycle.rs`, `src/note_editor.rs`, `src/app.rs`, `src/notes_view.rs`, `src/media_ui.rs`, native clipboard bridge and startup/migration code.

- [ ] Regress A→B→A unsaved edits and shutdown/restart of two drafts; migrate legacy single draft once; corrupt draft does not hide other drafts.
- [ ] Implement stable draft IDs and independently atomic per-draft files, serialized writes, durable switching and built-in Drafts tree. Keep note version/image identity per draft.
- [ ] Save/discard only the target draft; multi-draft close/migration/update flush preserves others; failed persistence cancels transition.
- [ ] Add native clipboard fallback for bitmap and copied image files, route to existing media jobs with exact insertion position and no duplicate browser insertion.
- [ ] Verify real-window draft switching, multiple recovery, individual discard/save, paste image, cloud failure and source preservation.

### 5. Updates

Files: new `src/update_backend.rs`, `src/update_ui.rs`, helper entrypoint, about settings section, package/release scripts, update protocol documentation and isolated integration tests.

- [ ] Add release parsing/version/asset-origin/size/hash regression fixtures before implementation.
- [ ] Implement background check/download/verified staging and explicit update state machine, with safe archive extraction and no user data access.
- [ ] Implement normal shutdown handshake and separate helper replacement/backup/rollback/restart using current_exe, preserving data-dir. Test in fixture install directories including spaces/Chinese and non-writable/failure cases.
- [ ] Expose Check update, release notes, download/progress and Restart to update, plus manual fallback; record the user-facing update scheme.

### 6. Delivery

- [ ] All scoped reviews and a final integrated review with no unresolved Critical/Important findings.
- [ ] Rust/fmt/Clippy, frontend/browser, strict CI native, security/licenses, release package startup and fresh screenshot verification.
- [ ] Commit/push main and branch, wait exact code CI, tag release and verify downloaded assets/startup. Record actual evidence and limitations without overstating live OSS/IME coverage.
