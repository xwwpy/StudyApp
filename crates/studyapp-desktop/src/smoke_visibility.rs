//! Actual HWND presentation and Windows input checks. DOM queries and CDP
//! screenshots alone cannot detect a hidden or occluded native child window.
use crate::{
    app::{Page, StudyApp},
    markdown_editor::MarkdownEditor,
};
use gpui_kit::{test::TestWindowExt, *};
use serde_json::Value;
use std::{path::Path, time::Duration};
use windows::Win32::{
    Foundation::{HWND, POINT},
    UI::{
        Input::KeyboardAndMouse::*,
        WindowsAndMessaging::{
            GetForegroundWindow, HWND_NOTOPMOST, HWND_TOPMOST, IsChild, SW_SHOWNORMAL,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetCursorPos, SetForegroundWindow,
            SetWindowPos, ShowWindow, WindowFromPoint,
        },
    },
};

async fn pause(cx: &AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(50))
        .await;
}

async fn snapshot(
    window: AnyWindowHandle,
    editor: &Entity<MarkdownEditor>,
    cx: &mut AsyncApp,
) -> Value {
    window
        .update(cx, |_, _, cx| {
            editor
                .read(cx)
                .test(serde_json::json!({
                    "type": "snapshot", "request": "native-visibility-state"
                }))
                .unwrap();
        })
        .unwrap();
    for _ in 0..100 {
        pause(cx).await;
        if let Some(value) = window
            .update(cx, |_, _, cx| {
                editor.update(cx, |editor, _| {
                    editor.test_results.remove("native-visibility-state")
                })
            })
            .unwrap()
        {
            return value;
        }
    }
    panic!("native visibility snapshot timed out");
}

fn parent(state: &Value) -> HWND {
    HWND(state["parentHwnd"].as_u64().unwrap() as usize as *mut _)
}

fn activate(state: &Value) {
    let hwnd = parent(state);
    // Only the current isolated smoke process's captured parent HWND is used.
    // Hidden startup avoids a console flash; visible testing is explicit here.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .unwrap();
        let _ = SetForegroundWindow(hwnd);
    }
}

pub async fn visible(
    window: AnyWindowHandle,
    editor: &Entity<MarkdownEditor>,
    cx: &mut AsyncApp,
) -> Value {
    for attempt in 0..240 {
        pause(cx).await;
        let state = window
            .update(cx, |_, window, cx| {
                window.render_frame(cx);
                let editor = editor.read(cx);
                assert!(editor.error().is_none(), "{:?}", editor.error());
                editor.ready().then(|| editor.native_state().unwrap())
            })
            .unwrap();
        if let Some(state) = state {
            activate(&state);
            if state["visible"] == true
                && state["controllerVisible"] == true
                && state["insideParent"] == true
                && state["hitIsEditor"] == true
                && state["bounds"]["width"].as_i64().unwrap_or(0) > 200
                && state["bounds"]["height"].as_i64().unwrap_or(0) > 120
            {
                if state["foregroundHwnd"] != state["parentHwnd"] {
                    // Foreground lock can reject SetForegroundWindow for a
                    // background-launched test. A verified click on our own
                    // visible child activates it as a user interaction.
                    click_body(&state);
                    continue;
                }
                return state;
            }
            if attempt == 239 {
                eprintln!("native failed viewport: {state}");
                let path = std::env::current_dir()
                    .unwrap()
                    .join(".scratch/native-first-failure.bmp");
                capture_screen(&state, &path);
                panic!("native editor is not visibly usable: {state}");
            }
        } else {
            assert!(attempt < 239, "editor did not become ready");
        }
    }
    unreachable!()
}

/// Move the OS pointer, so native WM_MOUSEMOVE and delayed GPUI tooltips agree.
pub async fn hover(
    window: AnyWindowHandle,
    editor: &Entity<MarkdownEditor>,
    id: SharedString,
    cx: &mut AsyncApp,
) {
    let state = visible(window, editor, cx).await;
    let center = window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window.find(id).bounds().center()
        })
        .unwrap();
    let scale = state["logicalBounds"][4].as_f64().unwrap() / 96.0;
    let left = state["parentBounds"]["x"].as_i64().unwrap() as i32;
    let top = state["parentBounds"]["y"].as_i64().unwrap() as i32;
    let width = state["parentBounds"]["width"].as_i64().unwrap() as i32;
    let height = state["parentBounds"]["height"].as_i64().unwrap() as i32;
    let x = left + (f64::from(f32::from(center.x)) * scale).round() as i32;
    let y = top + (f64::from(f32::from(center.y)) * scale).round() as i32;
    assert!(
        (left..left + width).contains(&x) && (top..top + height).contains(&y),
        "hover point is outside the test window: ({x}, {y}), {state}"
    );
    let parent = parent(&state);
    let target = unsafe { WindowFromPoint(POINT { x, y }) };
    assert!(
        target == parent || unsafe { IsChild(parent, target).as_bool() },
        "hover point belongs to another window: ({x}, {y}), {state}"
    );
    assert_eq!(unsafe { GetForegroundWindow() }, parent);
    unsafe { SetCursorPos(x, y).unwrap() };
    pause(cx).await;
}

