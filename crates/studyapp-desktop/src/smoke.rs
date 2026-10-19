use crate::app::{Page, StudyApp};
use gpui_kit::*;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use studyapp_core::{Command, Engine};

pub fn prepare(database: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        !database.exists(),
        "冒烟测试必须使用新的空目录，以免改变已有资料"
    );
    let folder = database
        .parent()
        .ok_or_else(|| anyhow::anyhow!("无效测试目录"))?;
    anyhow::ensure!(
        std::fs::read_dir(folder)?.next().transpose()?.is_none(),
        "冒烟测试必须使用新的空目录，以免改变已有资料"
    );
    let note = folder.join("example.md");
    std::fs::write(
        &note,
        "# Rust 所有权\n\n每个值都有一个所有者。\n\n- 借用允许访问数据\n- 生命周期约束引用\n\n```rust\nlet name = String::from(\"StudyApp\");\n```\n\n| 主题 | 状态 |\n| --- | --- |\n| 所有权 | 已学习 |\n",
    )?;
    let now = chrono::Utc::now().timestamp();
    let mut engine = Engine::open(database, now)?;
    engine.apply(Command::Import(vec![note]), now)?;
    let id = engine.snapshot().notes[0].id.clone();
    engine.apply(
        Command::Manual {
            version_ids: None,
            started_at: now - 172800,
            minutes: 45,
            summary: "理解 Rust 所有权与借用规则".into(),
            note_ids: vec![id],
        },
        now,
    )?;
    engine.checkpoint()?;
    Ok(())
}

