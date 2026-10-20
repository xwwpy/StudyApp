# WW-StudyApp note interactions and visible editor

User authorization: continue implementation and delivery without review pauses. Preserve all existing data and do not bulk-delete files or folders.

## Behavior

- Notes can be dragged from the note list onto a folder or Unclassified. Only metadata moves; source files and history remain unchanged. Reject cross-library stale drag payloads and maintenance-time mutations.
- Right-click a note for Move to folder, Reveal in File Explorer, and Copy file path. Move destinations include Unclassified and every full folder path except the note's current folder. Folder moves also exclude themselves and descendants. Large destination lists remain scrollable.
- Enter in the directory name input submits Create/Rename exactly once, respecting validation and busy state.
- Note paths are hidden by default. A persistent settings control enables file-location display in note rows/detail/editing and storage entries. Explicit reveal/copy actions remain available. Essential location selection controls in Settings remain usable.
- Diagnose blank reading and editing through actual native viewport visibility and human input, not only DOM state. Validate first open, page switches, history/editor switches, resize and restart. Preserve raw Markdown and draft durability.
- The storage copy list owns wheel events within its bounds, including when at either end; scrolling elsewhere still scrolls Settings.
- Visible product branding is WW-StudyApp. Keep existing database/config directory identifiers, sync formats and package/crate identities for compatibility. New published portable asset names may use WW-StudyApp.

## Approach and alternatives

Retain GPUI folder metadata and native menus rather than physically moving files (would break external tracking). Keep the existing offline WebView editor, identify its native visibility/layout failure before considering replacement; a replacement would unnecessarily risk draft/protocol behavior. Store the display preference with existing durable settings rather than a transient UI flag.

## Verification

Real native tests must exercise pointer drag, right-click destination selection, Enter, nested scrolling, visible editor content and actual keyboard input. Regression tests cover excluded/descendant destinations, stale drag rejection and old settings defaulting to hidden paths. Full Rust/frontend/native/release gates precede delivery. No user application is terminated or user library used for testing.