fn send(state: &Value, inputs: &[INPUT]) {
    // Never deliver input to an unrelated foreground application.
    assert_eq!(unsafe { GetForegroundWindow() }, parent(state));
    assert_eq!(
        unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) },
        inputs.len() as u32
    );
}

fn key(code: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: code,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

fn click_body(state: &Value) {
    let x = state["bounds"]["x"].as_i64().unwrap() as i32 + 75;
    let y = state["bounds"]["y"].as_i64().unwrap() as i32 + 105;
    let child = HWND(state["hwnd"].as_u64().unwrap() as usize as *mut _);
    let target = unsafe { WindowFromPoint(POINT { x, y }) };
    assert!(
        target == child || unsafe { IsChild(child, target).as_bool() },
        "mouse target is not our test editor: {state}"
    );
    unsafe { SetCursorPos(x, y).unwrap() };
    let inputs = [MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP].map(|flags| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: flags,
                ..Default::default()
            },
        },
    });
    assert_eq!(
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) },
        inputs.len() as u32
    );
}

fn type_text(state: &Value, text: &str) {
    let inputs: Vec<_> = text
        .encode_utf16()
        .flat_map(|unit| {
            [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP].map(|flags| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wScan: unit,
                        dwFlags: flags,
                        ..Default::default()
                    },
                },
            })
        })
        .collect();
    send(state, &inputs);
}

/// Capture screen pixels of this foreground editor. Unlike CDP or GPUI GPU
/// captures, this includes the native child as the desktop actually displays it.
fn capture_screen(state: &Value, path: &Path) -> usize {
    use windows::Win32::Graphics::Gdi::*;
    let x = state["bounds"]["x"].as_i64().unwrap() as i32;
    let y = state["bounds"]["y"].as_i64().unwrap() as i32;
    let width = state["bounds"]["width"].as_i64().unwrap() as i32;
    let height = state["bounds"]["height"].as_i64().unwrap() as i32;
    let child = HWND(state["hwnd"].as_u64().unwrap() as usize as *mut _);
    let target = unsafe {
        WindowFromPoint(POINT {
            x: x + width / 2,
            y: y + height / 2,
        })
    };
    assert!(
        target == child || unsafe { IsChild(child, target).as_bool() },
        "screenshot target is obscured by another application"
    );
    let length = (width * height * 4) as usize;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // Resources are paired in this scope before the captured bytes leave it.
    let pixels = unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .expect("native screenshot bitmap");
        let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
        let result = BitBlt(memory, 0, 0, width, height, Some(screen), x, y, SRCCOPY);
        let pixels = std::slice::from_raw_parts(bits as *const u8, length).to_vec();
        let _ = SelectObject(memory, previous);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        result.expect("native screen copy");
        pixels
    };
    let mut bmp = Vec::with_capacity(54 + length);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((54 + length) as u32).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&54_u32.to_le_bytes());
    bmp.extend_from_slice(&40_u32.to_le_bytes());
    bmp.extend_from_slice(&width.to_le_bytes());
    bmp.extend_from_slice(&(-height).to_le_bytes());
    bmp.extend_from_slice(&1_u16.to_le_bytes());
    bmp.extend_from_slice(&32_u16.to_le_bytes());
    bmp.extend_from_slice(&[0; 24]);
    bmp.extend_from_slice(&pixels);
    std::fs::write(path, bmp).unwrap();
    // White/transparent-only child regions previously passed CDP checks. The
    // fixture has several visible text lines below the toolbar.
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .skip(width as usize * 65)
        .filter(|pixel| pixel[0] < 180 && pixel[1] < 180 && pixel[2] < 180)
        .count()
}

async fn capture_visible(state: &Value, path: &Path, cx: &AsyncApp) {
    // Readiness only acknowledges JavaScript. Give desktop presentation its own
    // bounded condition wait; no arbitrary sleep can turn blank pixels green.
    for attempt in 0..60 {
        pause(cx).await;
        if capture_screen(state, path) > 200 {
            return;
        }
        assert!(
            attempt < 59,
            "native screenshot has no visible body text: {path:?}"
        );
    }
}

pub async fn capture(
    window: AnyWindowHandle,
    editor: &Entity<MarkdownEditor>,
    path: &Path,
    cx: &mut AsyncApp,
) {
    let state = visible(window, editor, cx).await;
    capture_visible(&state, path, cx).await;
}

