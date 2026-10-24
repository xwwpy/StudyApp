# Custom chrome implementation plan

Goal: replace the Windows native caption with app-drawn chrome and keep all window, data and performance guarantees.

1. Add a focused `window_chrome.rs` component: 44 px geometry, shared rounded controls, native window-control areas, semantic theme colors, keyboard/tooltips where supported. Add focused native tests for hit areas and window state.
2. Set transparent/custom caption options in `main.rs` after the matched performance comparison; preserve actual OS title and resize behavior.
3. Integrate the bar in `app.rs`, remove the duplicated content heading row, relocate navigation toggle, and keep overlays in root window coordinates. Keep sidebar footer version and update affordance legible.
4. Finish settings preview caching, readable shortcut labels and keycaps. Keep all save/discard and inline capture semantics.
5. Run native tests for real drag, minimize/restore/maximize, close protection, smaller viewport, WebView visibility and the existing complete workflow. Inspect screenshots in both themes.
6. Review the integrated diff, run final checks, package and publish only when all active scope passes. Record source references and measured limitations.
