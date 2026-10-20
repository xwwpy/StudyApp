//! Native regression coverage for logical folders, history and library storage.
//! Fixtures are retained beside the smoke library for inspection after a run.
use crate::{
    app::{Page, StudyApp},
    folders_ui::FolderFilter,
    markdown_editor::MarkdownEditor,
    worker::Work,
};
use gpui_kit::{test::TestWindowExt, *};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use studyapp_core::{Command, Note};

const TIMEOUT: Duration = Duration::from_secs(5);

async fn pause(cx: &AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(40))
        .await;
}

async fn until(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    description: &str,
    mut condition: impl FnMut(&StudyApp, &App) -> bool,
    cx: &mut AsyncApp,
) {
    let started = Instant::now();
    loop {
        if window
            .update(cx, |_, _, cx| condition(view.read(cx), cx))
            .expect("smoke window is open")
        {
            return;
        }
        assert!(
            started.elapsed() < TIMEOUT,
            "library smoke timed out: {description}"
        );
        pause(cx).await;
    }
}

async fn settled(window: AnyWindowHandle, view: &Entity<StudyApp>, cx: &mut AsyncApp) {
    until(
        window,
        view,
        "editor and storage settled",
        |app, _| {
            !app.note_saving
                && !app.storage_busy
                && !app.draft_loading
                && app.note_intent.is_none()
                && app.draft_persist.is_none()
                && app.draft_discard.is_none()
        },
        cx,
    )
    .await;
}

async fn bridge(
    window: AnyWindowHandle,
    editor: &Entity<MarkdownEditor>,
    mut action: Value,
    cx: &mut AsyncApp,
) -> Value {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let key = format!(
        "library-smoke-{}",
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    action["request"] = json!(key);
    window
        .update(cx, |_, _, cx| {
            editor.read(cx).test(action).expect("editor test request")
        })
        .unwrap();
    let started = Instant::now();
    loop {
        pause(cx).await;
        if let Some(result) = window
            .update(cx, |_, _, cx| {
                editor.update(cx, |editor, _| {
                    assert!(editor.error().is_none(), "{:?}", editor.error());
                    editor.test_results.remove(&key)
                })
            })
            .unwrap()
        {
            return result;
        }
        assert!(started.elapsed() < TIMEOUT, "editor response: {key}");
    }
}

fn send(window: AnyWindowHandle, view: &Entity<StudyApp>, command: Command, cx: &mut AsyncApp) {
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| app.send(command, cx))
        })
        .unwrap();
}

