# Continuous Markdown editing and note lifecycle

The user rejected 0.3's separate paragraph inputs/previews and reported poor rendering performance, absent language highlighting, no draft discard, and no way to stop tracking notes. Prior authorization permits autonomous design and delivery without another approval gate.

## Editor decision

Use the existing GPUI Kit shell with one offline CodeMirror 6 surface hosted in a Windows WebView2 child window through Wry. CodeMirror keeps Markdown as the document, incrementally parses it, virtualizes its viewport and decorates the actual editable text. Headings, emphasis, lists, quotations, tasks, links, code and tables appear in the continuous document. Syntax delimiters reveal around the selection for precise editing; there is no separate active-paragraph form, duplicate preview, or completion button. A source-mode toggle changes decorations without changing text or undo history.

Alternatives considered: extending GPUI's source editor requires a custom display map/IME/cursor implementation; a ProseMirror rich document would normalize Markdown on serialization. A raw Markdown editor with viewport decorations preserves the existing version and conflict semantics with substantially less editing risk.

Bundle JavaScript, styles, parsers and licenses into the application. No CDN, remote scripts, or network service is used for writing. Code fences highlight Rust, Python, JavaScript/TypeScript, C/C++, C#, Java, Go, HTML, CSS, JSON, YAML, TOML, SQL and shell. Unknown fences remain readable plain code. Untrusted Markdown is never executed as HTML, script or IPC. Restrict WebView navigation, new windows, downloads, permissions and remote resources.

## Host and persistence contract

- Wry construction pumps Windows messages. Construct on GPUI's foreground async task outside App/entity update borrows, using a captured owned HWND wrapper. IPC handlers only enqueue events.
- Embed a fixed/flex-height body using GPUI canvas bounds. CodeMirror owns internal scrolling; hide the child HWND when navigating away or closing the editor. Prevent overlap with GPUI headers/sidebars.
- Each document has a generation and revision. Init carries raw Markdown. Change messages carry bounded transactions with one-based line / UTF-16 column positions and inserted text. Rust applies them to its original text, preserving original line endings outside edits. Ignore stale generations, reject invalid/out-of-order transactions.
- A flush handshake settles composition and confirms the latest revision and normalized document before save, keep-draft, library replacement or close. The canonical Rust text is persisted only after acknowledgement. Input errors retain the document and show an actionable error. Ctrl+S inside WebView triggers the same save path.
- UI history is CodeMirror's transaction history. Native drafts, immutable versions, expected-hash conflicts and per-library storage remain.
- Do not parse or serialize the complete document on every keypress. Coalesce host draft notifications. Measure typing on a large document and bound rendered DOM to the viewport.

Frontend bridge API: `window.studyappEditor.load({generation,text,readonly})`, `setReadonly(bool)`, `setSource(bool)`, `command(name)`, `flush({request})`, `test(action)` when explicitly enabled. Outgoing IPC JSON: `ready`; `change` with generation/baseRevision/revision/changes; `flush` with generation/request/revision/normalizedText; `save`; `error`; test reports. Change records use `{fromLine,fromColumn,toLine,toColumn,text}`, positions relative to the pre-transaction document and applied in reverse order. JS normalizes line endings internally; Rust converts inserted LF using the original preferred EOL, and compares normalized text only at flush.

## Draft deletion

Both the active editor and recovery card expose 删除草稿. One concise in-app confirmation describes loss of unsaved edits. A tokenized serial `Discard` operation writes JSON null and acknowledges completion; it does not delete the Markdown file. Clear UI ownership before resetting the editor so blur or late events cannot recreate the discarded draft. Loading/saving/discarding are guarded. A failure leaves recoverable content and reports the error.

## Stop managing notes

Add `Note.detached` separately from `managed` (which means the application owns the Markdown file). `UnmanageNote` retains the note, file, versions, learning records and reviews, but stops polling, current-interval attribution and sync. Filter normal library/pickers without losing historical titles or existing associations. Explicit reimport of the same path reactivates the same note ID; changed content is captured as an offline version.

Schema 3 must read schema 1/2 and old backups, while older binaries reject schema 3. Restoring a backup does not write detached Markdown. Exclude detached UUIDs from the sync planner's union so remote copies cannot silently reimport them. Reject unmanage while a network sync is busy, and reject late sync application to detached notes.

## Verification

Use independent core/sync lifecycle tests, browser interaction and security tests against the actual bundled editor, native WebView IPC/save/recovery smoke and screenshot capture, and large-document typing/DOM measurements. Test IME, Unicode, CRLF/mixed EOL, cross-paragraph selection, undo/redo, mode switches, HTML-like text, unknown languages, invalid/stale IPC, discard/restart, unmanage/reimport/restart, sync exclusion and historical review integrity. GPUI GPU screenshots alone do not capture a child WebView and are not editor rendering evidence.

Never bulk-delete files or directories. Never terminate the user's currently running application. Publish only after the new implementation and release checks pass.
