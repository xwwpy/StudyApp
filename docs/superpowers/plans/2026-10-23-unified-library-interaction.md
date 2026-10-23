# Unified Library Interaction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development or executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Deliver a responsive single-tree library, animated concurrent notifications, code language controls, and categorized settings with recorded shortcut input.

**Architecture:** Keep the existing worker and immutable configuration protocol. Isolate tree indexing, notification lifecycle, and shortcut recording models; retain the persistent WebView editor. Independent editor, settings, and tree tasks may run in parallel; root owns application integration, native host performance and verification.

**Tech Stack:** Rust 1.98, GPUI Kit 0.7.3, WebView2, CodeMirror, config 0.15.27, Windows native smoke, browser tests.

## Global Constraints

- User authorized autonomous completion; no repeated design or release approval.
- No bulk/recursive deletion. Preserve user processes and test evidence.
- Do not publish until this scope passes. Agents do not commit, push, or run Cargo/native concurrently; root serializes these checks.
- Each agent records exact changed files, tests, limitations and integration hooks in `.scratch`.

## Task 1: Unified library and ordering

Own `notes_view.rs`, `folders_ui.rs`, `workspace_layout.rs`, a focused tree model module, and minimal core/config additions for persisted sort. Root owns `app.rs` integration.

- [ ] Add status/index/sort regressions: latest recorded hash neutral, later modified hash M, first unrecorded A, stable equal-title/time ordering, hierarchy/search ancestor retention, tracked/managed source distinction.
- [ ] Flatten expanded folders and child notes into one render list; preserve menus, note dates, drag/drop validation, detached/search selection, and selected note visibility.
- [ ] Replace two widths/handles with one library width; preserve legacy config readability, body reservation, icon reveal, left-edge hide and commit-on-release semantics.
- [ ] Persist sort choice through the categorized preference store without overwriting other drafts.
- [ ] Cache snapshot-derived metadata outside pointer render; report root app integration and native test changes needed.

## Task 2: Settings sections and shortcut recording

Own `settings_ui.rs`, settings rendering in `views.rs`, `shortcuts.rs`, and focused recording helpers. Root owns global keystroke interception in `app.rs`.

- [ ] Add recording/normalization/conflict tests before behavior: modifiers ignored, Escape cancels, captured command not executed, duplicate aliases mark all actions, invalid draft cannot save.
- [ ] Add category navigation with one shared draft/save/reset bar. Keep storage and sync controls and their asynchronous guard semantics.
- [ ] Record actual keys from a dedicated focus target; permit replace/append/clear and cancellation. Detect conflicts while editing and display them beside all affected rows.
- [ ] Preserve the previous leave/close/save/reset workflow; communicate interception hooks and smoke-test category navigation to root.

## Task 3: Fenced-code language controls

Own `editor/src`, editor browser tests and generated distribution assets only.

- [ ] Browser regressions for picker display, language update and highlighting, text/CRLF preservation, undo/redo, readonly, unknown language and multiple blocks.
- [ ] Implement an accessible fence language selector using the existing language registry; perform a normal CodeMirror transaction and keep document selection stable.
- [ ] Run `npm run verify`, inspect the resulting editor screenshot, report exact evidence.

## Task 4: Root responsiveness and notifications

Own `app.rs`, `feedback.rs`, new notification model/view, `markdown_editor.rs`, native host code and `resize_overlay.rs`. Coordinate layout diagnostics with Task 1.

- [ ] Trace native guide/layout and snapshot/render work during dragging; create deterministic regression counters or tests before optimizing redundant work.
- [ ] Implement unique queued notices, bounded visible stack, paused hover timers, animated removal, and isolated frame invalidation. Route every result event through enqueue rather than comparing text equality.
- [ ] Integrate tree selection events/settings capture hooks while preserving modal/close/worker ownership rules.

## Task 5: Verification and delivery

- [ ] Review each independent task for requirements and code quality, then broad final review.
- [ ] Update native smoke to the single-tree UI and categories; test drag frame behavior, concurrent identical notifications and fading, sort/status/source labels, real captured shortcuts and red conflicts, language picker.
- [ ] Run Rust tests, Clippy, fmt, browser checks, native window checks, audit/license gates and package startup. Update docs with observed results.
- [ ] Commit/push, require successful CI, tag/release and verify downloaded ZIP and fresh EXE startup; then record published evidence.
