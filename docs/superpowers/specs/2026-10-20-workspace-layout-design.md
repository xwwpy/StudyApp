# WW-StudyApp workspace layout and feedback

## Authorization and scope

The user requested a drag-preview fix, clearer folder/file presentation, adjustable panel sizes, a hideable navigation sidebar that collapses when dragged to the left edge, pointer feedback on every clickable control, and popup feedback. They chose confirmation dialogs plus floating result notifications; learning/review hover details remain floating tips. The existing authorization to implement, review, commit and publish without further approval applies.

0.5.0 is already tagged and publishing. Preserve its verification chain, then deliver this work as 0.6.0.

## Layout decision

Three alternatives were considered: polishing the existing vertically stacked folder/list region, merging folders/files into one tree, or separate folder/list/document columns. Use separate columns: this makes boundaries and independent sizing obvious without removing search, filtering, note previews or history.

- Application navigation defaults to 184 logical pixels, with a visible header toggle. It can resize between 144 and 320 pixels. Dragging its divider to the first 36 pixels of the window hides it; the header toggle remains reachable and restores the last expanded width.
- Notes workspace has a folder tree (default 176 pixels), note list (240 pixels) and flexible document area. Folder and note widths have independent dividers, constrained to 144–360 and 180–440 pixels. Preserve at least 280 pixels for the document. Clamp displayed widths to the current window; do not overwrite saved preferences merely because the window is smaller.
- Use continuous pane borders, distinct section headers, folder/Markdown icons, indentation guides, disclosure arrows, compact counts, clear selected rows, and breadcrumb context. Each scroll region is bounded and independent. Long titles truncate without pushing controls away.
- Persist preferred widths and navigation visibility in the existing settings record with backward-compatible defaults. Persist at the end of a drag, not on every pointer move. Do not change learning timer state.

## Drag preview

The drag preview must stay close to the pointer regardless of where the note was grabbed. Inspect GPUI's cursor-offset contract, reproduce a drag from the source row's lower-right corner, then use a compact preview with a fixed nearby grab point. The preview does not affect destination hit testing. All original metadata-only move validation remains.

## Feedback and interaction

- Confirmation prompts become centered modal dialogs with dim backdrop, explicit action/cancel, focus containment and Escape cancellation. Destructive actions still require explicit confirmation of one item.
- Operation success/failure becomes a dismissible floating notification. Success may auto-dismiss; errors remain long enough to read and can be dismissed. Long messages are bounded and scrollable. Notifications must not change page layout or obstruct typing.
- The native WebView is above GPUI visuals. Hide it while a blocking dialog or pane resize is active and restore the same document/generation afterward. Place floating notifications in the header above the native viewport; learning tips stay within the library panes.
- Folder create/rename/move/delete forms use dialogs. Local validation appears inside its associated dialog. Static status and help labels remain ordinary text; they are not transient prompts.
- All enabled clickable buttons, row actions, checkboxes and links use a hand cursor. Disabled controls keep a non-action cursor. Dividers use a horizontal resize cursor and clear hover/drag feedback.

## Verification

Use isolated libraries and a distinct test EXE; never stop a user process. Verify resizing, collapse/restore, restart preferences, long names/deep trees, bottom-right drag alignment, dialog cancel/confirm, floating notice geometry, pointer cursors, and visible native editing after each overlay/resize. Re-run the existing history, storage, draft and sync flows. Retain real desktop captures, GPUI screenshots and reports.

No bulk deletion. Preserve source files, database compatibility, existing sync identifiers and the 0.5.0 tag.
