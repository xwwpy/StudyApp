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
    let expected = window
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
            press(window, start, cx);
            assert!(
                view.read(cx).is_layout_resizing(),
                "divider did not capture pointer"
            );
            hidden(editor, cx);
            let end = start + point(px(delta), px(0.));
            pointer(window, end, true, cx);
            let during = window
                .find(SharedString::from(pane.to_owned()))
                .bounds()
                .size
                .width;
            assert!(
                (f32::from(during - before) - delta).abs() < 2.,
                "{pane} did not resize live"
            );
            assert_eq!(
                view.read(cx).snapshot.as_ref().unwrap().settings.layout,
                saved,
                "pointer move persisted layout before release"
            );
            release(window, end, cx);
            assert!(
                !view.read(cx).is_layout_resizing(),
                "divider retained capture after release"
            );
            let after = window
                .find(SharedString::from(pane.to_owned()))
                .bounds()
                .size
                .width;
            assert!(
                (f32::from(after - before) - delta).abs() < 2.,
                "{pane} width reverted on release"
            );
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

pub async fn exercise(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    database: &Path,
    cx: &mut AsyncApp,
) {
    settled(window, view, cx).await;
    window
        .update(cx, |_, window, cx| {
            window.resize(size(px(1440.), px(1000.)));
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
    let expanded = window
        .update(cx, |_, window, cx| {
            let expanded = view.read(cx).layout.navigation_width;
            let start = window.find("resize-navigation").bounds().center();
            press(window, start, cx);
            pointer(window, point(px(100.), start.y), true, cx);
            let edge = point(px(24.), start.y);
            pointer(window, edge, true, cx);
            release(window, edge, cx);
            assert!(
                !view.read(cx).layout.navigation_visible,
                "left edge did not collapse navigation"
            );
            assert!(window.try_find("navigation-pane").is_none());
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
    std::fs::write(database.with_file_name("workspace-smoke-result.txt"),
        "PASS: three live divider widths and release persistence; busy layout controls do not mutate preferences; intermediate edge collapse and restored original preferred width; native child hidden during resize/dialog and restored with same generation, revision, selection and text; folder modal background blocking, Escape and exactly-once Enter; floating notice avoids header actions, contains body clicks and dismisses explicitly; actual bottom-right drag preview origin, compact size and viewport snapping; metadata-only drop; 1000px document reservation and no-motion divider clicks retain preferred widths.\n").unwrap();
}
