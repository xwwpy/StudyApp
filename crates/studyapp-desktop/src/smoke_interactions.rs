//! Native regression scenarios for note interactions, display and nested scrolling.
use crate::{
    app::{Page, StudyApp},
    folders_ui::FolderFilter,
    smoke_library::{click_scrolled, create_note, scroll_settings, settled, until},
    worker::Work,
};
use gpui_kit::{component::WindowExt, test::TestWindowExt, *};
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
    window
        .update(cx, |_, window, cx| {
            window.click("folders-all", cx);
            crate::smoke_library::reveal_scrolled(
                window,
                format!("folder-row-{destination}").into(),
                "folder-tree-scroll",
                cx,
            );
            window.right_click(SharedString::from(format!("folder-row-{destination}")), cx);
        })
        .unwrap();
    until(
        window,
        view,
        "folder row context menu opened",
        |app, cx| app.folders.read(cx).overlay_open() && !app.folders.read(cx).modal_open(),
        cx,
    )
    .await;
    let rename_input = window
        .update(cx, |_, window, cx| {
            assert_eq!(
                view.read(cx).folders.read(cx).filter(),
                &FolderFilter::All,
                "right-click must keep the current note filter"
            );
            assert_eq!(
                editor.read(cx).native_state().unwrap()["controllerVisible"],
                false,
                "folder context menu must cover the native editor"
            );
            for old_button in ["folder-rename", "folder-move", "folder-delete"] {
                assert!(
                    window.try_find(SharedString::from(old_button)).is_none(),
                    "folder row actions must live in its context menu"
                );
            }
            window.press("down", cx);
            window.press("down", cx);
            window.press("enter", cx); // Rename the clicked row, while All remains selected.
            window.render_frame(cx);
            assert!(
                view.read(cx).folders.read(cx).modal_open(),
                "menu dismissal must not close the newly opened action dialog"
            );
            window.click("folder-name-input", cx);
            window.press("ctrl-a", cx);
            window
                .focused_input(cx)
                .expect("rename input must retain focus")
                .as_input()
                .expect("folder name uses a single-line input")
                .clone()
        })
        .unwrap();
    // Configured native shortcuts defer dispatch to avoid reentering StudyApp.
    // Real key messages leave the event loop between Ctrl+A and text input.
    cx.background_executor()
        .timer(Duration::from_millis(40))
        .await;
    until(
        window,
        view,
        "deferred Ctrl+A selected the rename input",
        |_, cx| {
            let input = rename_input.read(cx);
            input.selected_range() == (0..input.text().len())
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            assert_eq!(
                rename_input.read(cx).value().as_ref(),
                "拖动目标目录",
                "rename must start from the clicked folder's original name"
            );
            window.input("右键重命名拖动目录", cx);
            window.press("enter", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "context rename targets clicked folder",
        |app, cx| {
            !app.folders.read(cx).overlay_open()
                && app
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .folders
                    .iter()
                    .any(|folder| folder.id == destination && folder.name == "右键重命名拖动目录")
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.click(SharedString::from(format!("folder-row-{destination}")), cx);
            window.render_frame(cx);
            for old_button in ["folder-rename", "folder-move", "folder-delete"] {
                assert!(
                    window.try_find(SharedString::from(old_button)).is_none(),
                    "selected folder retained its obsolete bottom action buttons"
                );
            }
            window.click("folders-all", cx);
            // The compact row menu gives keyboard users the same four actions.
            crate::smoke_library::reveal_scrolled(
                window,
                format!("folder-menu-{destination}").into(),
                "folder-tree-scroll",
                cx,
            );
            window.click(SharedString::from(format!("folder-menu-{destination}")), cx);
        })
        .unwrap();
    until(
        window,
        view,
        "folder row keyboard menu opened",
        |app, cx| app.folders.read(cx).overlay_open() && !app.folders.read(cx).modal_open(),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            window.press("down", cx);
            window.press("enter", cx); // New child is the first menu item.
            window.render_frame(cx);
            assert!(view.read(cx).folders.read(cx).modal_open());
            window.input("键盘菜单子目录", cx);
            window.press("enter", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "keyboard row menu creates child",
        |app, cx| {
            !app.folders.read(cx).overlay_open()
                && app.snapshot.as_ref().unwrap().folders.iter().any(|folder| {
                    folder.name == "键盘菜单子目录"
                        && folder.parent_id.as_ref() == Some(&destination)
                })
        },
        cx,
    )
    .await;
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
            assert!(
                view.read(cx)
                    .preferences_ui
                    .draft
                    .as_ref()
                    .unwrap()
                    .show_file_paths
            );
            assert!(
                !view
                    .read(cx)
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .settings
                    .show_file_paths,
                "checkbox saved before explicit save"
            );
            assert!(view.read(cx).settings_dirty(cx));
            window.click(("nav", 1_usize), cx);
            window.render_frame(cx);
            assert_eq!(
                view.read(cx).page,
                Page::Settings,
                "dirty settings left without confirmation"
            );
            assert!(view.read(cx).preferences_ui.leave.is_some());
            window.click("settings-continue-editing", cx);
            assert_eq!(view.read(cx).page, Page::Settings);
            assert!(view.read(cx).preferences_ui.leave.is_none());
            assert!(
                view.read(cx).settings_dirty(cx),
                "continue editing discarded the draft"
            );
            let library = view.read(cx).last_library.clone();
            view.update(cx, |app, cx| {
                app.storage_busy = true;
                app.accept_picker_result(
                    library.clone(),
                    Work::SwitchLibrary(database.with_file_name("unused-picker-result")),
                    cx,
                );
                assert!(app.preferences_ui.leave.is_none());
                assert!(!app.preferences_ui.replacing);
                let (queued_library, work) = app
                    .picker_results
                    .try_recv()
                    .expect("picker result must wait while migration owns storage locks");
                assert_eq!(queued_library, library);
                app.storage_busy = false;
                app.accept_picker_result(queued_library, work, cx);
            });
            assert!(
                matches!(
                    view.read(cx).preferences_ui.leave,
                    Some(crate::settings_ui::SettingsContinuation::PickedWork(..))
                ),
                "async picker result bypassed dirty settings guard"
            );
            assert_eq!(view.read(cx).last_library, library);
            assert!(
                !view.read(cx).storage_busy,
                "guarded picker result started switching libraries"
            );
            window.render_frame(cx);
            window.click("settings-continue-editing", cx);
            view.update(cx, |app, cx| {
                app.close_requested = true;
                app.finish_draft_persist(crate::editor_lifecycle::NoteIntent::Close, window, cx);
            });
            assert!(
                matches!(
                    view.read(cx).preferences_ui.leave,
                    Some(crate::settings_ui::SettingsContinuation::Close)
                ),
                "async note close bypassed dirty settings guard"
            );
            assert!(!view.read(cx).close_allowed && !view.read(cx).close_requested);
            assert_eq!(view.read(cx).page, Page::Settings);
            window.render_frame(cx);
            window.click("settings-continue-editing", cx);
            assert!(view.read(cx).preferences_ui.leave.is_none());
            assert!(view.read(cx).settings_dirty(cx));
            window.click(("nav", 1_usize), cx);
            window.render_frame(cx);
            window.click("settings-discard", cx);
            assert_eq!(view.read(cx).page, Page::Notes);
            assert!(
                !view
                    .read(cx)
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .settings
                    .show_file_paths
            );
            assert!(
                !view.read(cx).settings_dirty(cx),
                "discard retained dirty settings"
            );
            window.click(("nav", 4_usize), cx);
            click_scrolled(window, "show-file-paths".into(), "page-scroll", cx);
            window.click(("nav", 1_usize), cx);
            window.render_frame(cx);
            assert!(view.read(cx).preferences_ui.leave.is_some());
            window.click("settings-save-and-continue", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "path setting saved before navigation continues",
        |app, cx| {
            app.page == Page::Notes
                && !app.preferences_ui.busy
                && app.preferences_ui.leave.is_none()
                && !app.settings_dirty(cx)
                && app.snapshot.as_ref().unwrap().settings.show_file_paths
        },
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
            assert!(
                !view
                    .read(cx)
                    .preferences_ui
                    .draft
                    .as_ref()
                    .unwrap()
                    .show_file_paths
            );
            assert!(
                view.read(cx)
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .settings
                    .show_file_paths
            );
            click_scrolled(window, "save-settings".into(), "page-scroll", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "path setting disabled again",
        |app, cx| {
            !app.preferences_ui.busy
                && !app.settings_dirty(cx)
                && !app.snapshot.as_ref().unwrap().settings.show_file_paths
        },
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
        "PASS: Enter directory creation; unselected folder right-click rename and row-menu child creation; folder actions use menus and modal survives menu dismissal; actual pointer drag; right-click Move to Unclassified; explicit path copy; show-path checkbox stages until save; dirty-settings continue/discard/save-and-continue; nested wheel at both boundaries, actual scrollbar drag and outer scrolling outside the list; source bytes unchanged.\n").unwrap();
}
