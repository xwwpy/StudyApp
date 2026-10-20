//! Native WebView/GPUI persistence integration. Browser tests cover real keyboard
//! selection and IME; these transactions cross the production IPC bridge.
use crate::{app::StudyApp, markdown_editor::MarkdownEditor};
use gpui_kit::{test::TestWindowExt, *};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

async fn pause(cx: &AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(50))
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
        "smoke-{}",
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    action["request"] = json!(key);
    window
        .update(cx, |_, _, cx| {
            editor
                .read(cx)
                .test(action)
                .expect("dispatch browser transaction")
        })
        .unwrap();
    for _ in 0..200 {
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
    }
    panic!("browser transaction timed out: {key}");
}

async fn settled(window: AnyWindowHandle, view: &Entity<StudyApp>, cx: &mut AsyncApp) {
    for _ in 0..200 {
        pause(cx).await;
        if window
            .update(cx, |_, _, cx| {
                let app = view.read(cx);
                !app.note_saving && app.draft_discard.is_none() && !app.draft_loading
            })
            .unwrap()
        {
            return;
        }
    }
    panic!("native note operation timed out");
}

pub async fn exercise(
    window: AnyWindowHandle,
    view: &Entity<StudyApp>,
    database: &Path,
    cx: &mut AsyncApp,
) -> String {
    let editor = window
        .update(cx, |_, window, cx| {
            assert_eq!(view.read(cx).snapshot.as_ref().unwrap().notes.len(), 2);
            window.click("new-note", cx);
            view.update(cx, |app, cx| {
                app.note_title
                    .update(cx, |s, cx| s.set_value("连续编辑测试", window, cx))
            });
            view.read(cx).note_content.clone()
        })
        .unwrap();
    for attempt in 0..400 {
        pause(cx).await;
        if window
            .update(cx, |_, _, cx| {
                assert!(
                    editor.read(cx).error().is_none(),
                    "{:?}",
                    editor.read(cx).error()
                );
                editor.read(cx).ready()
            })
            .unwrap()
        {
            break;
        }
        assert!(attempt < 399, "native WebView initialization timed out");
    }
    let first = "# 连续编辑测试\n\n第一次保存，**加粗知识点**与中文 🧠。\n\n```rust\nfn main() { println!(\"StudyApp\"); }\n```\n";
    bridge(
        window,
        &editor,
        json!({"type":"replace","from":0,"to":0,"text":first}),
        cx,
    )
    .await;
    let rendered = bridge(window, &editor, json!({"type":"snapshot"}), cx).await;
    assert_eq!(rendered["text"], first);
    assert_eq!(rendered["source"], false);
    window
        .update(cx, |_, _, cx| editor.read(cx).inject_test_error(true))
        .unwrap();
    pause(cx).await;
    assert_eq!(
        bridge(window, &editor, json!({"type":"snapshot"}), cx).await["text"],
        first
    );
    window
        .update(cx, |_, window, cx| {
            window.resize(size(px(1000.), px(700.)));
            window.render_frame(cx);
            editor.read(cx).inject_test_error(false);
        })
        .unwrap();
    for _ in 0..5 {
        pause(cx).await;
    }
    window
        .update(cx, |_, window, cx| {
            assert!(editor.read(cx).error().is_some());
            window.render_frame(cx);
            window.click("recover-editor-bridge", cx);
        })
        .unwrap();
    for _ in 0..10 {
        pause(cx).await;
    }
    assert_eq!(
        bridge(window, &editor, json!({"type":"snapshot"}), cx).await["text"],
        first
    );
    window
        .update(cx, |_, window, cx| {
            window.resize(size(px(1280.), px(860.)));
            window.render_frame(cx);
        })
        .unwrap();
    assert!(
        rendered["highlights"].as_u64().unwrap_or_default() > 0,
        "{rendered}"
    );
    // A single selection spans paragraphs; formatting and history stay in one editor.
    bridge(
        window,
        &editor,
        json!({"type":"select","anchor":2,"head":20}),
        cx,
    )
    .await;
    bridge(window, &editor, json!({"type":"command","name":"bold"}), cx).await;
    bridge(window, &editor, json!({"type":"command","name":"undo"}), cx).await;
    bridge(
        window,
        &editor,
        json!({"type":"command","name":"source"}),
        cx,
    )
    .await;
    let back = bridge(
        window,
        &editor,
        json!({"type":"command","name":"source"}),
        cx,
    )
    .await;
    assert_eq!(back["text"], first);
    bridge(window, &editor, json!({"type":"select","anchor":first.encode_utf16().count(),"head":first.encode_utf16().count()}), cx).await;
    let capture = window
        .update(cx, |_, _, cx| {
            editor
                .read(cx)
                .capture_png(database.with_file_name("note-editor.png"))
                .unwrap()
        })
        .unwrap();
    for attempt in 0..100 {
        pause(cx).await;
        if let Ok(result) = capture.try_recv() {
            result.unwrap();
            break;
        }
        assert!(attempt < 99, "WebView screenshot timeout");
    }
    window
        .update(cx, |_, window, cx| window.click("save-note", cx))
        .unwrap();
    settled(window, view, cx).await;
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            editor.read(cx).inject_test_error(false);
        })
        .unwrap();
    for _ in 0..5 {
        pause(cx).await;
    }
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            window.click("recover-editor-bridge", cx);
        })
        .unwrap();
    for _ in 0..10 {
        pause(cx).await;
    }
    assert_eq!(
        bridge(window, &editor, json!({"type":"snapshot"}), cx).await["readonly"],
        true
    );
    let (id, path) = window
        .update(cx, |_, window, cx| {
            let app = view.read(cx);
            assert!(app.note_editor.is_none(), "{:?}", app.notice);
            let id = app.selected_note.clone().unwrap();
            let note = app
                .snapshot
                .as_ref()
                .unwrap()
                .notes
                .iter()
                .find(|n| n.id == id)
                .unwrap();
            assert!(note.managed);
            let path = note.path.clone();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), first);
            window.click("edit-note", cx);
            let crlf = "# 中文 🧠\r\n\r\n第二次保存，保留历史。\r\n";
            editor.update(cx, |e, cx| e.set_value(crlf.into(), window, cx));
            (id, path)
        })
        .unwrap();
    bridge(
        window,
        &editor,
        json!({"type":"command","name":"source"}),
        cx,
    )
    .await;
    bridge(
        window,
        &editor,
        json!({"type":"command","name":"source"}),
        cx,
    )
    .await;
    // UTF-16 offset lands after the emoji, without normalizing untouched CRLF bytes.
    bridge(
        window,
        &editor,
        json!({"type":"replace","from":7,"to":7,"text":" 与高亮"}),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| window.click("keep-note-draft", cx))
        .unwrap();
    settled(window, view, cx).await;
    window
        .update(cx, |_, window, cx| {
            assert!(view.read(cx).recoverable_draft.is_some());
            window.click("recover-note-draft", cx);
            assert_eq!(
                editor.read(cx).value(),
                "# 中文 🧠 与高亮\r\n\r\n第二次保存，保留历史。\r\n"
            );
            window.click("save-note", cx);
        })
        .unwrap();
    settled(window, view, cx).await;
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# 中文 🧠 与高亮\r\n\r\n第二次保存，保留历史。\r\n"
    );

    // Discard an active draft, then discard a separately persisted recovery draft.
    for recovered in [false, true] {
        window
            .update(cx, |_, window, cx| window.click("new-note", cx))
            .unwrap();
        bridge(
            window,
            &editor,
            json!({"type":"replace","from":0,"to":0,"text":"删除的草稿，不应复活"}),
            cx,
        )
        .await;
        if recovered {
            window
                .update(cx, |_, window, cx| window.click("keep-note-draft", cx))
                .unwrap();
            settled(window, view, cx).await;
        }
        window
            .update(cx, |_, window, cx| {
                window.click(
                    if recovered {
                        "discard-recovered-draft"
                    } else {
                        "discard-note-draft"
                    },
                    cx,
                );
                window.click("confirm-note-action", cx);
            })
            .unwrap();
        settled(window, view, cx).await;
        window
            .update(cx, |_, _, cx| {
                assert!(view.read(cx).note_editor.is_none());
                assert!(view.read(cx).recoverable_draft.is_none());
            })
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(database.with_file_name("note-draft.json")).unwrap(),
            "null"
        );
    }
    window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| {
                app.selected_note = Some(id.clone());
                app.reader_key.clear();
                cx.notify();
            });
            window.render_frame(cx);
            window.click("unmanage-note", cx);
            window.click("confirm-note-action", cx);
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(700))
        .await;
    window
        .update(cx, |_, _, cx| {
            let app = view.read(cx);
            assert!(
                app.snapshot
                    .as_ref()
                    .unwrap()
                    .notes
                    .iter()
                    .find(|n| n.id == id)
                    .unwrap()
                    .detached
            );
            assert!(Path::new(&path).is_file());
            view.update(cx, |app, cx| {
                app.send(
                    studyapp_core::Command::Import(vec![path.clone().into()]),
                    cx,
                )
            });
        })
        .unwrap();
    cx.background_executor()
        .timer(Duration::from_millis(700))
        .await;
    window
        .update(cx, |_, _, cx| {
            let snapshot = view.read(cx).snapshot.as_ref().unwrap();
            assert!(!snapshot.notes.iter().find(|n| n.id == id).unwrap().detached);
            assert_eq!(
                snapshot.versions.iter().filter(|v| v.note_id == id).count(),
                2
            );
        })
        .unwrap();
    id
}

/// The PowerShell runner verifies this last draft after normal window shutdown.
pub async fn close_with_draft(window: AnyWindowHandle, view: &Entity<StudyApp>, cx: &mut AsyncApp) {
    let editor = window
        .update(cx, |_, window, cx| {
            view.update(cx, |app, cx| app.new_note(window, cx));
            window.render_frame(cx);
            view.read(cx).note_content.clone()
        })
        .unwrap();
    bridge(
        window,
        &editor,
        json!({"type":"replace","from":0,"to":0,"text":"关闭窗口前的末次输入 🧠"}),
        cx,
    )
    .await;
    window
        .update(cx, |_, window, cx| {
            assert!(
                !view.update(cx, |app, cx| app.handle_close(window, cx)),
                "close must wait for the editor and disk acknowledgements"
            );
        })
        .unwrap();
}