async fn create_note(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    title: &str,
    folder_id: Option<String>,
    cx: &mut AsyncApp,
) -> Note {
    send(
        window,
        view,
        Command::CreateNoteInFolder {
            title: title.into(),
            content: format!("# {title}\n\n原始内容。\n"),
            folder_id,
        },
        cx,
    );
    until(
        window,
        view,
        "create managed fixture",
        |app, _| {
            app.snapshot
                .as_ref()
                .is_some_and(|s| s.notes.iter().any(|n| n.title == title))
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, _, cx| {
            view.read(cx)
                .snapshot
                .as_ref()
                .unwrap()
                .notes
                .iter()
                .find(|n| n.title == title)
                .unwrap()
                .clone()
        })
        .unwrap()
}

/// Scroll only as far as needed, using the same hit testing as a native click.
fn click_scrolled(window: &mut Window, id: SharedString, scroll: &str, cx: &mut App) {
    for _ in 0..35 {
        window.render_frame(cx);
        let viewport = Bounds::new(point(px(0.), px(0.)), window.viewport_size());
        let container = window.try_find(SharedString::from(scroll.to_owned()));
        let target = window.try_find(id.clone());
        if let Some(target) = target.as_ref()
            && target.visible()
            && viewport.contains(&target.bounds().center())
            && container
                .as_ref()
                .is_some_and(|container| container.bounds().contains(&target.bounds().center()))
        {
            window.click(id, cx);
            return;
        }
        let scroll = if container.is_some_and(|container| {
            container.visible()
                && container.bounds().origin.y + container.bounds().size.height
                    <= viewport.size.height
        }) {
            scroll
        } else {
            "page-scroll"
        };
        let scroll_bounds = window.find(SharedString::from(scroll.to_owned())).bounds();
        let delta = if target
            .as_ref()
            .is_some_and(|target| target.bounds().center().y < scroll_bounds.origin.y.max(px(0.)))
        {
            160.
        } else {
            -160.
        };
        window.scroll(
            SharedString::from(scroll.to_owned()),
            ScrollDelta::Pixels(point(px(0.), px(delta))),
            cx,
        );
    }
    panic!("could not reveal native button {id}");
}

pub async fn exercise(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    database: &Path,
    cx: &mut AsyncApp,
) {
    let started = Instant::now();
    settled(window, view, cx).await;
    let (editor, external_files) = window
        .update(cx, |_, window, cx| {
            window.resize(size(px(1440.), px(1000.)));
            view.update(cx, |app, cx| {
                assert!(app.note_editor.is_none() && app.recoverable_draft.is_none());
                app.page = Page::Notes;
                app.history = false;
                app.search.update(cx, |s, cx| s.set_value("", window, cx));
                app.folders
                    .update(cx, |panel, cx| panel.set_filter(FolderFilter::All, cx));
                cx.notify();
            });
            let app = view.read(cx);
            let external_files = app
                .snapshot
                .as_ref()
                .unwrap()
                .notes
                .iter()
                .filter(|n| !n.managed && !n.missing)
                .map(|n| {
                    (
                        n.id.clone(),
                        PathBuf::from(&n.path),
                        std::fs::read(&n.path).expect("existing external source"),
                    )
                })
                .collect::<Vec<_>>();
            assert!(!external_files.is_empty(), "external source fixture exists");
            (app.note_content.clone(), external_files)
        })
        .unwrap();

    let mut parent_id = None;
    let mut child_id = None;
    for name in ["原生目录冒烟", "长历史与编辑"] {
        send(
            window,
            view,
            Command::CreateFolder {
                name: name.into(),
                parent_id: parent_id.clone(),
            },
            cx,
        );
        until(
            window,
            view,
            "logical folder created",
            |app, _| {
                app.snapshot
                    .as_ref()
                    .unwrap()
                    .folders
                    .iter()
                    .any(|f| f.name == name)
            },
            cx,
        )
        .await;
        let id = window
            .update(cx, |_, _, cx| {
                view.read(cx)
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .folders
                    .iter()
                    .find(|f| f.name == name)
                    .unwrap()
                    .id
                    .clone()
            })
            .unwrap();
        if parent_id.is_none() {
            parent_id = Some(id);
        } else {
            child_id = Some(id);
        }
    }
    let child_id = child_id.unwrap();
    let long_title = "研究".repeat(100);
    let note = create_note(window, view, &long_title, None, cx).await;
    let original_file = std::fs::read(&note.path).unwrap();
    window
        .update(cx, |_, window, cx| {
            window.click("move-note-folder", cx);
            window.click(
                SharedString::from(format!("folder-destination-{child_id}")),
                cx,
            );
            click_scrolled(window, "folder-submit".into(), "folder-action-scroll", cx);
        })
        .unwrap();
    until(
        window,
        view,
        "move note through folder panel",
        |app, _| {
            app.snapshot.as_ref().unwrap().notes.iter().any(|n| {
                n.id == note.id && n.folder_id.as_ref() == Some(&child_id) && n.path == note.path
            })
        },
        cx,
    )
    .await;
    assert_eq!(std::fs::read(&note.path).unwrap(), original_file);
    window
        .update(cx, |_, window, cx| {
            window.click(SharedString::from(format!("folder-row-{child_id}")), cx);
            assert_eq!(
                view.read(cx).folders.read(cx).filter(),
                &FolderFilter::Folder(child_id.clone())
            );
            window.render_frame(cx);
            assert!(
                window
                    .try_find(SharedString::from(format!("note-{}", note.id)))
                    .is_some()
            );
            for (id, _, _) in &external_files {
                assert!(
                    window
                        .try_find(SharedString::from(format!("note-{id}")))
                        .is_none()
                );
            }
            window.click("folders-root", cx);
            assert_eq!(view.read(cx).folders.read(cx).filter(), &FolderFilter::Root);
            window.render_frame(cx);
            assert!(
                window
                    .try_find(SharedString::from(format!("note-{}", note.id)))
                    .is_none()
            );
            window.click("folders-all", cx);
            assert_eq!(view.read(cx).folders.read(cx).filter(), &FolderFilter::All);
        })
        .unwrap();

    let long_content = format!(
        "# {long_title}\n\n{}\n",
        (0..1650)
            .map(|i| format!("新增第 {i:04} 行"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let expected_hash = window
        .update(cx, |_, _, cx| {
            view.read(cx)
                .snapshot
                .as_ref()
                .unwrap()
                .versions
                .iter()
                .rev()
                .find(|v| v.note_id == note.id)
                .unwrap()
                .hash
                .clone()
        })
        .unwrap();
    send(
        window,
        view,
        Command::SaveNote {
            note_id: note.id.clone(),
            expected_hash,
            content: long_content.clone(),
        },
        cx,
    );
    until(
        window,
        view,
        "long history version",
        |app, _| {
            app.snapshot
                .as_ref()
                .unwrap()
                .versions
                .iter()
                .any(|v| v.note_id == note.id && v.content == long_content)
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, window, _| window.resize(size(px(1000.), px(700.))))
        .unwrap();
    for _ in 0..3 {
        pause(cx).await;
    }
    window
        .update(cx, |_, window, cx| {
            assert!(window.viewport_size().width <= px(1001.));
            assert!(window.viewport_size().height <= px(701.));
            window.click("history-tab", cx);
            window.render_frame(cx);
            assert!(
                view.read(cx)
                    .history_preview
                    .scroll
                    .0
                    .borrow()
                    .base_handle
                    .bounds()
                    .size
                    .height
                    > px(0.),
                "long title must leave a scrollable history viewport at 1000x700"
            );
            let app = view.read(cx);
            assert!(app.history_preview.rows.len() > 1500);
            eprintln!(
                "history rows={}, viewport={:?}, items={:?}",
                app.history_preview.rows.len(),
                app.history_preview.scroll.0.borrow().base_handle.bounds(),
                app.history_preview.scroll.0.borrow().last_item_size
            );
            window.scroll(
                "history-diff-scroll",
                ScrollDelta::Pixels(point(px(0.), px(-440.))),
                cx,
            );
            assert!(
                view.read(cx)
                    .history_preview
                    .scroll
                    .0
                    .borrow()
                    .base_handle
                    .offset()
                    .y
                    < px(0.),
                "native wheel scroll must move the diff"
            );
            let app = view.read(cx);
            app.history_preview
                .scroll
                .scroll_to_item(app.history_preview.rows.len() - 1, ScrollStrategy::Bottom);
            window.render_frame(cx);
            window
                .render_to_image()
                .unwrap()
                .save(database.with_file_name("library-long-history.png"))
                .unwrap();
            assert!(
                view.read(cx)
                    .history_preview
                    .scroll
                    .0
                    .borrow()
                    .base_handle
                    .offset()
                    .y
                    < px(0.)
            );
            window
                .render_to_image()
                .unwrap()
                .save(database.with_file_name("library-long-history.png"))
                .unwrap();
            let last_row = view.read(cx).history_preview.rows.len() - 1;
            assert!(window.find(("diff-line", last_row)).visible());
            window.click("edit-note", cx);
            assert!(!view.read(cx).history);
            assert!(view.read(cx).selected_version.is_none());
            assert_eq!(
                view.read(cx).note_editor.as_ref().unwrap().note_id.as_ref(),
                Some(&note.id)
            );
            view.update(cx, |app, cx| {
                app.storage_busy = true;
                app.save_note(window, cx);
                assert!(!app.note_saving);
                assert!(app.note_intent.is_none());
                assert!(app.note_editor.is_some());
                app.storage_busy = false;
                cx.notify();
            });
        })
        .unwrap();
    let before = bridge(window, &editor, json!({"type":"snapshot"}), cx).await;
    assert_eq!(before["readonly"], false);
    assert_eq!(before["text"], long_content);
    let sentinel = "原生输入确认：从历史返回编辑后可继续保存 🧠\n";
    let input = window
        .update(cx, |_, _, cx| {
            editor
                .read(cx)
                .insert_text(sentinel)
                .expect("send actual WebView2 keyboard text")
        })
        .unwrap();
    let input_started = Instant::now();
    loop {
        match input.try_recv() {
            Ok(result) => {
                result.expect("WebView2 Input.insertText");
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                panic!("WebView2 input completion disconnected")
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                assert!(input_started.elapsed() < TIMEOUT, "native input completion");
                pause(cx).await;
            }
        }
    }
    let after = bridge(window, &editor, json!({"type":"snapshot"}), cx).await;
    assert_eq!(after["readonly"], false);
    let saved_content = after["text"].as_str().unwrap().to_owned();
    assert!(saved_content.contains(sentinel));
    assert!(saved_content.contains("新增第 1649 行"));
    window
        .update(cx, |_, window, cx| window.click("save-note", cx))
        .unwrap();
    until(
        window,
        view,
        "save native input after history",
        |app, _| {
            !app.note_saving
                && app.note_editor.is_none()
                && app
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .versions
                    .iter()
                    .any(|v| v.note_id == note.id && v.content == saved_content)
        },
        cx,
    )
    .await;
    assert_eq!(std::fs::read_to_string(&note.path).unwrap(), saved_content);
    window
        .update(cx, |_, window, cx| {
            window.resize(size(px(1440.), px(1000.)));
            window.render_frame(cx);
        })
        .unwrap();

    // Deleting a logical folder that only contains an unsaved draft must leave
    // that draft editable and saveable in the root.
    send(
        window,
        view,
        Command::CreateFolder {
            name: "草稿目录移除测试".into(),
            parent_id: None,
        },
        cx,
    );
    until(
        window,
        view,
        "draft folder created",
        |app, _| {
            app.snapshot
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .any(|f| f.name == "草稿目录移除测试")
        },
        cx,
    )
    .await;
    let draft_folder = window
        .update(cx, |_, window, cx| {
            let id = view
                .read(cx)
                .snapshot
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .find(|f| f.name == "草稿目录移除测试")
                .unwrap()
                .id
                .clone();
            window.click(SharedString::from(format!("folder-row-{id}")), cx);
            window.click("new-note", cx);
            view.update(cx, |app, cx| {
                assert_eq!(
                    app.note_editor.as_ref().unwrap().folder_id.as_ref(),
                    Some(&id)
                );
                app.note_title.update(cx, |title, cx| {
                    title.set_value("目录删除后的草稿", window, cx)
                });
            });
            id
        })
        .unwrap();
    let draft_text = "目录删除后仍保留的草稿内容。\n";
    bridge(
        window,
        &editor,
        json!({"type":"replace","from":0,"to":0,"text":draft_text}),
        cx,
    )
    .await;
    send(
        window,
        view,
        Command::DeleteFolder {
            folder_id: draft_folder.clone(),
        },
        cx,
    );
    until(
        window,
        view,
        "deleted draft folder falls back to root",
        |app, _| {
            app.snapshot
                .as_ref()
                .unwrap()
                .folders
                .iter()
                .all(|f| f.id != draft_folder)
                && app
                    .note_editor
                    .as_ref()
                    .is_some_and(|draft| draft.folder_id.is_none())
        },
        cx,
    )
    .await;
    let draft_after = bridge(window, &editor, json!({"type":"snapshot"}), cx).await;
    assert_eq!(draft_after["text"], draft_text);
    assert_eq!(draft_after["readonly"], false);
    window
        .update(cx, |_, window, cx| window.click("save-note", cx))
        .unwrap();
    until(
        window,
        view,
        "draft saved after its folder was deleted",
        |app, _| {
            app.note_editor.is_none()
                && !app.note_saving
                && app.snapshot.as_ref().unwrap().notes.iter().any(|n| {
                    n.title == "目录删除后的草稿"
                        && n.folder_id.is_none()
                        && std::fs::read_to_string(&n.path).is_ok_and(|text| text == draft_text)
                })
        },
        cx,
    )
    .await;

    let disposable = create_note(window, view, "单份副本清理测试", parent_id.clone(), cx).await;
    let disposable_history = window
        .update(cx, |_, window, cx| {
            let versions = view
                .read(cx)
                .snapshot
                .as_ref()
                .unwrap()
                .versions
                .iter()
                .filter(|v| v.note_id == disposable.id)
                .map(|v| (v.id.clone(), v.content.clone()))
                .collect::<Vec<_>>();
            window.click(("nav", 4_usize), cx);
            click_scrolled(window, "refresh-storage".into(), "page-scroll", cx);
            versions
        })
        .unwrap();
    until(
        window,
        view,
        "space report refreshed",
        |app, _| {
            !app.storage_busy
                && app.storage_report.as_ref().is_some_and(|r| {
                    r.copies
                        .iter()
                        .any(|copy| copy.note_id == disposable.id && copy.deletable)
                })
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            let report = view.read(cx).storage_report.as_ref().unwrap();
            assert!(report.database_bytes > 0 && report.managed_bytes > 0);
            assert!(
                external_files
                    .iter()
                    .all(|(id, _, _)| { report.copies.iter().all(|copy| &copy.note_id != id) })
            );
            click_scrolled(
                window,
                format!("delete-copy-{}", disposable.id).into(),
                "storage-copy-list",
                cx,
            );
            assert_eq!(view.read(cx).storage_confirm.as_ref(), Some(&disposable.id));
            click_scrolled(
                window,
                "confirm-delete-copy".into(),
                "storage-copy-list",
                cx,
            );
        })
        .unwrap();
    until(
        window,
        view,
        "single managed copy deleted",
        |app, _| {
            !app.storage_busy
                && app
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .notes
                    .iter()
                    .any(|n| n.id == disposable.id && n.detached)
                && !Path::new(&disposable.path).exists()
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, _, cx| {
            let snapshot = view.read(cx).snapshot.as_ref().unwrap();
            for (id, content) in &disposable_history {
                assert!(
                    snapshot
                        .versions
                        .iter()
                        .any(|v| &v.id == id && &v.content == content)
                );
            }
            assert_eq!(
                snapshot
                    .notes
                    .iter()
                    .find(|n| n.id == note.id)
                    .unwrap()
                    .folder_id
                    .as_ref(),
                Some(&child_id)
            );
        })
        .unwrap();
    for (_, path, bytes) in &external_files {
        assert_eq!(&std::fs::read(path).unwrap(), bytes);
    }
    assert_eq!(std::fs::read_to_string(&note.path).unwrap(), saved_content);

    settled(window, view, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.notice = None;
                app.new_note(window, cx);
            });
            window.render_frame(cx);
        })
        .unwrap();
    let migration_draft = "迁移前已确认并落盘的最后输入 🧠";
    bridge(
        window,
        &editor,
        json!({"type":"replace","from":0,"to":0,"text":migration_draft}),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| window.click("keep-note-draft", cx))
        .unwrap();
    settled(window, view, cx).await;
    let original_database = std::fs::canonicalize(database).unwrap();
    let original_directory = original_database.parent().unwrap().to_path_buf();
    let target = original_directory.with_file_name(format!(
        "studyapp-native-migrated-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&target).expect("fresh migration destination");
    window
        .update(cx, |_, _, cx| {
            view.update(cx, |app, cx| {
                assert!(app.note_editor.is_none());
                app.storage_work(Work::MigrateLibrary(target.clone()), cx);
            })
        })
        .unwrap();
    until(
        window,
        view,
        "migration switched to copied library",
        |app, _| {
            !app.storage_busy
                && !app.draft_loading
                && Path::new(&app.last_library).parent() == Some(target.as_path())
        },
        cx,
    )
    .await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert_eq!(
                app.recoverable_draft.as_ref().unwrap().content,
                migration_draft
            );
            let snapshot = app.snapshot.as_ref().unwrap();
            let migrated = snapshot.notes.iter().find(|n| n.id == note.id).unwrap();
            assert!(Path::new(&migrated.path).starts_with(&target));
            assert_eq!(migrated.folder_id.as_ref(), Some(&child_id));
            assert_eq!(
                std::fs::read_to_string(&migrated.path).unwrap(),
                saved_content
            );
            assert!(
                snapshot
                    .folders
                    .iter()
                    .any(|f| f.id == child_id && f.parent_id == parent_id)
            );
            assert!(
                snapshot
                    .versions
                    .iter()
                    .any(|v| v.note_id == note.id && v.content == saved_content)
            );
            assert!(
                snapshot
                    .notes
                    .iter()
                    .any(|n| n.id == disposable.id && n.detached)
            );
            for (id, path, bytes) in &external_files {
                let external = snapshot.notes.iter().find(|n| &n.id == id).unwrap();
                assert_eq!(Path::new(&external.path), path);
                assert_eq!(&std::fs::read(path).unwrap(), bytes);
            }
            app.worker
                .sender
                .send(Work::SwitchLibrary(original_directory.clone()))
                .unwrap();
        })
        .unwrap();
    until(
        window,
        view,
        "return to original library for close recovery",
        |app, _| {
            Path::new(&app.last_library) == original_database
                && !app.draft_loading
                && !app.storage_busy
        },
        cx,
    )
    .await;
    assert!(original_database.is_file());
    assert_eq!(std::fs::read_to_string(&note.path).unwrap(), saved_content);
    window
        .update(cx, |_, window, cx| {
            assert_eq!(
                view.read(cx).recoverable_draft.as_ref().unwrap().content,
                migration_draft
            );
            view.update(cx, |app, cx| app.begin_discard(window, cx));
        })
        .unwrap();
    settled(window, view, cx).await;
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.page = Page::Notes;
                app.history = false;
                app.selected_note = Some(note.id.clone());
                app.selected_version = None;
                app.folders
                    .update(cx, |panel, cx| panel.set_filter(FolderFilter::All, cx));
                assert!(app.note_editor.is_none() && app.recoverable_draft.is_none());
                cx.notify();
            });
            window.render_frame(cx);
        })
        .unwrap();
    std::fs::write(
        database.with_file_name("library-smoke-result.txt"),
        format!(
            "PASS: nested folders, native move/filter buttons, >1500 diff rows scrolled, busy save rejected without wedging the editor, native WebView2 input and save, deleted draft folder fallback and save, storage refresh and individual confirmed deletion, preserved history/external sources, migration and return.\nElapsed: {:?}\nMigration copy retained: {}\n",
            started.elapsed(), target.display()
        ),
    )
    .unwrap();
    eprintln!("native library smoke passed in {:?}", started.elapsed());
}
