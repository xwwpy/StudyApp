# Windows position-only invalidation patch

This directory contains the published `gpui-pre` 0.3.9 source under Apache-2.0.
The original `LICENSE-APACHE`, normalized manifest, original manifest, build
script, resources, examples, tests and documentation are retained.

- Archive: https://static.crates.io/crates/gpui-pre/gpui-pre-0.3.9.crate
- Archive SHA-256: `34f61c1cfd36fc9c436af09308c963822ae27a45a84feb21a77ba5c9e051f1c0`
- Upstream: https://github.com/zed-industries/zed
- Packaged upstream snapshot: `93d745668805029476e6d7fbb19652116fed51de`

## Change

Windows `on_moved` keeps immediately refreshing the platform-derived viewport,
scale factor and client mouse position and notifying every bounds observer.
It invalidates the client scene only when one of those client values changes.
Moving the already-rendered client on the desktop is handled by Windows/DWM.
Observers remain free to invalidate their own content or call `refresh()`.

The resize callback still uses the original unconditional `bounds_changed`
path, including same-size restore notifications. Other platforms retain their
original `on_moved` behavior. The Windows timer, runnable dispatch, native child
composition, DPI handlers and observer ordering remain unchanged.

`src/platform/test/window.rs` adds `simulate_move`; `src/app/test_context.rs`
exposes scoped movement and frame-request simulation for the focused public-API
regression in `crates/studyapp-desktop/src/smoke_window_drag.rs`. It counts real
view renders and verifies that pure moves deliver every observer notification
without rebuilding the client scene, observer-requested refresh remains effective,
and changed client mouse position, DPI and same-size resize still invalidate.
This regression runs with the normal workspace `--all-features` tests.
StudyApp's `window-drag` native smoke scenario also drives an actual caption
drag on Today, Notes and Settings and checks WebView bounds/visibility/regions.

## References

- https://github.com/zed-industries/zed/blob/main/crates/gpui/src/window.rs
- https://github.com/zed-industries/zed/blob/main/crates/gpui_windows/src/events.rs
- https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-entersizemove

Re-evaluate this narrow patch when upgrading GPUI. Do not replace it with
skipping or delaying the native moved callback: anchored overlays and other
bounds observers require prompt updates.
