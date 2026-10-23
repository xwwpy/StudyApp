# Adaptive product interface implementation plan

Use subagent-driven-development for independent tasks, then focused reviews and a broad review. Existing 0.6.0 changes are uncommitted and must be preserved. No destructive cleanup, user-process interruption, or release before these changes pass.

## A. Theme model and settings (agent)

Own core theme types/validation, preferences appearance persistence, `theme.rs`, `components.rs`, `settings_ui.rs`, settings portions of `views.rs`, focused tests. Coordinate hooks; root owns `app.rs`, `main.rs`, editor bridge and global color migration.

- [ ] Add backward-compatible theme mode, preset/custom token models and validation regressions.
- [ ] Extend categorized atomic preference save/default/load/migration and one shared draft, with no implicit disk writes.
- [ ] Implement resolved palette and GPUI component theme application API; provide editor-safe token payload.
- [ ] Redesign categories and grouped shortcut rows, search/grouping, keycap capture/conflict presentation, appearance previews and custom preset management.
- [ ] Report exact hooks and focused tests to root. Do not run Cargo/native concurrently.

## B. Contextual Markdown editing (agent)

Own frontend sources/tests/dist and native `shortcuts.rs` registry only. Root owns native Markdown bridge, click-to-edit host policy and note toolbar.

- [ ] Replace always-visible language select with active-fence bottom-right input and ranked five-item autocomplete; unsupported commit clears token; preserve metadata/EOL/undo/focus.
- [ ] Add Tab/Shift+Tab list transformations, headings 0..6 and other format commands using customizable action registry.
- [ ] Compact formatting toolbar/menu; expose safe theme tokens update and readonly click request `{type:'requestEdit',generation,position}` with UTF-16 offset and caret restoration hook.
- [ ] Browser tests first, then verify type/build/tests and inspect light/dark screenshots. Publish interface contract before root integration.

## C. Compact library and external drag feedback (agent)

Own `notes_view.rs`, `folders_ui.rs`, `note_actions.rs`, a new external-drop UI/model module if needed. Root owns `app.rs`, native host hooks and note-editor header.

- [ ] Compact plus/sort/overflow menus, no persistent import explanations or four-button sorting row.
- [ ] Clear folder/note hierarchy, compact metadata/source badges, retain exact selection/context and drag rules.
- [ ] Implement responsive external-drop feedback state/render API; root integrates platform/WebView lifecycle. No path enumeration or body parsing during drag moves.
- [ ] Focused tests for menu actions/sorting and drop lifecycle; report IDs/native smoke changes and integration hooks.

## D. Root integration and notification/native performance

- [ ] Native click-edit request validates generation/current managed note and preserves caret without draft loss; compact title/save/overflow header.
- [ ] Integrate resolved themes at startup/load/save/discard and system appearance change, migrate explicit palette use across remaining native surfaces, synchronize editor tokens.
- [ ] Integrate external drag enter/leave/drop events and native child visibility with no per-move reallocation; fix observed source of lag.
- [ ] Content-sized animated centered notices, preserve input/native editor geometry and queued duplicates.
- [ ] Update isolated smoke to menus, contextual editing/languages, drop overlay, theme modes/custom persistence and responsive notice sizes. Fix native resize expectations for real DPI-rounded pointer positions.

## E. Delivery

- [ ] Focused reviews + broad final review; no known actionable issues left.
- [ ] Rust/fmt/Clippy, frontend/browser, native UI/physical pixels, audit/licenses, optimized package startup; fresh screenshots/docs.
- [ ] Commit/push main+branch; require CI success; tag/release v0.6.0; verify downloaded ZIP hash/content/fresh startup and record evidence.