pub fn exercise(window: AnyWindowHandle, view: Entity<StudyApp>, database: PathBuf, cx: &mut App) {
    cx.spawn(async move |cx| {
        cx.background_executor().timer(Duration::from_secs(2)).await;
        for (index, page) in [Page::Today, Page::Notes, Page::Reviews, Page::Stats, Page::Settings].into_iter().enumerate() {
            let result = window.update(cx, |_, _, cx| {
                view.update(cx, |app, cx| {
                    app.page = page;
                    if page == Page::Notes {
                        app.selected_note = app.snapshot.as_ref().and_then(|s| s.notes.first().map(|n| n.id.clone()));
                        app.refresh_preview();
                    }
                    cx.notify();
                });
            });
            if let Err(error) = result { eprintln!("smoke window failed: {error}"); std::process::exit(2); }
            cx.background_executor().timer(Duration::from_millis(700)).await;
            #[cfg(feature = "ui-test")]
            {
                let screenshot = database.with_file_name(format!("page-{index}.png"));
                let capture = window.update(cx, |_, window, _| -> anyhow::Result<()> {
                    window.render_to_image()?.save(screenshot)?;
                    Ok(())
                });
                if !matches!(capture, Ok(Ok(()))) { eprintln!("screenshot failed: {capture:?}"); std::process::exit(3); }
            }
            #[cfg(not(feature = "ui-test"))]
            let _ = index;
        }
        #[cfg(feature = "ui-test")]
        {
            use gpui_kit::test::TestWindowExt;
            window.update(cx, |_, window, cx| {
                window.click(("nav", 2_usize), cx);
                assert_eq!(view.read(cx).page, Page::Reviews);
                let review = view.read(cx).snapshot.as_ref().expect("loaded snapshot").reviews[0].clone();
                window.click(SharedString::from(format!("review-check-{}", review.id)), cx);
            }).expect("review click");
            cx.background_executor().timer(Duration::from_secs(1)).await;
            window.update(cx, |_, window, cx| {
                assert_eq!(view.read(cx).snapshot.as_ref().unwrap().review_events.len(), 1);
                window.click(("review-filter", 1_usize), cx);
                let review = view.read(cx).snapshot.as_ref().unwrap().reviews[0].clone();
                window.click(SharedString::from(format!("undo-{}", review.id)), cx);
            }).expect("undo click");
            cx.background_executor().timer(Duration::from_secs(1)).await;
            window.update(cx, |_, window, cx| {
                assert!(view.read(cx).snapshot.as_ref().unwrap().review_events.is_empty());
                window.click(("nav", 0_usize), cx);
                window.click("pause", cx);
            }).expect("pause click");
            cx.background_executor().timer(Duration::from_secs(1)).await;
            window.update(cx, |_, window, cx| {
                assert!(view.read(cx).snapshot.as_ref().unwrap().timer.paused);
                window.click(("nav", 4_usize), cx);
                view.update(cx, |app, cx| app.interval.update(cx, |s, cx| s.set_value("25", window, cx)));
                window.click("save-settings", cx);
            }).expect("settings click");
            cx.background_executor().timer(Duration::from_secs(1)).await;
            window.update(cx, |_, _, cx| {
                assert_eq!(view.read(cx).snapshot.as_ref().unwrap().settings.interval_minutes, 25);
            }).expect("settings persisted");
            let backup = database.with_file_name("smoke-backup.sqlite3");
            window.update(cx, |_, _, cx| {
                view.update(cx, |app, cx| app.send(Command::Backup(backup.clone()), cx));
            }).expect("backup command");
            cx.background_executor().timer(Duration::from_secs(1)).await;
            assert!(backup.exists());
            window.update(cx, |_, window, cx| {
                view.update(cx, |app, cx| app.interval.update(cx, |s, cx| s.set_value("10", window, cx)));
                window.click("save-settings", cx);
            }).expect("temporary settings");
            cx.background_executor().timer(Duration::from_secs(1)).await;
            window.update(cx, |_, window, cx| {
                assert_eq!(view.read(cx).snapshot.as_ref().unwrap().settings.interval_minutes, 10);
                view.update(cx, |app, cx| {
                    app.open_editor(crate::app::EditorMode::Manual, window, cx);
                    app.send(Command::Restore(backup), cx);
                });
            }).expect("restore command");
            cx.background_executor().timer(Duration::from_secs(1)).await;
            window.update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert_eq!(app.snapshot.as_ref().unwrap().settings.interval_minutes, 25);
                assert_eq!(app.interval.read(cx).value().as_ref(), "25");
                assert!(app.editor.is_none());
            }).expect("restore refreshed interface");
            window.update(cx, |_, window, cx| {
                window.click(("nav", 0_usize), cx);
                window.click("manual-top", cx);
                let version = view.read(cx).snapshot.as_ref().unwrap().versions[0].id.clone();
                assert!(view.read(cx).chosen_versions.contains(&version));
                window.click(SharedString::from(format!("associate-version-{version}")), cx);
                assert!(!view.read(cx).chosen_versions.contains(&version));
                view.update(cx, |app, cx| app.summary.update(cx, |s, cx| s.set_value("确认阅读了选定的所有权快照", window, cx)));
                window.click("save-learning", cx);
            }).expect("version selection validation");
            cx.background_executor().timer(Duration::from_secs(1)).await;
            window.update(cx, |_, window, cx| {
                assert!(view.read(cx).editor.is_some());
                assert!(view.read(cx).notice.as_ref().is_some_and(Result::is_err));
                let version = view.read(cx).snapshot.as_ref().unwrap().versions[0].id.clone();
                window.click(SharedString::from(format!("associate-version-{version}")), cx);
                window.render_to_image().unwrap().save(database.with_file_name("learning-editor.png")).unwrap();
                window.click("save-learning", cx);
            }).expect("save selected version");
            cx.background_executor().timer(Duration::from_secs(1)).await;
            window.update(cx, |_, _, cx| {
                let app = view.read(cx);
                assert!(app.editor.is_none());
                let snapshot = app.snapshot.as_ref().unwrap();
                assert_eq!(snapshot.sessions.len(), 2);
                let session = snapshot.sessions.iter().find(|s| s.summary == "确认阅读了选定的所有权快照").unwrap();
                assert_eq!(session.version_ids, vec![snapshot.versions[0].id.clone()]);
                assert_eq!(snapshot.reviews.len(), 2);
            }).expect("saved learning snapshot association");
        }
        let report = database.with_file_name("smoke-result.txt");
        std::fs::write(report, "PASS: five native pages rendered; ui-test additionally captured screenshots and exercised navigation, review checkbox, undo, pause, settings save, backup restore refresh, learning save and version validation.\n").expect("write smoke report");
        cx.update(|cx| cx.quit());
    }).detach();
}

#[cfg(test)]
mod tests {
    use super::prepare;
    use studyapp_core::Engine;

    #[test]
    fn prepare_rejects_nonempty_directory_without_overwriting_notes_or_creating_database() {
        let directory = tempfile::tempdir().unwrap().keep();
        let note = directory.join("example.md");
        let original = "# 用户原有笔记\n保留这些内容。\n";
        std::fs::write(&note, original).unwrap();
        let database = directory.join("studyapp.sqlite3");

        assert!(prepare(&database).is_err());
        assert_eq!(std::fs::read_to_string(&note).unwrap(), original);
        assert!(!database.exists());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    }

    #[test]
    fn prepare_accepts_an_existing_empty_directory() {
        let directory = tempfile::tempdir().unwrap().keep();
        let database = directory.join("studyapp.sqlite3");

        prepare(&database).unwrap();

        assert!(database.is_file());
        assert!(directory.join("example.md").is_file());
        let engine = Engine::open(&database, chrono::Utc::now().timestamp()).unwrap();
        assert_eq!(engine.snapshot().notes.len(), 1);
        assert_eq!(engine.snapshot().sessions.len(), 1);
    }
}
