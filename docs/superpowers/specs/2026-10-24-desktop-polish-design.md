# Desktop interaction and performance follow-up

The user requests continued autonomous implementation; no review pause is required. This extends the October 23 reliability work and must retain its drafts, image, updater, storage, and scrolling guarantees.

## Approach

Use a restrained native desktop layout with a single accent and reusable controls. A cosmetic-only pass would leave repeated parsing/layout work untouched; replacing the entire native framework would add unrelated risk. Keep GPUI and the persistent CodeMirror document, isolate expensive derived state, and use short state transitions only where they explain a change.

## Required outcomes

- Whole-window move/resize: inspect the current Zed Windows event and draw coordination implementation; measure our move/bounds/frame work before changing it. Do not re-layout/reconfigure an unchanged WebView merely because its parent moved. Preserve cross-monitor DPI, maximize/restore, and native child visibility.
- Settings: fixed compact save header, category rail, bounded content with grouped reusable fields. On narrow windows retain access to all categories. Cache appearance resolution and shortcut analysis until their inputs change. Key recording appears in the selected binding cell with local cancellation and conflict feedback.
- Library: use common row dimensions, neutral selection fills, consistent file/folder/status icon treatment, and reserve a scrollbar gutter. Draft collection selection and a selected document remain distinct; their row backgrounds never overlap. Keep drag/drop, counts, sorting, context menus and source-file protections.
- Save: retain document scroll, selection and undo when committing the same document. Image reference changes map the existing position; success must not reopen the document at line one. Keep the editor frozen through the commit and draft retirement barriers, and unlock on failure.
- Updates: after a newer release is detected, show an unobtrusive action immediately above the sidebar version footer. It opens the existing update page; download/install remain explicit. The state refreshes when checks complete, without polling the network from render.
- Visual language: shared input/field/section/binding/list-row primitives, restrained borders, clear spacing and typography, theme-aware colors, short finite transitions. No permanent animation loops or animation-triggered full settings recomputation.

## Verification

Public-API GPUI regressions for scrolling and submenu regions; library row/gutter bounds; settings draft unchanged until save; inline key capture and conflict behavior; browser/native save-position regression; move/resize counters and timing with real window input; existing full core, sync, desktop, editor and native suites. Real cloud service interoperability is not claimed without an account test.

## References

- https://github.com/zed-industries/zed/tree/main/crates/gpui_windows/src
- https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-entersizemove
- https://learn.microsoft.com/en-us/windows/apps/design/basics/
- https://learn.microsoft.com/en-us/windows/apps/design/motion/
- https://github.com/microsoft/vscode/blob/main/src/vs/workbench/contrib/update/browser/update.ts (global update availability indicator and state-dependent actions)
