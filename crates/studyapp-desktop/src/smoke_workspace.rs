//! Native workspace regressions. Every mutation targets the isolated smoke library.
use crate::{
    app::{Page, StudyApp},
    folders_ui::FolderFilter,
    markdown_editor::MarkdownEditor,
    smoke_library::{create_note, settled, until},
};
use gpui_kit::{test::TestWindowExt, *};
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::Gdi::{GetDC, GetPixel, ReleaseDC},
        UI::{
            Input::KeyboardAndMouse::{
                GetCapture, GetFocus, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN,
                MOUSEEVENTF_LEFTUP, MOUSEINPUT, SendInput,
            },
            WindowsAndMessaging::{
                FindWindowExW, GW_OWNER, GWL_EXSTYLE, GetForegroundWindow, GetWindow,
                GetWindowLongW, GetWindowRect, HWND_NOTOPMOST, IsChild, IsWindowVisible,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetCursorPos, SetWindowPos, WS_EX_LAYERED,
                WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
                WindowFromPoint,
            },
        },
    },
    core::w,
};

fn parent_hwnd(native: &Value) -> HWND {
    HWND(native["parentHwnd"].as_u64().unwrap() as usize as *mut _)
}

/// Only target the foreground isolated smoke HWND. Divider capture and guide
/// presentation must be exercised through Win32, not fabricated GPUI capture.
fn native_pointer(native: &Value, position: Point<Pixels>, button: Option<bool>) {
    let parent = parent_hwnd(native);
    let scale = native["logicalBounds"][4].as_f64().unwrap() / 96.;
    let x = native["parentBounds"]["x"].as_i64().unwrap() as i32
        + (f64::from(f32::from(position.x)) * scale).round() as i32;
    let y = native["parentBounds"]["y"].as_i64().unwrap() as i32
        + (f64::from(f32::from(position.y)) * scale).round() as i32;
    unsafe {
        assert_eq!(GetForegroundWindow(), parent, "native drag lost foreground");
        let hit = WindowFromPoint(POINT { x, y });
        assert!(
            hit == parent || IsChild(parent, hit).as_bool() || GetCapture() == parent,
            "native drag must target only the isolated app"
        );
        SetCursorPos(x, y).unwrap();
        if let Some(down) = button {
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
            assert_eq!(SendInput(&[input], std::mem::size_of::<INPUT>() as i32), 1);
        }
    }
}

fn owned_guide(native: &Value) -> Option<HWND> {
    let parent = parent_hwnd(native);
    let mut after = None;
    while let Ok(hwnd) = unsafe { FindWindowExW(None, after, w!("WWStudyResizeGuide"), None) } {
        if unsafe { GetWindow(hwnd, GW_OWNER) }.is_ok_and(|owner| owner == parent) {
            return Some(hwnd);
        }
        after = Some(hwnd);
    }
    None
}

fn assert_guide(native: &Value, expected: bool) {
    let guide = owned_guide(native);
    if !expected {
        assert!(
            guide.is_none(),
            "released/cancelled drag retained an owned guide HWND"
        );
        return;
    }
    let guide = guide.expect("drag must create an owned native dashed guide");
    let parent = parent_hwnd(native);
    unsafe {
        assert!(IsWindowVisible(guide).as_bool(), "guide HWND is hidden");
        assert_eq!(GetCapture(), parent, "guide must retain parent capture");
        assert_eq!(GetForegroundWindow(), parent, "guide stole foreground");
        assert_ne!(GetFocus(), guide, "guide stole keyboard focus");
        let style = GetWindowLongW(guide, GWL_EXSTYLE) as u32;
        for flag in [
            WS_EX_LAYERED,
            WS_EX_TRANSPARENT,
            WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW,
        ] {
            assert_ne!(style & flag.0, 0, "guide missing required extended style");
        }
        assert_eq!(style & WS_EX_TOPMOST.0, 0, "guide is globally topmost");
        let mut bounds = RECT::default();
        GetWindowRect(guide, &mut bounds).unwrap();
        assert!(bounds.right > bounds.left && bounds.right - bounds.left <= 8);
        assert!(bounds.bottom - bounds.top > 200);
        // Read screen pixels, so a logical guide behind the native WebView is
        // not mistaken for a visible line. Both dash and transparent gap occur.
        let dc = GetDC(None);
        let x = (bounds.left + bounds.right) / 2;
        let sample_y = (native["bounds"]["y"].as_i64().unwrap() as i32 + 40)
            .max(bounds.top + 30)
            .min(bounds.bottom - 100);
        let mut dashes = 0;
        let mut gaps = 0;
        for offset in 0..70 {
            let pixel = GetPixel(dc, x, sample_y + offset);
            if pixel.0 == 0x00557B37 {
                dashes += 1;
            } else {
                gaps += 1;
            }
        }
        ReleaseDC(None, dc);
        assert!(
            dashes > 8 && gaps > 8,
            "guide must paint visible dashes and gaps: {dashes}/{gaps}"
        );
    }
}

