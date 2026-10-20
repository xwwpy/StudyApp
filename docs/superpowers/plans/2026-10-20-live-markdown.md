# Live Markdown Implementation Plan

> **For agentic workers:** Use subagent-driven-development for isolated buffer implementation and requesting-code-review for integration review.

**Goal:** Native, single-column Markdown block editing with immediate rendering and a full-source escape hatch.

**Architecture:** A pure Markdown buffer preserves bytes and bounded undo history. A dedicated GPUI entity owns the buffer, source/block inputs and cached previews. StudyApp consumes only the entity's current Markdown and Change event.

**Tech Stack:** Rust 1.98, GPUI Kit 0.7.3, pulldown-cmark 0.13; no new dependencies.

## Global Constraints

- Never bulk-delete files or directories.
- Preserve `NoteDraft` schema and existing save/conflict/library contracts.
- Sanitize previews with the same policy as the current reader.
- Source-mode content is never truncated; live rendering is bounded.
- User waived review gates; implement, verify and push without asking again.

## Task 1: Lossless buffer and rendering blocks

Files: create `crates/studyapp-desktop/src/markdown.rs`; modify `lib.rs` and, if necessary, `presentation.rs`.

Interfaces:

```rust
pub struct MarkdownBlock { pub range: std::ops::Range<usize>, pub html: String }
pub fn markdown_blocks(source: &str) -> Vec<MarkdownBlock>;
pub struct MarkdownBuffer { /* private raw text and bounded history */ }
// MarkdownBuffer::new(String), text(&self)->&str, replace(Range<usize>, &str)->bool,
// checkpoint(), undo()->bool, redo()->bool, can_undo()->bool, can_redo()->bool.
```

- [x] Write failing tests for exact concatenation of block source slices (CRLF, Unicode, fences, nested lists, tables, definitions), sanitized block HTML, replace validation and undo/redo across checkpoints.
- [x] Run `cargo test -p studyapp-desktop --lib markdown --locked`; verify failures before implementation.
- [x] Implement full-document parser offset grouping and bounded undo/redo. Active edit ranges are owned by UI, not automatically reparsed on every keystroke.
- [x] Run focused tests, inspect output, review buffer contract before integration.

## Task 2: Native live editor and integration

Files: create `crates/studyapp-desktop/src/markdown_editor.rs`; update `main.rs`, `app.rs`, `note_editor.rs`, `smoke.rs`.

Interfaces: `MarkdownEditor::new(window,cx)`, `set_value(String,window,cx)`, `value()->&str`, `set_readonly(bool,cx)`; emits `MarkdownChanged`. Native smoke may inspect public(crate) input/model state.

- [x] Extend native smoke to require default live mode and live preview after a real block input edit; demonstrate old editor fails this requirement.
- [x] Render a persistent toolbar, bounded scroll area and cached clickable blocks. Active block uses Textarea plus immediate sanitized preview; source mode uses whole text with preview.
- [x] Implement toolbar insertion using selected byte range; keyboard completion and shared undo/redo; flush current input before mode changes/save.
- [x] Replace StudyApp's `note_content`/`note_preview` with this entity. Draft capture reads canonical Markdown directly. Saving freezes the entity; failure unlocks it.
- [x] Test editing, recovery, version persistence, source round trips, Unicode and undo using `cargo build -p studyapp-desktop --features ui-test --locked` and `scripts/smoke.ps1` in a new isolated directory.

## Task 3: Review and delivery

Files: README, release notes, verification evidence, editor screenshot, workspace version/lockfile.

- [x] Independent code review against the spec; fix correctness issues and add regression coverage.
- [x] Run fmt, workspace tests, all-feature clippy, native smoke and inspect screenshots.
- [x] Document actual interaction and limitations. Commit and push authorized changes to the existing GitHub repository.
- [ ] Verify main CI; package/publish a feature release only after gates pass. Report links and evidence accurately.
