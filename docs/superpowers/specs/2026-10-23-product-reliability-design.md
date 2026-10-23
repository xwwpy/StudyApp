# Product reliability, document styling and updates

User requested autonomous implementation and waived additional reviews. This supersedes the pending 0.6.0 release gate: do not publish until these follow-up requirements pass.

## Decisions

Repair the current GPUI/WebView document architecture with explicit shared components. A separate rich-text document model would risk Markdown round-trip and undo; isolated cosmetic patches would leave the ownership and lifecycle failures intact. Keep one persistent CodeMirror editor, but move document selection, draft persistence, overlay bounds and resolved styles into explicit contracts.

### Document rendering and appearance

Provide readable system/light/dark presets and independently editable application and Markdown styles. Markdown settings include body/headings/link/quote/code/table colors, body and mono fonts, font sizes, line height, reading width, table grid/striping and code border/radius. Validate values and readable foreground/background pairs; custom presets must be editable, saved transactionally, restored on restart and previewed without losing the draft. Avoid arbitrary executable CSS. Table columns share actual widths, have visible cell boundaries and preserve editable source/caret/undo. Nested ordered, unordered, task and continuation lists retain their real depth. Code fences form one coherent block with contextual language control. Screenshot actual rendered samples, not only token objects.

### Draft library

Add a built-in Drafts folder supporting multiple independently persisted notes, including new notes and edits of saved notes. Switching documents flushes the current WebView and waits for durable draft acknowledgement, then opens the selected note/draft; it does not force the user to save or delete another draft. Drafts carry stable IDs, titles, timestamps, note/version identity and image group. Explicit save publishes a note version and retires only that draft; unchanged save is silent. Closing flushes active content and retains all drafts. Individual deletion requires confirmation and retains source notes/history/images. Import legacy note-draft.json without overwriting or deleting it. A failed write keeps the active document and exposes a recoverable friendly error.

### Storage and cloud

Use one directory field/picker component for images, synchronization, library selection and migration, with consistent path formatting and stale-callback/library guards. Cloud/Both selection expands real configuration: provider, endpoint, bucket/region where applicable, prefix and account fields. Support WebDAV, local folder/NAS and generic object storage (S3-compatible); Alibaba OSS is an example of this category, not a vendor-specific product option. Expose addressing/protocol compatibility where services differ. Retain DPAPI-protected secrets separately from ordinary JSON. Connection tests run in background against the current draft settings. No silent use of an unrelated hidden configuration; invalid cloud configuration blocks save with inline actionable feedback. Follow official signing and conditional-write semantics and test protocol fixtures; do not claim real account tests without credentials.

Storage management shows usage by database/history, managed notes, images, drafts and auxiliary data, with per-item status and safe individual actions. A cleaned or absent managed copy is a normal state, not a raw OS error. Never delete tracked originals, unknown paths or whole directories. Preserve compare-and-swap and path-safety checks; friendly UI errors retain developer diagnostics separately.

### Shared interactions

Result notices appear near the window top, horizontally centered and sized to content, with no opaque rectangular cutout below the card. Remove separate image-progress success notices and suppress no-change save notices. Keep meaningful failures, duplicate queueing and dismissal animation. Modal confirmations remain centered. Opening a menu keeps the document visible; only actual popup/tooltip bounds occlude the native child, with matching rounded geometry and cleanup. Hover details follow the pointer and clamp to window edges through one reusable surface. Folder and note drag cards share the source row height. Use neutral controls for ordinary actions, a restrained accent for the principal action, and consistent danger styling.

Image paste must handle real Windows image clipboard data as well as browser File/ClipboardEvent data, exactly once, with original insertion position/generation checks and the existing bounded media pipeline.

Audit all scrollable surfaces: cards retain their complete header/footer structure while their bounded content scrolls, visible scrollbars accompany overflowing content, and scroll clipping follows the actual content viewport. Apply a common scroll/card component where practical; nested regions contain wheel events only over their own content, retaining accessible outer-page navigation.

### In-app updates

Check the official xwwpy/StudyApp latest stable release in background; display current/new version, release notes and actionable errors. Download only the expected Windows x64 asset over HTTPS, enforce size limits, verify both release SHA256 asset and GitHub asset digest, reject archive traversal/links/duplicates. Use current_exe to identify the installation directory, preserving an existing executable filename and --data-dir selection.

Only explicit user click starts update. Before restart, persist every draft and resolve dirty settings with the existing save/discard flow. A separate staged helper waits for normal parent exit, verifies expected old executable identity, stages a backup and replaces only the packaged executable and explicitly owned distribution files. Never remove unrelated files or user data. On failure restore the old executable and leave a readable status; on success restart the new program. Non-writable installations show a manual-install path, with no silent elevation. Develop and test the updater entirely in isolated fixture directories, never update the user's running app during this task.

## Global constraints

- Preserve all user data, external tracked files, history, existing .scratch evidence and unrelated changes.
- No bulk deletion or recursive deletion; delete only one explicitly selected file when required.
- Keep work on codex/studyapp-local; no release tag before fresh verification.
- Never expose credentials, low-level OS errors or internal paths as primary user-facing failures.
- Local native keyboard fallback remains explicitly labelled; CI retains strict WIN32_SENDINPUT.
- Do not weaken concurrency, exact-text, native-pixel, input-focus or source-preservation assertions to pass tests.

## References

- VS Code hot exit: https://code.visualstudio.com/docs/editing/codebasics
- Typora image insertion and storage: https://support.typora.io/Images/
- Typora themes: https://support.typora.io/About-Themes/
- OSS REST and S3 compatibility: https://www.alibabacloud.com/help/en/oss/developer-reference/compatibility-with-amazon-s3
- GitHub release metadata and asset digest: https://docs.github.com/en/rest/releases/releases

These references inform the workflows; no claim of identical behavior or third-party service certification is made.
