# Adaptive product interface implementation plan

Use subagent-driven-development for independent tasks, then focused reviews and a broad review. Existing 0.6.0 changes are uncommitted and must be preserved. No destructive cleanup, user-process interruption, or release before these changes pass.

## A. Theme model and settings (agent)

Own core theme types/validation, preferences appearance persistence, `theme.rs`, `components.rs`, `settings_ui.rs`, settings portions of `views.rs`, focused tests. Coordinate hooks; root owns `app.rs`, `main.rs`, editor bridge and global color migration.

- [x] Add backward-compatible theme mode, preset/custom token models and validation regressions.
- [x] Extend categorized atomic preference save/default/load/migration and one shared draft, with no implicit disk writes.
- [x] Implement resolved palette and GPUI component theme application API; provide editor-safe token payload.
- [x] Redesign categories and grouped shortcut rows, search/grouping, keycap capture/conflict presentation, appearance previews and custom preset management.
- [x] Report exact hooks and focused tests to root. Do not run Cargo/native concurrently.

## B. Contextual Markdown editing (agent)

Own frontend sources/tests/dist and native `shortcuts.rs` registry only. Root owns native Markdown bridge, click-to-edit host policy and note toolbar.

- [x] Replace always-visible language select with active-fence bottom-right input and ranked five-item autocomplete; unsupported commit clears token; preserve metadata/EOL/undo/focus.
- [x] Add Tab/Shift+Tab list transformations, headings 0..6 and other format commands using customizable action registry.
- [x] Compact formatting toolbar/menu; expose safe theme tokens update and readonly click request `{type:'requestEdit',generation,position}` with UTF-16 offset and caret restoration hook.
- [x] Browser tests first, then verify type/build/tests and inspect light/dark screenshots. Publish interface contract before root integration.

## C. Compact library and external drag feedback (agent)

Own `notes_view.rs`, `folders_ui.rs`, `note_actions.rs`, a new external-drop UI/model module if needed. Root owns `app.rs`, native host hooks and note-editor header.

- [x] Compact plus/sort/overflow menus, no persistent import explanations or four-button sorting row.
- [x] Clear folder/note hierarchy, compact metadata/source badges, retain exact selection/context and drag rules.
- [x] Implement responsive external-drop feedback state/render API; root integrates platform/WebView lifecycle. No path enumeration or body parsing during drag moves.
- [x] Focused tests for menu actions/sorting and drop lifecycle; report IDs/native smoke changes and integration hooks.

## D. Root integration and notification/native performance

- [x] Native click-edit request validates generation/current managed note and preserves caret without draft loss; compact title/save/overflow header.
- [x] Integrate resolved themes at startup/load/save/discard and system appearance change, migrate explicit palette use across remaining native surfaces, synchronize editor tokens.
- [x] Integrate external drag enter/leave/drop events and native child visibility with no per-move reallocation; fix observed source of lag.
- [x] Content-sized animated centered notices, preserve input/native editor geometry and queued duplicates.
- [x] Update isolated smoke to menus, contextual editing/languages, drop overlay, theme modes/custom persistence and responsive notice sizes. Fix native resize expectations for real DPI-rounded pointer positions.

## E. Delivery

- [x] Focused reviews + broad final review; no known actionable issues left.
- [x] Rust/fmt/Clippy, frontend/browser, native UI/physical pixels, audit/licenses, optimized package startup; fresh screenshots/docs.
- [ ] Commit/push main+branch; require CI success; tag/release v0.6.0; verify downloaded ZIP hash/content/fresh startup and record evidence.

## F. Managed Markdown images (user follow-up)

- [x] Render inline and reference Markdown images through native-validated image data, including relative tracked-file resources and HTTPS images. Keep the offline editor unable to read arbitrary files or send credentials.
- [x] Insert from picker, clipboard and drop; one persistent asset group per note, readable timestamp/title/unique filenames; preserve source images.
- [x] Store image settings in their own transactional category: default local library images, user-selected absolute folder, or configured WebDAV/NAS with local cache; require successful upload before claiming cloud insertion.
- [x] Convert tracked notes into managed drafts by copying referenced images and rewriting only the managed copy; warn on missing resources and preserve Markdown syntax/ordinary links.
- [x] Bounded background resolution (current plus at most one draining cancelled task), request/generation/revision/library guards, bounded bytes/dimensions and no repeated image payloads for ordinary typing. Retain staged files on upload failure.
- [x] Verify per-note ownership, change of storage root, migrated library resolution, remote transfer, stale responses, native rendering and draft restart.
