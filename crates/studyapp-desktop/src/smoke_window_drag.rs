//! Whole-window movement through the real Win32 caption modal loop.
//! All input targets the isolated smoke process; no user library is touched.
use crate::app::{Page, StudyApp};
use gpui_kit::{test::TestWindowExt, *};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEINPUT,
            SendInput,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetWindowRect, HTCAPTION, HTMAXBUTTON, HTMINBUTTON,
            HWND_NOTOPMOST, IsIconic, IsZoomed, SW_RESTORE, SWP_NOACTIVATE, SWP_NOSIZE,
            SWP_NOZORDER, SendMessageW, SetCursorPos, SetWindowPos, ShowWindow, WM_NCHITTEST,
        },
    },
};

async fn settle(cx: &AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(250))
        .await;
}

fn mouse_button(down: bool) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: if down {
                    MOUSEEVENTF_LEFTDOWN
                } else {
                    MOUSEEVENTF_LEFTUP
                },
                ..Default::default()
            },
        },
    };
    assert_eq!(
        unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) },
        1
    );
}

fn control_point(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    id: &str,
    cx: &mut AsyncApp,
) -> (usize, i32, i32) {
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let center = window
                .find(SharedString::from(id.to_owned()))
                .bounds()
                .center();
            let native = view.read(cx).note_content.read(cx).native_state().unwrap();
            let scale = native["logicalBounds"][4].as_f64().unwrap() / 96.;
            (
                native["parentHwnd"].as_u64().unwrap() as usize,
                native["parentBounds"]["x"].as_i64().unwrap() as i32
                    + (f64::from(f32::from(center.x)) * scale).round() as i32,
                native["parentBounds"]["y"].as_i64().unwrap() as i32
                    + (f64::from(f32::from(center.y)) * scale).round() as i32,
            )
        })
        .unwrap()
}

fn assert_hit(raw: usize, x: i32, y: i32, expected: u32) {
    let hwnd = HWND(raw as *mut _);
    assert_eq!(
        unsafe { GetForegroundWindow() },
        hwnd,
        "native input must target the isolated foreground window"
    );
    let hit = unsafe {
        SendMessageW(
            hwnd,
            WM_NCHITTEST,
            None,
            Some(LPARAM(((y as u32) << 16 | (x as u32 & 0xffff)) as isize)),
        )
    };
    assert_eq!(
        hit.0, expected as isize,
        "custom titlebar has the wrong native hit region"
    );
}

/// CI native input helper; full-smoke close uses the actual custom Close region.
/// Coordinates come from the current GPUI layout, including maximized windows.
pub async fn click_window_control(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    id: &str,
    expected_hit: u32,
    cx: &mut AsyncApp,
) {
    let (raw, x, y) = control_point(window, view, id, cx);
    assert_eq!(unsafe { GetForegroundWindow() }, HWND(raw as *mut _));
    unsafe {
        SetCursorPos(x, y).unwrap();
    }
    // GPUI's native hit-test callback consults the rendered mouse hitboxes.
    // Deliver the real hover before verifying the corresponding Win32 region.
    settle(cx).await;
    assert_hit(raw, x, y, expected_hit);
    mouse_button(true);
    mouse_button(false);
}

fn native_drag(raw: usize, x: i32, y: i32) -> Value {
    let hwnd = HWND(raw as *mut _);
    let mut rect = RECT::default();
    unsafe {
        GetWindowRect(hwnd, &mut rect).unwrap();
    }
    assert_eq!(unsafe { GetForegroundWindow() }, hwnd);
    unsafe {
        SetCursorPos(x, y).unwrap();
    }
    std::thread::sleep(Duration::from_millis(80));
    assert_hit(raw, x, y, HTCAPTION);
    mouse_button(true);
    // Always release our input when assertions fail, before reporting the failure.
    struct Release;
    impl Drop for Release {
        fn drop(&mut self) {
            mouse_button(false);
        }
    }
    let release = Release;
    std::thread::sleep(Duration::from_millis(50));
    let began = Instant::now();
    let mut positions = Vec::new();
    for step in 0..80_i32 {
        assert_eq!(
            unsafe { GetForegroundWindow() },
            hwnd,
            "drag lost isolated foreground"
        );
        let offset = if step < 40 { step * 3 } else { (79 - step) * 3 };
        unsafe {
            SetCursorPos(x + offset, y + (offset / 4)).unwrap();
        }
        std::thread::sleep(Duration::from_millis(16));
        let mut current = RECT::default();
        unsafe {
            GetWindowRect(hwnd, &mut current).unwrap();
        }
        positions.push((began.elapsed().as_micros(), current.left, current.top));
    }
    drop(release);
    let changed = positions
        .windows(2)
        .filter(|pair| pair[0].1 != pair[1].1 || pair[0].2 != pair[1].2)
        .count();
    assert!(
        changed >= 30,
        "actual OS window movement stalled: only {changed} positions"
    );
    let mut change_gaps = Vec::new();
    let mut last_change = positions[0].0;
    for pair in positions.windows(2) {
        if pair[0].1 != pair[1].1 || pair[0].2 != pair[1].2 {
            change_gaps.push(pair[1].0 - last_change);
            last_change = pair[1].0;
        }
    }
    change_gaps.sort_unstable();
    json!({"inputSamples": positions.len(), "changedPositions": changed,
        "motionP95GapMicros": change_gaps[(change_gaps.len() - 1) * 95 / 100],
        "motionMaxGapMicros": change_gaps.last(),
        "elapsedMicros": began.elapsed().as_micros(), "positions": positions,
        "original": [rect.left, rect.top]})
}

