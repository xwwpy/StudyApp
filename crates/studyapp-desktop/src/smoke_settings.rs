//! Settings categories and recorded-key native-window regressions.
use crate::{
    app::{Page, StudyApp},
    settings_ui::SettingsSection,
    smoke_library::{click_scrolled, reveal_scrolled, settled, until},
};
use gpui_kit::{test::TestWindowExt, *};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn settings_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn read(root: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                read(&path, result);
            } else {
                result.insert(path.clone(), std::fs::read(path).unwrap());
            }
        }
    }
    let mut result = BTreeMap::new();
    read(root, &mut result);
    result
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
            crate::smoke_library::resize(window, size(px(1280.), px(900.)));
            window.click(("nav", 4_usize), cx);
            window.render_frame(cx);
            view.update(cx, |app, cx| app.clear_notices(cx));
            let original = view.read(cx).preferences_ui.saved.as_ref().unwrap().clone();
            let files_root = database.parent().unwrap().join("settings");
            let files = settings_files(&files_root);
            let before_note_editor = view.read(cx).note_editor.is_some();
            let before_selected = view.read(cx).selected_note.clone();
            let before_versions = view.read(cx).snapshot.as_ref().unwrap().versions.len();

            window.click("settings-section-learning", cx);
            let interval = view.read(cx).interval.clone();
            interval.update(cx, |input, cx| input.set_value("35", window, cx));
            window.click("settings-section-appearance", cx);
            click_scrolled(
                window,
                "show-file-paths".into(),
                "settings-section-scroll",
                cx,
            );
            for (section, id, _) in SettingsSection::ALL {
                window.click(SharedString::from(format!("settings-section-{id}")), cx);
                window.render_frame(cx);
                let app = view.read(cx);
                assert!(app.preferences_ui.section == section);
                assert_eq!(
                    app.interval.read(cx).value().as_str(),
                    "35",
                    "category switch discarded Learning input"
                );
                assert_eq!(
                    app.preferences_ui.draft.as_ref().unwrap().show_file_paths,
                    !original.settings.show_file_paths
                );
                assert_eq!(
                    app.preferences_ui.saved.as_ref().unwrap().revision,
                    original.revision
                );
                assert!(app.settings_dirty(cx));
                assert_eq!(
                    settings_files(&files_root),
                    files,
                    "category switching wrote configuration files"
                );
            }

            window.click("settings-section-shortcuts", cx);
            click_scrolled(
                window,
                "record-shortcut-app.new".into(),
                "settings-section-scroll",
                cx,
            );
            assert!(view.read(cx).preferences_ui.recording.is_some());
            window.press("ctrl-s", cx);
            window.render_frame(cx);
            assert!(view.read(cx).preferences_ui.recording.is_none());
            assert_eq!(
                view.read(cx).preferences_ui.shortcuts["app.new"]
                    .read(cx)
                    .value()
                    .as_str(),
                "Ctrl+S"
            );
            assert!(
                !view.read(cx).note_saving && view.read(cx).note_intent.is_none(),
                "recorded Ctrl+S executed save"
            );
            assert!(window.try_find("shortcut-conflict-app.new").is_some());
            assert!(
                window.try_find("shortcut-conflict-editor.save").is_some(),
                "conflict omitted counterpart row"
            );
            window.click("save-settings", cx);
            assert!(
                !view.read(cx).preferences_ui.busy,
                "invalid shortcut draft started save"
            );
            assert!(view.read(cx).preferences_ui.error.is_some());
            assert_eq!(
                settings_files(&files_root),
                files,
                "invalid draft wrote configuration"
            );

            // Restore New with an actual recorded key, then capture that application
            // command on Import. A leaked Ctrl+N would open the dirty-leave dialog.
            click_scrolled(
                window,
                "record-shortcut-app.new".into(),
                "settings-section-scroll",
                cx,
            );
            window.press("ctrl-n", cx);
            click_scrolled(
                window,
                "record-shortcut-app.import".into(),
                "settings-section-scroll",
                cx,
            );
            window.press("ctrl-n", cx);
            window.render_frame(cx);
            assert!(
                view.read(cx).preferences_ui.leave.is_none(),
                "recorded application command executed"
            );
            assert_eq!(view.read(cx).page, Page::Settings);
            assert_eq!(view.read(cx).note_editor.is_some(), before_note_editor);
            assert_eq!(view.read(cx).selected_note, before_selected);
            assert_eq!(
                view.read(cx).snapshot.as_ref().unwrap().versions.len(),
                before_versions
            );
            assert!(window.find("shortcut-conflict-app.new").visible());
            assert!(window.find("shortcut-conflict-app.import").visible());
            window
                .render_to_image()
                .unwrap()
                .save(database.with_file_name("settings-shortcut-conflicts.png"))
                .unwrap();

            click_scrolled(
                window,
                "append-shortcut-app.import".into(),
                "settings-section-scroll",
                cx,
            );
            window.press("f8", cx);
            assert_eq!(
                view.read(cx).preferences_ui.shortcuts["app.import"]
                    .read(cx)
                    .value()
                    .as_str(),
                "Ctrl+N;F8"
            );
            click_scrolled(
                window,
                "record-shortcut-app.import".into(),
                "settings-section-scroll",
                cx,
            );
            window.press("escape", cx);
            assert!(view.read(cx).preferences_ui.recording.is_none());
            assert_eq!(
                view.read(cx).preferences_ui.shortcuts["app.import"]
                    .read(cx)
                    .value()
                    .as_str(),
                "Ctrl+N;F8",
                "Escape changed binding"
            );
            click_scrolled(
                window,
                "clear-shortcut-app.import".into(),
                "settings-section-scroll",
                cx,
            );
            assert!(
                view.read(cx).preferences_ui.shortcuts["app.import"]
                    .read(cx)
                    .value()
                    .is_empty()
            );

            click_scrolled(
                window,
                "record-shortcut-app.import".into(),
                "settings-section-scroll",
                cx,
            );
            window.blur(cx);
            window.press("f9", cx);
            assert!(
                view.read(cx).preferences_ui.recording.is_none(),
                "focus loss left recorder armed"
            );
            assert!(
                view.read(cx).preferences_ui.shortcuts["app.import"]
                    .read(cx)
                    .value()
                    .is_empty(),
                "escaped focus still captured a key"
            );
            // Bindings now use recorded keycaps. After capture ends, the real
            // search input must accept text and filter the grouped actions.
            let input = view
                .read(cx)
                .preferences_ui
                .shortcut_search
                .as_ref()
                .unwrap()
                .clone();
            window.render_frame(cx);
            input.read(cx).focus_handle(cx).focus(window, cx);
            window.input("heading-6", cx);
            assert_eq!(input.read(cx).value().as_str(), "heading-6");
            window.render_frame(cx);
            assert!(window.try_find("shortcut-row-editor.heading-6").is_some());
            assert!(window.try_find("shortcut-row-app.import").is_none());
            input.update(cx, |input, cx| input.set_value("", window, cx));
            click_scrolled(
                window,
                "record-shortcut-app.import".into(),
                "settings-section-scroll",
                cx,
            );
            assert_eq!(
                view.read(cx)
                    .preferences_ui
                    .recording
                    .as_ref()
                    .map(|recording| recording.action.as_str()),
                Some("app.import")
            );
            window.press("f10", cx);
            assert_eq!(
                view.read(cx).preferences_ui.shortcuts["app.import"]
                    .read(cx)
                    .value()
                    .as_str(),
                "F10"
            );
            assert!(view.read(cx).preferences_ui.recording.is_none());
            assert!(
                input.read(cx).focus_handle(cx).is_focused(window),
                "recording completion did not return focus to the visible search input"
            );
            assert_eq!(settings_files(&files_root), files);

            let header = window.find("save-settings").bounds();
            reveal_scrolled(
                window,
                "clear-shortcut-editor.delete-group-forward".into(),
                "settings-section-scroll",
                cx,
            );
            assert_eq!(
                window.find("save-settings").bounds(),
                header,
                "save bar scrolled with section"
            );
            window.click("reset-settings", cx);
            window.render_frame(cx);
            window.click("confirm-reset-settings", cx);
            assert_eq!(
                view.read(cx)
                    .preferences_ui
                    .saved
                    .as_ref()
                    .unwrap()
                    .revision,
                original.revision
            );
            assert_eq!(
                settings_files(&files_root),
                files,
                "reset wrote before explicit save"
            );
            for action in crate::shortcuts::SHORTCUT_ACTIONS {
                assert_eq!(
                    view.read(cx).preferences_ui.shortcuts[action.id]
                        .read(cx)
                        .value()
                        .as_str(),
                    action.default
                );
            }
            view.update(cx, |app, cx| app.discard_settings(window, cx));
            assert!(!view.read(cx).settings_dirty(cx));
            assert_eq!(
                view.read(cx)
                    .preferences_ui
                    .saved
                    .as_ref()
                    .unwrap()
                    .revision,
                original.revision
            );
            window.click(("nav", 1_usize), cx);
            assert_eq!(view.read(cx).page, Page::Notes);
        })
        .unwrap();
    let original_appearance = window
        .update(cx, |_, _, cx| {
            view.read(cx)
                .preferences_ui
                .saved
                .as_ref()
                .unwrap()
                .settings
                .appearance
                .clone()
        })
        .unwrap();
    for mode in ["dark", "light", "restore"] {
        window
            .update(cx, |_, window, cx| {
                window.click(("nav", 4_usize), cx);
                window.click("settings-section-appearance", cx);
                if mode == "restore" {
                    view.update(cx, |app, cx| {
                        app.preferences_ui.draft.as_mut().unwrap().appearance =
                            original_appearance.clone();
                        app.refresh_appearance(window, cx);
                        cx.notify();
                    });
                } else {
                    window.click(SharedString::from(format!("theme-mode-{mode}")), cx);
                }
                window.click("save-settings", cx);
            })
            .unwrap();
        until(
            window,
            view,
            "native theme committed",
            |app, cx| !app.preferences_ui.busy && !app.settings_dirty(cx),
            cx,
        )
        .await;
        let editor = window
            .update(cx, |_, window, cx| {
                window.click(("nav", 1_usize), cx);
                view.update(cx, |app, cx| app.clear_notices(cx));
                view.read(cx).note_content.clone()
            })
            .unwrap();
        crate::smoke_visibility::visible(window, &editor, cx).await;
        let state = crate::smoke_editor::bridge(
            window,
            &editor,
            serde_json::json!({"type":"snapshot"}),
            cx,
        )
        .await;
        window
            .update(cx, |_, _, _| {
                let palette = crate::theme::palette();
                assert_eq!(
                    state["theme"]["mode"],
                    if palette.dark { "dark" } else { "light" }
                );
                assert_eq!(
                    state["theme"]["background"],
                    format!("#{:06X}", palette.background)
                );
                assert_eq!(
                    state["theme"]["surface"],
                    format!("#{:06X}", palette.surface)
                );
                assert_eq!(state["theme"]["text"], format!("#{:06X}", palette.text));
            })
            .unwrap();
        crate::smoke_visibility::capture_window(
            window,
            &editor,
            &database.with_file_name(format!("native-theme-{mode}.bmp")),
            cx,
        )
        .await;
    }
    std::fs::write(database.with_file_name("settings-capture-smoke-result.txt"),
        "PASS: all six categories preserve one draft and byte-identical settings files; fixed save bar remains visible; platform Ctrl+S and Ctrl+N record without dispatch; both conflicting rows displayed in screenshot; strict save rejects; F8 appends, Escape cancels, clear unbinds, focus loss disarms and normal input resumes; reset remains draft until save, discard restores baseline; light/dark theme persistence, native/editor token coherence and full native screenshots, original appearance restored. Key events use native-window GPUI test dispatch; separate strict Win32 input gates remain required.\n").unwrap();
}
