use serde_json::{Value, json};
use studyapp_core::{Command, Engine, LayoutSettings, Settings};

#[test]
fn legacy_settings_restore_default_workspace_layout() {
    let settings: Settings = serde_json::from_value(json!({
        "interval_minutes": 45, "answer_minutes": 3, "review_days": [1, 5]
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(settings).unwrap()["layout"],
        json!({
            "navigation_width": 184, "folders_width": 176, "notes_width": 240,
            "navigation_visible": true
        })
    );
}

#[test]
fn workspace_layout_persists_without_changing_learning_or_path_settings() {
    let directory = tempfile::tempdir().unwrap().keep();
    let database = directory.join("layout.sqlite");
    let layout = LayoutSettings {
        navigation_width: 256,
        folders_width: 240,
        notes_width: 320,
        navigation_visible: false,
    };
    for layout_first in [false, true] {
        let mut engine = Engine::open(&database, 1000).unwrap();
        let commands = if layout_first {
            vec![
                Command::SaveLayout(layout.clone()),
                learning_update(30, 8, vec![2, 6]),
                Command::SetShowFilePaths(true),
            ]
        } else {
            vec![
                learning_update(30, 8, vec![2, 6]),
                Command::SetShowFilePaths(true),
                Command::SaveLayout(layout.clone()),
            ]
        };
        for command in commands {
            engine.apply(command, 1000).unwrap();
        }
        engine.tick(1001, 1).unwrap();
        let timer = engine.snapshot().timer;
        engine
            .apply(Command::SaveLayout(layout.clone()), 1001)
            .unwrap();
        assert_eq!(engine.snapshot().timer.active_seconds, timer.active_seconds);
        assert_eq!(engine.snapshot().timer.target_seconds, timer.target_seconds);
        drop(engine);
        let restored = Engine::open(&database, 1001).unwrap().snapshot();
        assert_eq!(restored.settings.layout, layout);
        assert_eq!(restored.settings.interval_minutes, 30);
        assert_eq!(restored.settings.answer_minutes, 8);
        assert_eq!(restored.settings.review_days, vec![2, 6]);
        assert!(restored.settings.show_file_paths);
    }
}

#[test]
fn invalid_workspace_widths_are_rejected_without_persisting_changes() {
    let directory = tempfile::tempdir().unwrap().keep();
    let database = directory.join("invalid-layout.sqlite");
    let mut engine = Engine::open(&database, 1000).unwrap();
    let original = engine.snapshot().settings;
    for (navigation_width, folders_width, notes_width) in [
        (143, 176, 240),
        (321, 176, 240),
        (184, 143, 240),
        (184, 361, 240),
        (184, 176, 179),
        (184, 176, 441),
        (u32::MAX, 176, 240),
    ] {
        let layout = LayoutSettings {
            navigation_width,
            folders_width,
            notes_width,
            navigation_visible: false,
        };
        assert!(
            engine
                .apply(Command::SaveLayout(layout.clone()), 1001)
                .is_err()
        );
        let mut full = original.clone();
        full.layout = layout;
        assert!(engine.apply(Command::SaveSettings(full), 1001).is_err());
        assert_eq!(engine.snapshot().settings, original);
    }
    drop(engine);
    assert_eq!(
        Engine::open(&database, 1001).unwrap().snapshot().settings,
        original
    );
}

#[test]
fn new_settings_hide_file_paths_by_default() {
    let encoded = serde_json::to_value(Settings::default()).unwrap();
    assert_eq!(encoded["show_file_paths"], false);
}

#[test]
fn legacy_settings_without_path_visibility_default_to_hidden() {
    let legacy = json!({
        "interval_minutes": 45,
        "answer_minutes": 3,
        "review_days": [1, 5, 12]
    });
    let settings: Settings = serde_json::from_value(legacy).unwrap();
    let encoded = serde_json::to_value(settings).unwrap();
    assert_eq!(encoded["show_file_paths"], false);
    assert_eq!(encoded["interval_minutes"], 45);
    assert_eq!(encoded["answer_minutes"], 3);
    assert_eq!(encoded["review_days"], json!([1, 5, 12]));
}

#[test]
fn file_path_visibility_persists_when_enabled_and_disabled() {
    let directory = tempfile::tempdir().unwrap().keep();
    let database = directory.join("learning.sqlite");
    let mut engine = Engine::open(&database, 1000).unwrap();

    for (index, visible) in [true, false].into_iter().enumerate() {
        let now = 1001 + index as i64;
        let mut encoded = serde_json::to_value(engine.snapshot().settings).unwrap();
        encoded["show_file_paths"] = json!(visible);
        let settings = serde_json::from_value(encoded).unwrap();
        engine.apply(Command::SaveSettings(settings), now).unwrap();
        drop(engine);
        engine = Engine::open(&database, now).unwrap();
        let restored = serde_json::to_value(engine.snapshot().settings).unwrap();
        assert_eq!(restored["show_file_paths"], visible);
    }
}

#[test]
fn opening_legacy_database_preserves_settings_and_hides_paths() {
    let directory = tempfile::tempdir().unwrap().keep();
    let database = directory.join("learning.sqlite");
    drop(Engine::open(&database, 1000).unwrap());
    let connection = rusqlite::Connection::open(&database).unwrap();
    let raw: String = connection
        .query_row("SELECT data FROM app_state WHERE id=1", [], |row| {
            row.get(0)
        })
        .unwrap();
    let mut state: Value = serde_json::from_str(&raw).unwrap();
    state["settings"]
        .as_object_mut()
        .unwrap()
        .remove("show_file_paths");
    state["settings"]["answer_minutes"] = json!(8);
    connection
        .execute(
            "UPDATE app_state SET data=?1 WHERE id=1",
            [serde_json::to_string(&state).unwrap()],
        )
        .unwrap();
    drop(connection);

    let engine = Engine::open(&database, 1001).unwrap();
    let restored = serde_json::to_value(engine.snapshot().settings).unwrap();
    assert_eq!(restored["show_file_paths"], false);
    assert_eq!(restored["answer_minutes"], 8);
}

fn learning_update(interval_minutes: u32, answer_minutes: u32, review_days: Vec<u32>) -> Command {
    Command::SaveLearningSettings {
        interval_minutes,
        answer_minutes,
        review_days,
    }
}

fn rapid_updates(learning_first: bool) {
    for initially_visible in [false, true] {
        let directory = tempfile::tempdir().unwrap().keep();
        let database = directory.join("learning.sqlite");
        let mut engine = Engine::open(&database, 1000).unwrap();
        let mut initial = engine.snapshot().settings;
        initial.show_file_paths = initially_visible;
        engine.apply(Command::SaveSettings(initial), 1000).unwrap();
        // Queue both intentions before accepting a refreshed worker snapshot.
        let learning = learning_update(30, 8, vec![2, 6, 15]);
        let visibility = Command::SetShowFilePaths(!initially_visible);
        let commands = if learning_first {
            [learning, visibility]
        } else {
            [visibility, learning]
        };
        for command in commands {
            engine.apply(command, 1001).unwrap();
        }
        let expected = Settings {
            interval_minutes: 30,
            answer_minutes: 8,
            review_days: vec![2, 6, 15],
            show_file_paths: !initially_visible,
            layout: Default::default(),
        };
        assert_eq!(
            engine.snapshot().settings,
            expected,
            "queued setting intent was lost"
        );
        assert_eq!(engine.snapshot().timer.target_seconds, 30 * 60);
        drop(engine);
        let restored = Engine::open(&database, 1002).unwrap();
        assert_eq!(
            restored.snapshot().settings,
            expected,
            "merged settings did not persist"
        );
    }
}

#[test]
fn rapid_learning_then_visibility_updates_preserve_both_intentions() {
    rapid_updates(true);
}

#[test]
fn rapid_visibility_then_learning_updates_preserve_both_intentions() {
    rapid_updates(false);
}

#[test]
fn invalid_learning_update_preserves_visibility_and_all_saved_fields() {
    let directory = tempfile::tempdir().unwrap().keep();
    let database = directory.join("learning.sqlite");
    let mut engine = Engine::open(&database, 1000).unwrap();
    engine.apply(Command::SetShowFilePaths(true), 1001).unwrap();
    let saved = engine.snapshot().settings;
    for (interval, answer, days) in [(4, 5, vec![1]), (60, 31, vec![1]), (60, 5, vec![0])] {
        assert!(
            engine
                .apply(learning_update(interval, answer, days), 1002)
                .is_err()
        );
        assert_eq!(engine.snapshot().settings, saved);
        assert_eq!(engine.snapshot().timer.target_seconds, 3600);
    }
    drop(engine);
    assert_eq!(
        Engine::open(&database, 1003).unwrap().snapshot().settings,
        saved
    );
}

#[test]
fn learning_update_preserves_current_timer_and_changes_the_next_cycle() {
    let directory = tempfile::tempdir().unwrap().keep();
    let mut engine = Engine::open(&directory.join("learning.sqlite"), 1000).unwrap();
    engine.apply(learning_update(5, 5, vec![1]), 1000).unwrap();
    engine.tick(1001, 1).unwrap();
    engine.apply(learning_update(10, 2, vec![2]), 1001).unwrap();
    engine.apply(Command::SetShowFilePaths(true), 1001).unwrap();
    assert_eq!(engine.snapshot().timer.active_seconds, 1);
    assert_eq!(engine.snapshot().timer.target_seconds, 300);
    for now in (1011..=1301).step_by(10) {
        engine.tick(now, 10).unwrap();
    }
    let snapshot = engine.snapshot();
    assert_eq!(snapshot.timer.active_seconds, 1);
    assert_eq!(snapshot.timer.target_seconds, 600);
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.sessions[0].ended_at, 1300);
    assert_eq!(snapshot.sessions[0].deadline, 1420);
    assert!(snapshot.settings.show_file_paths);
}