pub async fn exercise(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    database: &Path,
    cx: &mut AsyncApp,
) {
    crate::smoke_library::until(
        window,
        view,
        "window drag fixture loaded",
        |app, _| {
            app.snapshot
                .as_ref()
                .is_some_and(|snapshot| !snapshot.notes.is_empty())
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                app.page = Page::Notes;
                app.selected_note = app
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.notes.first().map(|note| note.id.clone()));
                cx.notify();
            })
        })
        .unwrap();
    let editor = window
        .update(cx, |_, _, cx| view.read(cx).note_content.clone())
        .unwrap();
    let visible = crate::smoke_visibility::visible(window, &editor, cx).await;
    let raw = visible["parentHwnd"].as_u64().unwrap() as usize;
    unsafe {
        let _ = ShowWindow(HWND(raw as *mut _), SW_RESTORE);
    }
    settle(cx).await;
    unsafe {
        SetWindowPos(
            HWND(raw as *mut _),
            Some(HWND_NOTOPMOST),
            30,
            30,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .unwrap();
    }
    assert!(
        !unsafe { IsZoomed(HWND(raw as *mut _)) }.as_bool(),
        "drag fixture must start restored"
    );
    let mut results = Vec::new();
    for (page, name) in [
        (Page::Today, "today"),
        (Page::Notes, "notes"),
        (Page::Settings, "settings"),
    ] {
        window
            .update(cx, |_, _, cx| {
                view.update(cx, |app, cx| {
                    app.page = page;
                    cx.notify();
                })
            })
            .unwrap();
        settle(cx).await;
        let before = window
            .update(cx, |_, _, cx| editor.read(cx).native_state().unwrap())
            .unwrap();
        let (_, x, y) = control_point(window, view, "window-title-drag", cx);
        let mut result = cx
            .background_executor()
            .spawn(async move { native_drag(raw, x, y) })
            .await;
        settle(cx).await;
        let after = window
            .update(cx, |_, _, cx| editor.read(cx).native_state().unwrap())
            .unwrap();
        for key in ["boundsUpdates", "visibilityUpdates", "occlusionUpdates"] {
            assert_eq!(
                after[key], before[key],
                "same-DPI caption drag changed editor {key}"
            );
        }
        result["page"] = json!(name);
        result["nativeBoundsWrites"] = json!(0);
        result["nativeVisibilityWrites"] = json!(0);
        result["nativeOcclusionWrites"] = json!(0);
        results.push(result);
        unsafe {
            SetWindowPos(
                HWND(raw as *mut _),
                None,
                30,
                30,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
            )
            .unwrap();
        }
        settle(cx).await;
    }
    std::fs::write(
        database.with_file_name("window-drag-performance.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
    exercise_controls(window, view, raw, cx).await;
    std::fs::write(database.with_file_name("window-controls-result.txt"), "PASS: custom titlebar exposes native Min/Max/Drag hit regions; actual minimize/restore, maximize/restore and caption double-click work. Close is covered by full smoke's persisted-draft shutdown.\n").unwrap();
    std::fs::write(database.with_file_name("window-drag-result.txt"), "PASS: real caption drag moved the isolated native window on Today, Notes and Settings; same-DPI movement preserved WebView bounds, visibility and occlusion regions. Timing is UI-thread elapsed time, not sampled CPU or input-to-photon latency.\n").unwrap();
}

async fn expect_native_state(raw: usize, maximized: bool, minimized: bool, cx: &AsyncApp) {
    for _ in 0..100 {
        if unsafe {
            IsZoomed(HWND(raw as *mut _)).as_bool() == maximized
                && IsIconic(HWND(raw as *mut _)).as_bool() == minimized
        } {
            return;
        }
        cx.background_executor()
            .timer(Duration::from_millis(25))
            .await;
    }
    panic!("custom titlebar did not reach expected native window state");
}

async fn exercise_controls(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    raw: usize,
    cx: &mut AsyncApp,
) {
    click_window_control(window, view, "window-maximize", HTMAXBUTTON, cx).await;
    expect_native_state(raw, true, false, cx).await;
    settle(cx).await;
    click_window_control(window, view, "window-maximize", HTMAXBUTTON, cx).await;
    expect_native_state(raw, false, false, cx).await;
    settle(cx).await;
    click_window_control(window, view, "window-minimize", HTMINBUTTON, cx).await;
    expect_native_state(raw, false, true, cx).await;
    unsafe {
        let _ = ShowWindow(HWND(raw as *mut _), SW_RESTORE);
    }
    expect_native_state(raw, false, false, cx).await;
    settle(cx).await;
    for maximized in [true, false] {
        let (raw, x, y) = control_point(window, view, "window-title-drag", cx);
        assert_eq!(unsafe { GetForegroundWindow() }, HWND(raw as *mut _));
        unsafe {
            SetCursorPos(x, y).unwrap();
        }
        settle(cx).await;
        assert_hit(raw, x, y, HTCAPTION);
        mouse_button(true);
        mouse_button(false);
        cx.background_executor()
            .timer(Duration::from_millis(60))
            .await;
        mouse_button(true);
        mouse_button(false);
        expect_native_state(raw, maximized, false, cx).await;
        settle(cx).await;
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{
        Context, IntoElement, Modifiers, MouseMoveEvent, ParentElement, Pixels, PlatformInput,
        Point, Render, RequestFrameOptions, Subscription, TestAppContext, Window, div, point, px,
    };
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    struct MoveProbe {
        renders: Rc<Cell<usize>>,
        observed: Rc<RefCell<Vec<Point<Pixels>>>>,
        invalidate_on_move: bool,
        _subscription: Subscription,
    }

    impl Render for MoveProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            div().child("position-only regression")
        }
    }

    #[gpui_kit::test]
    fn position_only_moves_notify_observers_without_rendering(cx: &mut TestAppContext) {
        let renders = Rc::new(Cell::new(0));
        let observed = Rc::new(RefCell::new(Vec::new()));
        let handle = cx.add_window({
            let renders = renders.clone();
            let observed = observed.clone();
            move |window, cx: &mut Context<MoveProbe>| {
                let subscription = cx.observe_window_bounds(window, |probe, window, cx| {
                    probe.observed.borrow_mut().push(window.bounds().origin);
                    if probe.invalidate_on_move {
                        cx.notify();
                    }
                });
                MoveProbe {
                    renders,
                    observed,
                    invalidate_on_move: false,
                    _subscription: subscription,
                }
            }
        });
        cx.simulate_window_frame_request(handle.into(), RequestFrameOptions::default());
        let initial_renders = renders.get();
        assert!(initial_renders > 0);
        for offset in 1..=5 {
            cx.simulate_window_move(handle.into(), point(px(offset as f32 * 10.), px(20.)));
            cx.simulate_window_frame_request(handle.into(), RequestFrameOptions::default());
        }
        assert_eq!(
            observed.borrow().len(),
            5,
            "bounds observers lost movement events"
        );
        assert_eq!(
            observed.borrow().last().copied(),
            Some(point(px(50.), px(20.)))
        );
        assert_eq!(
            renders.get(),
            initial_renders,
            "pure desktop movement rebuilt the client scene"
        );

        // Anchored overlays and any other bounds observer can still invalidate.
        handle
            .update(cx, |probe, _, _| probe.invalidate_on_move = true)
            .unwrap();
        cx.simulate_window_move(handle.into(), point(px(60.), px(20.)));
        cx.simulate_window_frame_request(handle.into(), RequestFrameOptions::default());
        assert_eq!(
            renders.get(),
            initial_renders + 1,
            "observer cx.notify was suppressed"
        );
        handle
            .update(cx, |probe, _, _| probe.invalidate_on_move = false)
            .unwrap();

        // Even a same-size native restore must retain the old resize behavior.
        let viewport = handle
            .update(cx, |_, window, _| window.viewport_size())
            .unwrap();
        cx.simulate_window_resize(handle.into(), viewport);
        cx.simulate_window_frame_request(handle.into(), RequestFrameOptions::default());
        assert_eq!(renders.get(), initial_renders + 2);
        cx.simulate_window_scale_factor_change(handle.into(), 2.);
        cx.simulate_window_frame_request(handle.into(), RequestFrameOptions::default());
        assert_eq!(renders.get(), initial_renders + 3);
        handle
            .update(cx, |_, window, _| assert_eq!(window.scale_factor(), 2.))
            .unwrap();

        // Programmatic movement under a stationary pointer must update hover.
        handle
            .update(cx, |_, window, cx| {
                window.dispatch_event(
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position: point(px(15.), px(20.)),
                        pressed_button: None,
                        modifiers: Modifiers::default(),
                    }),
                    cx,
                );
            })
            .unwrap();
        cx.simulate_window_frame_request(handle.into(), RequestFrameOptions::default());
        let before_mouse_move = renders.get();
        cx.simulate_window_move(handle.into(), point(px(70.), px(20.)));
        cx.simulate_window_frame_request(handle.into(), RequestFrameOptions::default());
        assert_eq!(
            renders.get(),
            before_mouse_move + 1,
            "client cursor change did not refresh hover state"
        );
    }
}