async fn native_tick(cx: &AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(70))
        .await;
}

fn pointer(window: &mut Window, position: Point<Pixels>, pressed: bool, cx: &mut App) {
    window.dispatch_event(
        MouseMoveEvent {
            position,
            pressed_button: pressed.then_some(MouseButton::Left),
            ..Default::default()
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
}

fn press(window: &mut Window, position: Point<Pixels>, cx: &mut App) {
    pointer(window, position, false, cx);
    window.dispatch_event(
        MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
}

fn release(window: &mut Window, position: Point<Pixels>, cx: &mut App) {
    window.dispatch_event(
        MouseUpEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
}

fn hidden(editor: &Entity<MarkdownEditor>, cx: &App) {
    assert_eq!(
        editor.read(cx).native_state().unwrap()["controllerVisible"],
        false,
        "blocking workspace interaction must hide the native child"
    );
}

async fn browser(
    window: AnyWindowHandle,
    editor: &Entity<MarkdownEditor>,
    action: Value,
    cx: &mut AsyncApp,
) -> Value {
    const REQUEST: &str = "workspace-regression";
    let mut action = action;
    action["request"] = json!(REQUEST);
    window
        .update(cx, |_, _, cx| editor.read(cx).test(action).unwrap())
        .unwrap();
    let started = Instant::now();
    loop {
        cx.background_executor()
            .timer(Duration::from_millis(25))
            .await;
        if let Some(result) = window
            .update(cx, |_, _, cx| {
                editor.update(cx, |editor, _| editor.test_results.remove(REQUEST))
            })
            .unwrap()
        {
            return result;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "workspace browser response timed out"
        );
    }
}

async fn unchanged(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    editor: &Entity<MarkdownEditor>,
    baseline: &Value,
    reader_key: &str,
    cx: &mut AsyncApp,
) {
    crate::smoke_visibility::visible(window, editor, cx).await;
    let current = browser(window, editor, json!({"type":"snapshot"}), cx).await;
    for field in ["generation", "revision", "text", "selection"] {
        assert_eq!(
            current[field], baseline[field],
            "workspace interaction reloaded/changed {field}"
        );
    }
    window
        .update(cx, |_, _, cx| {
            assert_eq!(view.read(cx).reader_key, reader_key)
        })
        .unwrap();
}

async fn resize_pane(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    editor: &Entity<MarkdownEditor>,
    divider: &str,
    pane: &str,
    delta: f32,
    cx: &mut AsyncApp,
) {
    let native = crate::smoke_visibility::visible(window, editor, cx).await;
    // Visibility setup raises the isolated test app. Remove that test-only
    // topmost flag before checking that its owned guide is not topmost.
    unsafe {
        SetWindowPos(
            parent_hwnd(&native),
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .unwrap();
    }
    let (before, start, preferred, saved) = window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let before = window
                .find(SharedString::from(pane.to_owned()))
                .bounds()
                .size
                .width;
            let start = window
                .find(SharedString::from(divider.to_owned()))
                .bounds()
                .center();
            let saved = view
                .read(cx)
                .snapshot
                .as_ref()
                .unwrap()
                .settings
                .layout
                .clone();
            (before, start, view.read(cx).layout.clone(), saved)
        })
        .unwrap();
    native_pointer(&native, start, Some(true));
    native_tick(cx).await;
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                view.read(cx).is_layout_resizing(),
                "divider did not capture pointer"
            );
            assert_guide(&native, true);
        })
        .unwrap();
    let end = start + point(px(delta), px(0.));
    native_pointer(&native, end, None);
    native_tick(cx).await;
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let during = window
                .find(SharedString::from(pane.to_owned()))
                .bounds()
                .size
                .width;
            assert_eq!(during, before, "{pane} resized before release");
            assert_eq!(
                view.read(cx).layout,
                preferred,
                "candidate mutated preferred layout"
            );
            let current = editor.read(cx).native_state().unwrap();
            assert_eq!(current["visible"], true, "drag hid the WebView HWND");
            assert_eq!(
                current["controllerVisible"], true,
                "drag hid the WebView controller"
            );
            assert_eq!(
                current["bounds"], native["bounds"],
                "drag moved the live editor before release"
            );
            assert_guide(&current, true);
            assert_eq!(
                view.read(cx).snapshot.as_ref().unwrap().settings.layout,
                saved,
                "pointer move persisted layout before release"
            );
        })
        .unwrap();
    native_pointer(&native, end, Some(false));
    native_tick(cx).await;
    let expected = window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(
                !view.read(cx).is_layout_resizing(),
                "divider retained capture after release"
            );
            assert_guide(&native, false);
            let after = window
                .find(SharedString::from(pane.to_owned()))
                .bounds()
                .size
                .width;
            let committed = &view.read(cx).layout;
            let preferred_width = match pane {
                "navigation-pane" => committed.navigation_width,
                "folders-pane" => committed.folders_width,
                "notes-pane" => committed.notes_width,
                _ => unreachable!("unknown resize fixture pane"),
            };
            let requested = (f32::from(before) + delta).round();
            assert_eq!(
                preferred_width, requested as u32,
                "{pane} candidate was not committed on release"
            );
            // Hosted Windows desktops can cap the window at 1024 px despite
            // resize(1440). The saved preference must still commit exactly;
            // the rendered note pane must leave 280 px for the document.
            let rendered = if pane == "notes-pane" {
                let navigation = window.find("navigation-pane").bounds().size.width;
                let folders = window.find("folders-pane").bounds().size.width;
                requested.min(
                    f32::from(window.viewport_size().width - navigation - folders)
                        - 32.
                        - 3. * 6.
                        - 280.,
                )
            } else {
                requested
            };
            assert!(
                (f32::from(after) - rendered).abs() < 2.,
                "{pane} rendered {after:?}, expected {rendered} from saved width {preferred_width}"
            );
            assert!(window.find("document-pane").bounds().size.width >= px(279.));
            view.read(cx).layout.clone()
        })
        .unwrap();
    until(
        window,
        view,
        "released divider persisted",
        |app, _| !app.layout_dirty && app.snapshot.as_ref().unwrap().settings.layout == expected,
        cx,
    )
    .await;
}

