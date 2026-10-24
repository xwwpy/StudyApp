# Desktop polish implementation plan

1. Finish the current clipboard, draft lifecycle, object-storage and menu regressions. Preserve exact test evidence and all existing user data.
2. Diagnose native window moving using current Zed source and local counters; implement only evidenced hot-path fixes and verify DPI/resize behavior.
3. Preserve document state across same-note save, including reference adoption, with browser/native regression coverage.
4. Introduce reusable grouped settings/input and library row styling. Split settings navigation from its fixed save header; make key capture inline and cache binding analysis and appearance resolution.
5. Correct library scrollbar gutter, draft collection/document selection states and note source/status indicators. Add finite theme-aware feedback transitions.
6. Subscribe the shell to update availability and show a matching footer action opening update settings. Keep explicit user-driven download and restart.
7. Run targeted tests, then formatting, strict Clippy, workspace-equivalent Rust suites, complete browser tests, native clipboard/full smoke and portable package startup. Review, commit/push and verify GitHub CI before publishing a release. Record any external limitations explicitly.
