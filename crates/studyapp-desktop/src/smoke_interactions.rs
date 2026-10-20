//! Native regression scenarios for note interactions, display and nested scrolling.
use crate::{
    app::{Page, StudyApp},
    folders_ui::FolderFilter,
    smoke_library::{click_scrolled, create_note, scroll_settings, settled, until},
    worker::Work,
};
use gpui_kit::{test::TestWindowExt, *};
use std::{path::Path, time::Duration};
use studyapp_desktop::presentation::display_path;

pub async fn exercise(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    database: &Path,
    cx: &mut AsyncApp,
) {
    settled(window, view, cx).await;
    window
        .update(cx, |_, window, cx| {
            window.resize(size(px(1280.), px(900.)));
            view.update(cx, |app, cx| {
                app.page = Page::Notes;
                app.history = false;
                app.search
                    .update(cx, |input, cx| input.set_value("", window, cx));
                app.folders
                    .update(cx, |panel, cx| panel.set_filter(FolderFilter::All, cx));
                cx.notify();
            });
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(150))
        .await;
    let editor = window
        .update(cx, |_, _, cx| view.read(cx).note_content.clone())
        .unwrap();
    crate::smoke_visibility::capture(
        window,
        &editor,
        &database.with_file_name("native-after-history.bmp"),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.click("folder-create", cx);
            window.click("folder-name-input", cx);
            window.input("拖动目标目录", cx);
            window.press("enter", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "Enter creates exactly one folder",
        |app, _| {
            app.snapshot
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .filter(|f| f.name == "拖动目标目录")
                .count()
                == 1
        },
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
                .find(|f| f.name == "拖动目标目录")
                .unwrap()
                .id
                .clone()
        })
        .unwrap();
    let note = create_note(window, view, "拖动与右键测试", None, cx).await;
    let bytes = std::fs::read(&note.path).unwrap();
    let note_element: SharedString = format!("note-{}", note.id).into();
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.search.update(cx, |input, cx| {
                    input.set_value("拖动与右键测试", window, cx)
                });
                app.selected_note = Some(note.id.clone());
                app.selected_version = None;
                cx.notify();
            });
            window.render_frame(cx);
            assert!(
                window.try_find("note-file-path").is_none(),
                "paths must be hidden by default"
            );
            assert!(
                window
                    .try_find(SharedString::from(format!("note-row-path-{}", note.id)))
                    .is_none()
            );
            window.drag_to(
                note_element.clone(),
                SharedString::from(format!("folder-drop-{destination}")),
                cx,
            );
        })
        .unwrap();
    until(
        window,
        view,
        "native drag moves into target",
        |app, _| {
            app.snapshot
                .as_ref()
                .unwrap()
                .notes
                .iter()
                .any(|n| n.id == note.id && n.folder_id.as_ref() == Some(&destination))
        },
        cx,
    )
    .await;
    assert_eq!(std::fs::read(&note.path).unwrap(), bytes);
    crate::smoke_visibility::hover(window, &editor, note_element.clone(), cx).await;
    for attempt in 0..100 {
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
        let shown = window
            .update(cx, |_, window, cx| {
                window.render_frame(cx);
                window.try_find("sidebar-tooltip-content").is_some()
            })
            .unwrap();
        if shown {
            break;
        }
        assert!(
            attempt < 99,
            "native pointer hover did not reveal schedule tooltip"
        );
    }
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let tip = window.find("sidebar-tooltip-content").bounds();
            let document = window.find("continuous-markdown-editor").bounds();
            assert!(
                tip.right() <= document.left(),
                "sidebar tooltip overlaps native editor"
            );
            window
                .render_to_image()
                .unwrap()
                .save(database.with_file_name("sidebar-hover.png"))
                .unwrap();
        })
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            window.right_click(note_element.clone(), cx);
        })
        .unwrap();
    until(
        window,
        view,
        "deferred note context menu opened",
        |app, _| app.active_note_menu.is_some(),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            assert_eq!(
                view.read(cx).note_content.read(cx).native_state().unwrap()["controllerVisible"],
                false
            );
            window.press("down", cx);
            window.press("right", cx);
            window.press("enter", cx); // Unclassified is first for a classified note.
        })
        .unwrap();
    until(
        window,
        view,
        "context menu moves to Unclassified and closes",
        |app, _| {
            app.active_note_menu.is_none()
                && app
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .notes
                    .iter()
                    .any(|n| n.id == note.id && n.folder_id.is_none())
        },
        cx,
    )
    .await;
    let editor = window
        .update(cx, |_, _, cx| view.read(cx).note_content.clone())
        .unwrap();
    crate::smoke_visibility::capture(
        window,
        &editor,
        &database.with_file_name("native-after-menu.bmp"),
        cx,
    )
    .await;
    let previous_clipboard = window
        .update(cx, |_, _, cx| cx.read_from_clipboard())
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            window.right_click(note_element.clone(), cx)
        })
        .unwrap();
    until(
        window,
        view,
        "deferred copy-path menu opened",
        |app, _| app.active_note_menu.is_some(),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.press("down", cx);
            window.press("down", cx);
            window.press("down", cx);
            window.press("enter", cx);
            assert_eq!(
                cx.read_from_clipboard().and_then(|item| item.text()),
                Some(display_path(&note.path))
            );
            cx.write_to_clipboard(
                previous_clipboard.unwrap_or_else(|| ClipboardItem::new_string(String::new())),
            );
            window.click(("nav", 4_usize), cx);
            click_scrolled(window, "show-file-paths".into(), "page-scroll", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "path setting enabled",
        |app, _| app.snapshot.as_ref().unwrap().settings.show_file_paths,
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.click(("nav", 1_usize), cx);
            window.render_frame(cx);
            assert!(window.try_find("note-file-path").is_some());
            assert!(
                window
                    .try_find(SharedString::from(format!("note-row-path-{}", note.id)))
                    .is_some()
            );
            window.click("edit-note", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "editor path enabled",
        |app, _| app.note_editor.is_some(),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("editor-file-path").is_some());
            window.click("discard-note-draft", cx);
            window.click("confirm-note-action", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "discard unchanged path test draft",
        |app, _| app.note_editor.is_none() && app.draft_discard.is_none(),
        cx,
    )
    .await;
    for i in 0..12 {
        create_note(window, view, &format!("空间滚动测试 {i:02}"), None, cx).await;
    }
    window
        .update(cx, |_, window, cx| {
            window.click(("nav", 4_usize), cx);
            view.update(cx, |app, cx| app.storage_work(Work::RefreshStorage, cx));
        })
        .unwrap();
    until(
        window,
        view,
        "storage report with multiple copies",
        |app, _| {
            !app.storage_busy
                && app
                    .storage_report
                    .as_ref()
                    .is_some_and(|r| r.copies.len() >= 12)
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            // Position the complete bounded list within the viewport.
            for _ in 0..35 {
                window.render_frame(cx);
                let bounds = window.find("storage-copy-list").bounds();
                if bounds.origin.y > px(120.)
                    && bounds.bottom() < window.viewport_size().height - px(50.)
                {
                    break;
                }
                let delta = if bounds.origin.y <= px(120.) {
                    80.
                } else {
                    -80.
                };
                scroll_settings(window, delta, cx);
            }
            let initial = window.find("storage-copy-list").bounds();
            assert!(initial.origin.y > px(80.));
            assert!(initial.bottom() < window.viewport_size().height);
            assert!(view.read(cx).storage_copy_scroll.max_offset().y > px(0.));
            for delta in [-80., -10000., -80., 10000., 80.] {
                window.scroll(
                    "storage-copy-list",
                    ScrollDelta::Pixels(point(px(0.), px(delta))),
                    cx,
                );
                assert_eq!(
                    window.find("storage-copy-list").bounds().origin,
                    initial.origin,
                    "nested wheel moved Settings"
                );
            }
            assert_eq!(view.read(cx).storage_copy_scroll.offset().y, px(0.));
            // Base Scrollbar is a custom Element; its vertical track is at the
            // viewport's right edge (the element itself spans the whole list).
            window.drag(
                point(initial.right() - px(8.), initial.origin.y + px(8.)),
                point(initial.right() - px(8.), initial.bottom() - px(8.)),
                cx,
            );
            assert!(
                view.read(cx).storage_copy_scroll.offset().y
                    == -view.read(cx).storage_copy_scroll.max_offset().y,
                "scrollbar thumb drag must reach the final file"
            );
            window
                .render_to_image()
                .unwrap()
                .save(database.with_file_name("storage-contained-scroll.png"))
                .unwrap();
            let old_top = window.find("storage-copy-list").bounds().origin.y;
            // Wheel to the left of the inset card, outside its file-list bounds.
            let page = window.find("page-scroll").bounds();
            window.dispatch_event(
                ScrollWheelEvent {
                    position: point(page.origin.x + px(5.), initial.center().y),
                    delta: ScrollDelta::Pixels(point(px(0.), px(-60.))),
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            assert!(
                window.find("storage-copy-list").bounds().origin.y < old_top,
                "Settings must scroll outside the file list"
            );
            click_scrolled(window, "show-file-paths".into(), "page-scroll", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "path setting disabled again",
        |app, _| !app.snapshot.as_ref().unwrap().settings.show_file_paths,
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.click(("nav", 1_usize), cx);
            window.render_frame(cx);
            assert!(window.try_find("note-file-path").is_none());
            view.update(cx, |app, cx| {
                app.search
                    .update(cx, |input, cx| input.set_value("", window, cx));
                cx.notify();
            });
        })
        .unwrap();
    crate::smoke_visibility::capture(
        window,
        &editor,
        &database.with_file_name("native-after-settings.bmp"),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, _| window.resize(size(px(1000.), px(750.))))
        .unwrap();
    crate::smoke_visibility::capture(
        window,
        &editor,
        &database.with_file_name("native-after-resize.bmp"),
        cx,
    )
    .await;
    assert_eq!(std::fs::read(&note.path).unwrap(), bytes);
    std::fs::write(database.with_file_name("interactions-smoke-result.txt"),
        "PASS: Enter directory creation, actual pointer drag, right-click Move to Unclassified, context-menu visibility and keyboard selection, explicit path copy, persistent display toggle UI, nested wheel at both boundaries, actual scrollbar drag and outer scrolling outside the list; source bytes unchanged.\n").unwrap();
}