pub async fn type_into(
    window: AnyWindowHandle,
    editor: &Entity<MarkdownEditor>,
    text: &str,
    cx: &mut AsyncApp,
) {
    let browser = snapshot(window, editor, cx).await;
    assert_eq!(
        browser["readonly"], false,
        "editor is not editable: {browser}"
    );
    let state = visible(window, editor, cx).await;
    click_body(&state);
    pause(cx).await;
    type_text(&state, text);
    for attempt in 0..80 {
        pause(cx).await;
        if window
            .update(cx, |_, _, cx| {
                let editor = editor.read(cx);
                assert!(editor.error().is_none(), "{:?}", editor.error());
                editor.value().contains(text)
            })
            .unwrap()
        {
            return;
        }
        assert!(
            attempt < 79,
            "native keyboard text did not reach persistence: {text:?}"
        );
    }
}

pub async fn first_open(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    database: &Path,
    cx: &mut AsyncApp,
) {
    let (editor, original) = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.page = Page::Notes;
                app.selected_note = app
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.notes.first().map(|note| note.id.clone()));
                cx.notify();
            });
            window.render_frame(cx);
            let editor = view.read(cx).note_content.clone();
            let original = editor.read(cx).value().to_owned();
            (editor, original)
        })
        .unwrap();
    let state = visible(window, &editor, cx).await;
    std::fs::write(
        database.with_file_name("native-first-reader.json"),
        serde_json::to_string_pretty(&state).unwrap(),
    )
    .unwrap();
    capture_visible(
        &state,
        &database.with_file_name("native-first-reader.bmp"),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| window.click("edit-note", cx))
        .unwrap();
    assert!(
        window
            .update(cx, |_, _, cx| view.read(cx).note_editor.is_some())
            .unwrap(),
        "edit button did not start a draft"
    );
    let browser = snapshot(window, &editor, cx).await;
    assert_eq!(
        browser["readonly"], false,
        "editor still readonly: {browser}"
    );
    let state = visible(window, &editor, cx).await;
    click_body(&state);
    pause(cx).await;
    let focus = window
        .update(cx, |_, _, cx| editor.read(cx).native_state().unwrap())
        .unwrap();
    assert_eq!(
        focus["focused"], true,
        "click did not focus editor: {focus}"
    );
    type_text(&state, "native_keyboard_🧠");
    for attempt in 0..80 {
        pause(cx).await;
        let content = window
            .update(cx, |_, _, cx| editor.read(cx).value().to_owned())
            .unwrap();
        if content.contains("native_keyboard_🧠") {
            break;
        }
        if attempt == 79 {
            capture_screen(&state, &database.with_file_name("native-input-failure.bmp"));
            let browser = snapshot(window, &editor, cx).await;
            panic!(
                "Windows keyboard input did not reach editable content; native={content:?}, browser={browser}"
            );
        }
    }
    capture_visible(
        &state,
        &database.with_file_name("native-first-editor.bmp"),
        cx,
    )
    .await;
    // Native input may form several history groups depending on event timing.
    // Keep exercising the real shortcut until all test-only input is undone.
    // An intermediate half-surrogate may intentionally leave canonical text
    // unchanged until the next group repairs it, so use a bounded overall wait.
    let mut undo_states = Vec::new();
    for attempt in 0..32 {
        let current = window
            .update(cx, |_, _, cx| {
                let editor = editor.read(cx);
                assert!(editor.error().is_none(), "{:?}", editor.error());
                editor.value().to_owned()
            })
            .unwrap();
        if current == original {
            break;
        }
        undo_states.push(current);
        assert!(
            attempt < 31,
            "native undo did not restore original Markdown: expected={original:?}, history={undo_states:?}"
        );
        send(
            &state,
            &[
                key(VK_CONTROL, KEYBD_EVENT_FLAGS::default()),
                key(VK_Z, KEYBD_EVENT_FLAGS::default()),
                key(VK_Z, KEYEVENTF_KEYUP),
                key(VK_CONTROL, KEYEVENTF_KEYUP),
            ],
        );
        pause(cx).await;
        pause(cx).await;
    }
    window
        .update(cx, |_, window, cx| {
            window.click("discard-note-draft", cx);
            window.click("confirm-note-action", cx);
        })
        .unwrap();
    for attempt in 0..80 {
        pause(cx).await;
        if window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                app.note_editor.is_none() && app.draft_discard.is_none()
            })
            .unwrap()
        {
            break;
        }
        assert!(
            attempt < 79,
            "native input check did not finish discarding its test draft"
        );
    }
    let state = visible(window, &editor, cx).await;
    unsafe {
        SetWindowPos(
            parent(&state),
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .unwrap();
    }
    eprintln!("native first-open reading, click focus, keyboard input and undo passed");
}
