# Unified library and interaction design

The user requested autonomous implementation without further design approval. This extends the unreleased 0.6.0 work at `1e48aab`, whose CI and security checks passed. Do not publish the old scope while implementing this request.

## Library

Use one resizable, collapsible library tree beside the document, replacing the independent folder and notes columns. Folders expand inline and contain their notes; uncategorized notes remain under an explicit drop target. Preserve right-click operations, logical-only drag/drop, source-file protection, keyboard accessibility, search, detached view, and learning/review hover dates. Folder selection must not change row size or add stacked selection fills. A flat list loses hierarchy and retaining two columns contradicts the request; the single tree is the chosen approach.

Support name ascending/descending and updated-time newest/oldest sorting, stable ties, and remember the choice in categorized interface preferences. Folder hierarchy remains intact when sorting. Search keeps ancestors of matching notes visible.

Compute status from real version/session identity: newest version referenced by a confirmed learned session is recorded; a later different version is modified/unrecorded (`M`, amber); a first version never recorded is new/unrecorded (`A`, green). Neutral recorded notes do not imply synchronization. Explicit source badges distinguish external tracked source (read-only) from app-managed copy, plus missing/detached states. Color always has text/tooltip meaning.

## Drag responsiveness

Measure existing divider/native-overlay and note/folder drag paths before changing them. Avoid re-reading/re-indexing all version bodies, rebuilding rich note metadata, native layout writes or repainting unchanged overlay pixels on every mouse move. Keep the document visible and its bounds stable while dragging a divider; commit only on release and preserve cancel/capture/focus behavior. Regressions must test measurable redundant operations or timings, not claim smoothness from compilation alone.

## Notifications

Replace the single replaceable result slot with an ID-based queue. Identical messages are independent events and may be visible simultaneously. Show a bounded, centered stack with severity icon, compact title/body, close control and entrance/exit fade/slide. Queue overflow rather than silently dropping messages. Success auto-dismisses; errors have a longer readable lifetime and can be manually dismissed. Hover pauses the timer. Animate a separate view so toast frames do not rebuild the document tree. Confirmations remain centered modal dialogs.

## Markdown language picker

Fenced code blocks expose a language selector like Typora, with plain text and the supported programming-language names/aliases. The control changes only the fence info token, preserves code and unrelated whitespace/newlines, enters normal undo/save history, and immediately updates highlighting. Read-only documents cannot mutate. Unknown tags remain visible and may be replaced. Dropdown interactions must not reposition the editor cursor or cause host shortcut actions.

## Settings

Split settings into Learning, Appearance, Shortcuts, Storage, Sync, and About/Account sections. One settings draft and one persistent save/reset bar cover all sections. Section switching does not discard input or write files; leaving Settings retains the existing save/discard/continue guard.

Shortcut rows have a record-key control. While recording, capture the next valid physical key combination instead of executing commands or editing the document. Modifier-only keys do not complete capture; Escape cancels, clear unbinds, and normal input resumes after capture. Normalize through the existing registry. Detect conflicts live, mark every involved row red and show the conflicting action label, and reject invalid saves. Retain multiple alternative bindings and reset defaults.

## Safety and validation

No bulk deletion, no changes to source file bytes from organization actions, no secret logging, no termination of user processes. Keep Windows WebView2/native constraints and immutable configuration generations. Add focused model/browser/UI regressions, full Rust and browser checks, native real-window interaction evidence, security/license checks, then CI and a verified release download. Preserve all failed evidence and report environment limitations accurately.
