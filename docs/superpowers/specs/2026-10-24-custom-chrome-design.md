# Custom window chrome and iPhone-inspired hierarchy

The user's new request extends the active desktop polish work. Their earlier instruction to finish autonomously still applies; this design does not add an approval pause.

## Chosen approach

Draw a compact 44 logical pixel title bar inside GPUI. The platform keeps window management and resize hit testing; all title bar pixels and controls are rendered by the app. Reusing the system caption would not satisfy the request. Reimplementing OS window movement would duplicate reliable platform behavior and risk the current drag-performance fix. Use native window-control hit areas behind custom visual controls.

The bar uses the application's semantic surface, text and accent colors. A leading sidebar icon remains discoverable whether navigation is visible or hidden, followed by a concise current-page title (or selected note title). A flexible empty area provides drag space. Trailing content actions are grouped separately from minimize, maximize/restore and close. Window controls retain Windows placement, familiar symbols, tooltips and sufficiently large targets. Keep close routed through the existing settings/draft persistence barrier.

Integrate the existing page heading into this bar, avoiding an additional heading band. Keep the sidebar's compact brand, content actions and all existing test identities where practical. The top bar is outside content scrolling and outside the native Markdown child. Notifications must not cover title-bar window controls or steal their hit areas; dialogs remain centered in the whole window.

## Visual language

Apply Apple HIG principles through aligned groups, progressive disclosure, neutral grouped surfaces, concentric rounded controls, restrained accent usage, visible navigation and short cancellable transitions. Adapt to available desktop space; preserve keyboard/mouse conventions. Use semantic solid/tinted surfaces with a fine separator rather than expensive backdrop effects over the native editor. No claim of native Apple Liquid Glass support or Apple platform conformance.

Maintain light/dark/custom theme contrast, inactive-window readability, reduced-motion support and compact layouts. Preserve 32 px library row height, the scrollbar gutter, durable drafts, save viewport, inline key capture and the existing source-file protections.

## Verification

- Platform chrome disabled and custom title bar rendered in real window screenshots.
- Real native hit-test positions for drag, minimize, maximize and close; correct client region on buttons so actions never start dragging accidentally.
- Actual caption drag and double-click maximize/restore; restored editor geometry remains visible.
- Existing close test persists the final editor input through the custom close button; unsaved settings still prompt before exit.
- Full native suite and window-drag scenario run after chrome integration, with small-window geometry checks. Preserve the earlier matched before/after performance comparison separately.

## References (read 2026-10-24)

- https://developer.apple.com/design/human-interface-guidelines/layout
- https://developer.apple.com/design/human-interface-guidelines/toolbars
- https://developer.apple.com/design/human-interface-guidelines/materials
- https://developer.apple.com/design/human-interface-guidelines/motion
- Existing GPUI TitleBar/WindowControlArea and Windows non-client event implementation.
