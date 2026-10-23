# Adaptive product interface design

The latest user feedback supersedes the initial 0.6.0 control presentation. Keep the single-tree, transactional configuration, reliable editor and storage model; complete this scope before publishing. The user explicitly authorized autonomous implementation and waived further review pauses.

## Chosen approach

Use the existing native GPUI shell with a restrained, desktop-adapted Apple Notes/Files style: grouped surfaces, small icon toolbars, contextual menus, consistent type scale and one accent. Retain the offline CodeMirror/WebView editor. A complete web shell rewrite would disturb native window and persistence guarantees; merely shrinking current buttons would retain the overcrowded hierarchy. No mobile layout or platform chrome is copied literally.

Reference: Apple HIG toolbars/menus (`https://developer.apple.com/design/human-interface-guidelines/toolbars`, `/menus`); Typora documents its language entry at the bottom-right of a fence (`https://support.typora.io/Code-Fences/`).

## Library and import

- One compact header (title, reveal, plus and sort/menu icons), one search row; remove persistent explanatory labels and separate import/four-sort controls.
- Plus menu offers new Markdown, new folder, import files, import folder. Sort menu offers name ascending/descending and updated time newest/oldest, with selected checkmark. Detached view is a menu toggle.
- Folders use a disclosure chevron and folder glyph, notes use document/source glyphs. Shared row rhythm, subtle selected fill and clear nesting. Keep A/M indicators, readonly/managed source information (compact badge or tooltip), timestamps and right-click actions.
- External files/folders hovering over the app reveal a responsive inset drop surface with a large plus and concise import hint. Read filenames only while hovering; enumeration/content reading remain in the worker after release. Leaving/cancelling clears the overlay. Internal note/folder drags do not activate external import.
- Overlay must appear above the native WebView without repeatedly creating/destroying it. Preserve editor document/selection during hover, restore on cancel and after drop. Busy/modal states reject safely with clear affordance.

## Editor

- Compact title/status/actions header; secondary note actions move into overflow menu. Formatting toolbar becomes concise grouped controls with a format menu.
- Clicking current managed-note text enters editing at the clicked position, with no separate Edit prerequisite. Preserve immutable history reading and readonly external source semantics: explicit copy editing remains available for external snapshots. Do not discard existing drafts or bypass async flush/conflict guards.
- Active fenced block shows a bottom-right language text field. Inactive blocks have no language control. Typing shows up to five nearby matching supported languages/aliases; arrow keys/Enter choose, Escape cancels. Commit on Enter/blur; unsupported input clears the language token. Preserve fences, content, trailing metadata, line endings and undo. Unknown imported language is not changed until edited.
- Tab/Shift+Tab indent/outdent Markdown list items (nested ordered, unordered and task lists), preserve selection and undo; code blocks retain code indentation. Ctrl+1..6 sets heading levels, Ctrl+0 paragraph, plus registered formatting commands. All bindings remain configurable and conflict-checked.

## Themes and settings

- Six settings categories remain, using compact navigation and grouped form rows. Shortcut rows show action + keycaps; click to record, clear/append via small controls, red inline conflicts; searchable/grouped shortcuts reduce scanning.
- Appearance settings support System, Light, Dark and saved custom presets. Persist safe palette/accent/font-size/spacing tokens, not executable CSS. Offer named custom create/update/delete within the shared draft; only Save commits. Reset restores factory defaults; discard restores saved theme. Preview changes may be immediate but never persist implicitly.
- Native shell, menus, inputs, notifications and editor use the same resolved theme. System mode responds to OS appearance changes. Old configurations migrate with defaults; appearance category stays separate and uses the existing atomic manifest protocol.

## Notifications

Content-sized cards with bounded width and wrapped text, compact icon/dismiss button, restrained shadow and fade/slide transition. Preserve distinct duplicates, bounded backlog and hover-pause. Horizontally centered for every window size. Long content must remain readable without covering an unoccluded native child; coordinate temporary native visibility only when actually necessary, never reflow editor geometry for animation.

## Verification

Regression-first for editor transformations, language completion, theme serialization/draft atomicity, drag enter/leave/drop and click-edit ownership. Browser screenshots verify light/dark/custom and editor language controls. Native isolated smoke verifies compact menus, drag overlay across editor, selection preservation, shortcut recording, themed settings, notification geometry and existing data workflows. Test-owned EXEs/data only; no bulk deletion and no stopping user PID 41512. Root serializes Cargo/native work. CI still requires Win32 SendInput. Publish only after local gates, independent review and remote CI.