async fn cancel_resize(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    editor: &Entity<MarkdownEditor>,
    cx: &mut AsyncApp,
) {
    let native = crate::smoke_visibility::visible(window, editor, cx).await;
    unsafe {
        SetWindowPos(
            parent_hwnd(&native),
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .unwrap();
    }
    let (start, layout, revision) = window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            (
                window.find("resize-notes").bounds().center(),
                view.read(cx).layout.clone(),
                view.read(cx)
                    .preferences_ui
                    .saved
                    .as_ref()
                    .unwrap()
                    .revision
                    .clone(),
            )
        })
        .unwrap();
    native_pointer(&native, start, Some(true));
    native_tick(cx).await;
    let end = start + point(px(45.), px(0.));
    native_pointer(&native, end, None);
    native_tick(cx).await;
    window
        .update(cx, |_, window, cx| {
            assert!(view.read(cx).is_layout_resizing());
            assert_guide(&native, true);
            window.press("escape", cx);
            window.render_frame(cx);
            assert!(
                !view.read(cx).is_layout_resizing(),
                "Escape failed to cancel divider candidate"
            );
            assert_eq!(view.read(cx).layout, layout);
            assert!(!view.read(cx).layout_dirty, "cancel queued a layout save");
            assert_guide(&native, false);
        })
        .unwrap();
    // Physical release after Escape must not commit the cancelled candidate.
    native_pointer(&native, end, Some(false));
    native_tick(cx).await;
    window
        .update(cx, |_, _, cx| {
            assert_eq!(view.read(cx).layout, layout);
            assert_eq!(
                view.read(cx)
                    .preferences_ui
                    .saved
                    .as_ref()
                    .unwrap()
                    .revision,
                revision
            );
            let current = editor.read(cx).native_state().unwrap();
            assert_eq!(current["controllerVisible"], true);
            assert_eq!(current["bounds"], native["bounds"]);
            assert_guide(&native, false);
        })
        .unwrap();
    // The persistent WebView can retain keyboard focus during native capture.
    // Exercise that separate Escape IPC path with an explicit browser key.
    let before_escape = browser(window, editor, json!({"type":"snapshot"}), cx).await;
    native_pointer(&native, start, Some(true));
    native_tick(cx).await;
    native_pointer(&native, end, None);
    native_tick(cx).await;
    window
        .update(cx, |_, _, cx| assert!(view.read(cx).is_layout_resizing()))
        .unwrap();
    browser_key(window, editor, "Escape", "Escape", 0, 27, cx).await;
    until(
        window,
        view,
        "WebView Escape cancels divider",
        |app, _| !app.is_layout_resizing(),
        cx,
    )
    .await;
    native_pointer(&native, end, Some(false));
    native_tick(cx).await;
    window
        .update(cx, |_, _, cx| {
            assert_eq!(view.read(cx).layout, layout);
            assert!(!view.read(cx).layout_dirty);
            assert_guide(&native, false);
        })
        .unwrap();
    let after_escape = browser(window, editor, json!({"type":"snapshot"}), cx).await;
    for field in ["generation", "revision", "text"] {
        assert_eq!(before_escape[field], after_escape[field]);
    }
    // Escape may normally collapse a CodeMirror selection. Restore only that
    // fixture selection before subsequent workspace-preservation assertions.
    browser(
        window,
        editor,
        json!({
            "type":"select",
            "anchor":before_escape["selection"]["anchor"],
            "head":before_escape["selection"]["head"]
        }),
        cx,
    )
    .await;
}

async fn browser_key(
    window: AnyWindowHandle,
    editor: &Entity<MarkdownEditor>,
    key: &str,
    code: &str,
    modifiers: u32,
    virtual_key: i32,
    cx: &mut AsyncApp,
) {
    let reply = window
        .update(cx, |_, _, cx| {
            editor
                .read(cx)
                .dispatch_key(key, code, modifiers, virtual_key)
                .unwrap()
        })
        .unwrap();
    for _ in 0..100 {
        native_tick(cx).await;
        match reply.try_recv() {
            Ok(result) => {
                result.expect("CDP shortcut dispatch failed");
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                panic!("CDP shortcut callback disconnected")
            }
        }
    }
    panic!("CDP shortcut dispatch timed out");
}

async fn saved_shortcuts(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    database: &Path,
    cx: &mut AsyncApp,
) {
    let note = create_note(window, view, "原生设置快捷键回归", None, cx).await;
    let original = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.selected_note = Some(note.id.clone());
                app.selected_version = None;
                app.search
                    .update(cx, |input, cx| input.set_value("", window, cx));
                app.folders
                    .update(cx, |panel, cx| panel.set_filter(FolderFilter::All, cx));
                cx.notify();
            });
            let original = view
                .read(cx)
                .preferences_ui
                .saved
                .as_ref()
                .unwrap()
                .shortcuts
                .clone();
            window.click(("nav", 4_usize), cx);
            let save_input = view.read(cx).preferences_ui.shortcuts["editor.save"].clone();
            save_input.update(cx, |input, cx| input.set_value("Ctrl+Shift+S", window, cx));
            assert!(view.read(cx).settings_dirty(cx));
            assert_eq!(
                view.read(cx)
                    .preferences_ui
                    .saved
                    .as_ref()
                    .unwrap()
                    .shortcuts,
                original,
                "shortcut field updated committed configuration before save"
            );
            crate::smoke_library::click_scrolled(window, "save-settings".into(), "page-scroll", cx);
            original
        })
        .unwrap();
    until(
        window,
        view,
        "custom shortcut committed",
        |app, cx| {
            !app.preferences_ui.busy
                && !app.settings_dirty(cx)
                && app
                    .preferences_ui
                    .saved
                    .as_ref()
                    .is_some_and(|saved| saved.shortcuts["editor.save"] == "Ctrl+Shift+S")
        },
        cx,
    )
    .await;
    let (editor, versions) = window
        .update(cx, |_, window, cx| {
            window.click(("nav", 1_usize), cx);
            window.render_frame(cx);
            window.click("edit-note", cx);
            (
                view.read(cx).note_content.clone(),
                view.read(cx)
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .versions
                    .iter()
                    .filter(|version| version.note_id == note.id)
                    .count(),
            )
        })
        .unwrap();
    until(
        window,
        view,
        "custom shortcut editor ready",
        |app, cx| app.note_editor.is_some() && app.note_content.read(cx).ready(),
        cx,
    )
    .await;
    crate::smoke_visibility::visible(window, &editor, cx).await;
    let current = browser(window, &editor, json!({"type":"snapshot"}), cx).await;
    let expected = format!(
        "{}\n自定义保存组合键已应用。\n",
        current["text"].as_str().unwrap()
    );
    browser(window, &editor, json!({"type":"replace","from":0,"to":current["text"].as_str().unwrap().encode_utf16().count(),"text":expected}), cx).await;
    browser_key(window, &editor, "s", "KeyS", 2, 83, cx).await;
    for _ in 0..3 {
        native_tick(cx).await;
    }
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert_eq!(
                app.snapshot
                    .as_ref()
                    .unwrap()
                    .versions
                    .iter()
                    .filter(|version| version.note_id == note.id)
                    .count(),
                versions,
                "old Ctrl+S still saved after settings remap"
            );
            assert!(!app.note_saving && app.note_intent.is_none());
        })
        .unwrap();
    browser_key(window, &editor, "S", "KeyS", 10, 83, cx).await;
    until(
        window,
        view,
        "custom shortcut saves through native flush barrier",
        |app, _| {
            !app.note_saving
                && app.note_intent.is_none()
                && app
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .versions
                    .iter()
                    .filter(|version| version.note_id == note.id)
                    .count()
                    == versions + 1
        },
        cx,
    )
    .await;
    for _ in 0..3 {
        native_tick(cx).await;
    }
    assert_eq!(std::fs::read_to_string(&note.path).unwrap(), expected);
    window
        .update(cx, |_, window, cx| {
            assert_eq!(
                view.read(cx)
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .versions
                    .iter()
                    .filter(|version| version.note_id == note.id)
                    .count(),
                versions + 1,
                "one shortcut created more than one saved version"
            );
            window.click(("nav", 4_usize), cx);
            for (action, binding) in &original {
                let input = view.read(cx).preferences_ui.shortcuts[action].clone();
                input.update(cx, |input, cx| input.set_value(binding.clone(), window, cx));
            }
            crate::smoke_library::click_scrolled(window, "save-settings".into(), "page-scroll", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "original shortcut settings restored",
        |app, cx| {
            !app.preferences_ui.busy
                && !app.settings_dirty(cx)
                && app
                    .preferences_ui
                    .saved
                    .as_ref()
                    .is_some_and(|saved| saved.shortcuts == original)
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| window.click(("nav", 1_usize), cx))
        .unwrap();
    std::fs::write(database.with_file_name("native-shortcut-settings-result.txt"),
        "PASS: shortcut draft requires explicit save; committed Ctrl+Shift+S reaches the persistent WebView; old Ctrl+S does not save; new chord saves one version through native flush; original bindings restored. Shortcut chords used explicit CDP Input.dispatchKeyEvent; the existing strict Win32 cursor/keyboard gates remain separate.\n").unwrap();
}

pub async fn exercise(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    database: &Path,
    cx: &mut AsyncApp,
) {
    settled(window, view, cx).await;
    let workspace_width = if std::env::var_os("WW_STUDYAPP_SMOKE_NARROW_WORKSPACE").is_some() {
        1024.
    } else {
        1440.
    };
    window
        .update(cx, |_, window, cx| {
            window.resize(size(px(workspace_width), px(1000.)));
            view.update(cx, |app, cx| {
                app.page = Page::Notes;
                app.history = false;
                app.show_detached = false;
                app.notice = None;
                app.search
                    .update(cx, |input, cx| input.set_value("", window, cx));
                app.folders
                    .update(cx, |panel, cx| panel.set_filter(FolderFilter::All, cx));
                cx.notify();
            });
        })
        .unwrap();
    let note = create_note(window, view, "工作区拖动预览回归", None, cx).await;
    let original = std::fs::read(&note.path).unwrap();
    let editor = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.selected_note = Some(note.id.clone());
                app.selected_version = None;
                app.search.update(cx, |input, cx| {
                    input.set_value("工作区拖动预览回归", window, cx)
                });
                cx.notify();
            });
            view.read(cx).note_content.clone()
        })
        .unwrap();
    crate::smoke_visibility::visible(window, &editor, cx).await;
    crate::smoke_visibility::assert_cursor(
        window,
        &editor,
        "toggle-navigation".into(),
        CursorStyle::PointingHand,
        cx,
    )
    .await;
    crate::smoke_visibility::assert_cursor(
        window,
        &editor,
        "resize-notes".into(),
        CursorStyle::ResizeColumn,
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.storage_busy = true;
                cx.notify();
            });
            window.render_frame(cx);
            let preferred = view.read(cx).layout.clone();
            let dirty = view.read(cx).layout_dirty;
            let persisted = view
                .read(cx)
                .snapshot
                .as_ref()
                .unwrap()
                .settings
                .layout
                .clone();
            window.click("toggle-navigation", cx);
            for divider in ["resize-navigation", "resize-folders", "resize-notes"] {
                let start = window.find(SharedString::from(divider)).bounds().center();
                press(window, start, cx);
                assert!(
                    !view.read(cx).is_layout_resizing(),
                    "busy divider captured pointer"
                );
                let end = start + point(px(20.), px(0.));
                pointer(window, end, true, cx);
                release(window, end, cx);
            }
            assert_eq!(
                view.read(cx).layout,
                preferred,
                "busy controls changed preferred layout"
            );
            assert_eq!(
                view.read(cx).layout_dirty,
                dirty,
                "busy controls queued layout persistence"
            );
            assert_eq!(
                view.read(cx).snapshot.as_ref().unwrap().settings.layout,
                persisted
            );
        })
        .unwrap();
    crate::smoke_visibility::assert_cursor(
        window,
        &editor,
        "edit-note".into(),
        CursorStyle::Arrow,
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.storage_busy = false;
                cx.notify();
            });
            window.render_frame(cx);
        })
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.notice = Some(Err(
                    "工作区浮动提示遮挡回归：这段提示应拦截覆盖范围内的点击。".repeat(8),
                ));
                cx.notify();
            });
            window.render_frame(cx);
            let notice = window.find("floating-notice").bounds();
            for action in ["new-note", "header-import", "toggle-navigation"] {
                assert!(
                    !notice.intersects(&window.find(SharedString::from(action)).bounds()),
                    "floating notice covered header action {action}"
                );
            }
            let body = window.find("notice-message-scroll").bounds().center();
            assert!(notice.contains(&body));
            press(window, body, cx);
            release(window, body, cx);
            assert!(
                view.read(cx).note_editor.is_none(),
                "floating notice body click opened an editor"
            );
            assert!(
                view.read(cx).notice.is_some(),
                "notice body click dismissed the notice"
            );
            window
                .render_to_image()
                .unwrap()
                .save(database.with_file_name("workspace-floating-notice.png"))
                .unwrap();
            window.click("dismiss-notice", cx);
            assert!(view.read(cx).notice.is_none());
        })
        .unwrap();
    let baseline = browser(
        window,
        &editor,
        json!({"type":"select","anchor":7,"head":9}),
        cx,
    )
    .await;
    let reader_key = window
        .update(cx, |_, _, cx| view.read(cx).reader_key.clone())
        .unwrap();
    for (divider, pane, delta) in [
        ("resize-navigation", "navigation-pane", 24.),
        ("resize-folders", "folders-pane", 32.),
        ("resize-notes", "notes-pane", 36.),
    ] {
        resize_pane(window, view, &editor, divider, pane, delta, cx).await;
        unchanged(window, view, &editor, &baseline, &reader_key, cx).await;
    }
    cancel_resize(window, view, &editor, cx).await;
    unchanged(window, view, &editor, &baseline, &reader_key, cx).await;
    let expanded = window
        .update(cx, |_, window, cx| {
            let expanded = view.read(cx).layout.navigation_width;
            let start = window.find("resize-navigation").bounds().center();
            press(window, start, cx);
            pointer(window, point(px(100.), start.y), true, cx);
            let edge = point(px(24.), start.y);
            pointer(window, edge, true, cx);
            assert!(
                view.read(cx).layout.navigation_visible,
                "navigation collapsed before release"
            );
            assert!(window.try_find("navigation-pane").is_some());
            release(window, edge, cx);
            assert!(
                !view.read(cx).layout.navigation_visible,
                "left edge did not collapse navigation"
            );
            assert!(window.try_find("navigation-pane").is_none());
            assert!(window.try_find("resize-navigation").is_none());
            assert!(
                window.try_find("toggle-navigation").is_some(),
                "collapsed navigation lost restore button"
            );
            expanded
        })
        .unwrap();
    until(
        window,
        view,
        "collapsed navigation saved",
        |app, _| {
            !app.layout_dirty
                && !app
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .settings
                    .layout
                    .navigation_visible
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.click("toggle-navigation", cx);
            window.render_frame(cx);
            assert_eq!(view.read(cx).layout.navigation_width, expanded);
            assert!(
                (f32::from(window.find("navigation-pane").bounds().size.width) - expanded as f32)
                    .abs()
                    < 2.
            );
        })
        .unwrap();
    until(
        window,
        view,
        "restored navigation saved",
        |app, _| {
            !app.layout_dirty
                && app
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .settings
                    .layout
                    .navigation_visible
        },
        cx,
    )
    .await;
    unchanged(window, view, &editor, &baseline, &reader_key, cx).await;
    let folders_width = window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let preferred = view.read(cx).layout.folders_width;
            let start = window.find("resize-folders").bounds().center();
            let left = window.find("folders-pane").bounds().left();
            press(window, start, cx);
            let edge = point(left + px(24.), start.y);
            pointer(window, edge, true, cx);
            assert!(
                view.read(cx).layout.folders_visible,
                "folders collapsed before release"
            );
            assert!(window.try_find("folders-pane").is_some());
            release(window, edge, cx);
            assert!(!view.read(cx).layout.folders_visible);
            assert_eq!(
                view.read(cx).layout.folders_width,
                preferred,
                "collapse lost restored folder width"
            );
            assert!(window.try_find("folders-pane").is_none());
            assert!(window.try_find("resize-folders").is_none());
            assert!(
                window.try_find("toggle-folders").is_some(),
                "collapsed folders lost restore icon"
            );
            preferred
        })
        .unwrap();
    until(
        window,
        view,
        "collapsed folders saved",
        |app, _| {
            !app.layout_dirty
                && !app
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .settings
                    .layout
                    .folders_visible
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.click("toggle-folders", cx);
            window.render_frame(cx);
            assert!(view.read(cx).layout.folders_visible);
            assert_eq!(view.read(cx).layout.folders_width, folders_width);
            assert!(window.try_find("resize-folders").is_some());
            assert!(
                (f32::from(window.find("folders-pane").bounds().size.width) - folders_width as f32)
                    .abs()
                    < 2.
            );
        })
        .unwrap();
    until(
        window,
        view,
        "restored folders saved",
        |app, _| {
            !app.layout_dirty
                && app
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .settings
                    .layout
                    .folders_visible
        },
        cx,
    )
    .await;
    unchanged(window, view, &editor, &baseline, &reader_key, cx).await;
    crate::smoke_visibility::capture(
        window,
        &editor,
        &database.with_file_name("workspace-resized-native.bmp"),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.click("folder-create", cx);
            window.render_frame(cx);
            assert!(view.read(cx).folders.read(cx).modal_open());
            assert!(window.try_find("modal-content-scroll").is_some());
            hidden(&editor, cx);
            window
                .render_to_image()
                .unwrap()
                .save(database.with_file_name("workspace-modal.png"))
                .unwrap();
            let navigation_visible = view.read(cx).layout.navigation_visible;
            window.click("toggle-navigation", cx);
            assert_eq!(
                view.read(cx).layout.navigation_visible,
                navigation_visible,
                "dialog backdrop leaked a click"
            );
            window.press("escape", cx);
            assert!(
                !view.read(cx).folders.read(cx).modal_open(),
                "Escape failed to close folder dialog"
            );
        })
        .unwrap();
    unchanged(window, view, &editor, &baseline, &reader_key, cx).await;
    const DESTINATION: &str = "工作区布局回归目标";
    window
        .update(cx, |_, window, cx| {
            window.click("folder-create", cx);
            window.input(DESTINATION, cx); // Initial modal focus must accept typing.
            window.press("enter", cx);
            window.press("enter", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "folder modal Enter submitted once",
        |app, cx| {
            !app.folders.read(cx).modal_open()
                && app
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .folders
                    .iter()
                    .filter(|f| f.name == DESTINATION)
                    .count()
                    == 1
        },
        cx,
    )
    .await;
    unchanged(window, view, &editor, &baseline, &reader_key, cx).await;
    crate::smoke_visibility::capture(
        window,
        &editor,
        &database.with_file_name("workspace-after-dialog-native.bmp"),
        cx,
    )
    .await;
    crate::smoke_visibility::capture_window(
        window,
        &editor,
        &database.with_file_name("workspace-full.bmp"),
        cx,
    )
    .await;

    let destination = window
        .update(cx, |_, _, cx| {
            view.read(cx)
                .snapshot
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .find(|f| f.name == DESTINATION)
                .unwrap()
                .id
                .clone()
        })
        .unwrap();
    let destination_row: SharedString = format!("folder-row-{destination}").into();
    window
        .update(cx, |_, window, cx| {
            crate::smoke_library::reveal_scrolled(
                window,
                destination_row.clone(),
                "folder-tree-scroll",
                cx,
            );
        })
        .unwrap();
    crate::smoke_visibility::assert_cursor(
        window,
        &editor,
        destination_row,
        CursorStyle::PointingHand,
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let source = window
                .find(SharedString::from(format!("note-{}", note.id)))
                .bounds();
            let grab = point(source.right() - px(3.), source.bottom() - px(3.));
            press(window, grab, cx);
            let pointer_at = grab + point(px(10.), px(-10.));
            pointer(window, pointer_at, true, cx);
            assert!(
                cx.has_active_drag(),
                "lower-right source grab did not start a drag"
            );
            let preview = window.find("note-drag-preview").bounds();
            assert!((f32::from(preview.origin.x - pointer_at.x) - 12.).abs() <= 2.);
            assert!((f32::from(preview.origin.y - pointer_at.y) - 12.).abs() <= 2.);
            assert!(
                preview.size.width <= px(222.) && preview.size.height <= px(64.),
                "drag ghost grew to source bounds"
            );
            window
                .render_to_image()
                .unwrap()
                .save(database.with_file_name("workspace-bottom-right-drag.png"))
                .unwrap();
            let edge = point(
                window.viewport_size().width - px(4.),
                window.viewport_size().height - px(4.),
            );
            pointer(window, edge, true, cx);
            let snapped = window.find("note-drag-preview").bounds();
            assert!(snapped.right() <= window.viewport_size().width + px(1.));
            assert!(snapped.bottom() <= window.viewport_size().height + px(1.));
            assert!(
                snapped.right() >= edge.x - px(1.) && snapped.bottom() >= edge.y - px(1.),
                "edge preview drifted away from pointer"
            );
            let target = window
                .find(SharedString::from(format!("folder-drop-{destination}")))
                .bounds()
                .center();
            pointer(window, target, true, cx);
            release(window, target, cx);
            assert!(!cx.has_active_drag());
        })
        .unwrap();
    until(
        window,
        view,
        "lower-right drag moved only logical membership",
        |app, _| {
            app.snapshot.as_ref().unwrap().notes.iter().any(|n| {
                n.id == note.id && n.folder_id.as_ref() == Some(&destination) && n.path == note.path
            })
        },
        cx,
    )
    .await;
    unchanged(window, view, &editor, &baseline, &reader_key, cx).await;
    assert_eq!(std::fs::read(&note.path).unwrap(), original);
    // Compact defaults now fit at 1000 px. Explicitly enlarge the preferred
    // note width so the following narrow-window assertions still test fitting.
    resize_pane(
        window,
        view,
        &editor,
        "resize-notes",
        "notes-pane",
        180.,
        cx,
    )
    .await;
    unchanged(window, view, &editor, &baseline, &reader_key, cx).await;
    window
        .update(cx, |_, window, _| window.resize(size(px(1000.), px(750.))))
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(150))
        .await;
    window
        .update(cx, |_, window, cx| {
            let preferred = view.read(cx).layout.clone();
            let persisted = view
                .read(cx)
                .snapshot
                .as_ref()
                .unwrap()
                .settings
                .layout
                .clone();
            let dirty = view.read(cx).layout_dirty;
            window.render_frame(cx);
            assert!(window.viewport_size().width <= px(1001.));
            assert!(
                window.find("document-pane").bounds().size.width >= px(279.),
                "small window stole reserved document width"
            );
            assert!(
                window.find("notes-pane").bounds().size.width < px(preferred.notes_width as f32),
                "fixture must temporarily fit a smaller note pane"
            );
            for divider in ["resize-navigation", "resize-folders", "resize-notes"] {
                let position = window.find(SharedString::from(divider)).bounds().center();
                press(window, position, cx);
                assert!(view.read(cx).is_layout_resizing());
                release(window, position, cx);
                assert!(!view.read(cx).is_layout_resizing());
                assert_eq!(
                    view.read(cx).layout,
                    preferred,
                    "no-motion divider click saved temporary fitted widths"
                );
                assert_eq!(view.read(cx).layout_dirty, dirty);
                assert_eq!(
                    view.read(cx).snapshot.as_ref().unwrap().settings.layout,
                    persisted
                );
            }
        })
        .unwrap();
    unchanged(window, view, &editor, &baseline, &reader_key, cx).await;
    crate::smoke_visibility::capture(
        window,
        &editor,
        &database.with_file_name("workspace-small-native.bmp"),
        cx,
    )
    .await;
    crate::smoke_visibility::capture_window(
        window,
        &editor,
        &database.with_file_name("workspace-small-full.bmp"),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.viewport_size().width <= px(1001.));
            window
                .render_to_image()
                .unwrap()
                .save(database.with_file_name("workspace-small.png"))
                .unwrap();
        })
        .unwrap();
    let saved_layout = window
        .update(cx, |_, _, cx| {
            view.read(cx)
                .snapshot
                .as_ref()
                .unwrap()
                .settings
                .layout
                .clone()
        })
        .unwrap();
    std::fs::write(
        database.with_file_name("workspace-saved-layout.json"),
        serde_json::to_vec_pretty(&saved_layout).unwrap(),
    )
    .unwrap();
    saved_shortcuts(window, view, database, cx).await;
    std::fs::write(database.with_file_name("workspace-smoke-result.txt"),
        "PASS: three deferred native divider drags preserve layout and visible WebView bounds while held; owned dashed guide retains capture/focus and is not globally topmost; release commits and Escape cancels without saving; busy layout controls remain inert; navigation and folder edge collapse retain width and restore icons; same editor generation/revision/selection/text survive workspace changes; folder modal background blocking, Escape and exactly-once Enter; floating notice contains body clicks and dismisses explicitly; compact native drag preview and metadata-only drop; 1000px document reservation and no-motion divider clicks retain preferred widths.\n").unwrap();
}
