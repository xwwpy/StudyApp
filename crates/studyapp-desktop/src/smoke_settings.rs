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
            window.render_frame(cx);
            let preview_rebuilds = view.read(cx).appearance_preview_rebuilds();
            click_scrolled(
                window,
                "show-file-paths".into(),
                "settings-section-scroll",
                cx,
            );
            assert_eq!(view.read(cx).appearance_preview_rebuilds(), preview_rebuilds, "unrelated appearance checkbox rebuilt Markdown preview colors");
            let body_size = view.read(cx).preferences_ui.appearance_fields["md-body-size"].clone();
            let original_body_size = body_size.read(cx).value();
            body_size.update(cx, |input, cx| input.set_value("18", window, cx));
            window.render_frame(cx);
            assert_eq!(view.read(cx).appearance_preview_rebuilds(), preview_rebuilds + 1, "programmatic field change did not refresh preview");
            body_size.update(cx, |input, cx| input.set_value(original_body_size, window, cx));
            window.render_frame(cx);
            assert_eq!(view.read(cx).appearance_preview_rebuilds(), preview_rebuilds + 2);
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

            window.click("settings-section-storage", cx);
            click_scrolled(
                window,
                "images-storage-cloud".into(),
                "settings-section-scroll",
                cx,
            );
            assert_eq!(
                view.read(cx)
                    .preferences_ui
                    .image_cloud
                    .as_ref()
                    .unwrap()
                    .kind,
                0
            );
            for key in [
                "endpoint",
                "bucket",
                "region",
                "access_key_id",
                "secret",
                "prefix",
            ] {
                let id = SharedString::from(format!("image-cloud-field-{key}"));
                reveal_scrolled(window, id.clone(), "settings-section-scroll", cx);
                assert!(window.find(id).bounds().size.height > px(0.));
            }
            assert_eq!(
                settings_files(&files_root),
                files,
                "cloud form wrote settings before save"
            );
            click_scrolled(
                window,
                "images-storage-local".into(),
                "settings-section-scroll",
                cx,
            );
            window.click("settings-section-shortcuts", cx);
            window.render_frame(cx);
            let nav = window.find("settings-category-navigation").bounds();
            let content = window.find("settings-section-scroll").bounds();
            if window.viewport_size().width < px(1120.) {
                assert!(
                    nav.bottom() <= content.top(),
                    "compact categories must stay above the scroll viewport"
                );
            } else {
                assert!(
                    nav.right() <= content.left(),
                    "wide settings categories must occupy a separate rail"
                );
            }
            let rebuilds = view.read(cx).shortcut_analysis_rebuilds();
            let mut frame_micros = Vec::with_capacity(20);
            for _ in 0..20 {
                view.update(cx, |_, cx| cx.notify());
                let started = std::time::Instant::now();
                window.render_frame(cx);
                frame_micros.push(started.elapsed().as_micros() as u64);
            }
            frame_micros.sort_unstable();
            std::fs::write(database.with_file_name("settings-render-timing.json"), serde_json::to_string_pretty(&serde_json::json!({
                "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
                "measurement": "GPUI TestWindowExt render_frame (refresh + draw + clear), unchanged shortcuts page",
                "frames": frame_micros.len(), "actions": crate::shortcuts::SHORTCUT_ACTIONS.len(),
                "p50_us": frame_micros[9], "p95_us": frame_micros[18], "samples_us": frame_micros,
                "analysis_rebuilds_before": rebuilds, "analysis_rebuilds_after": view.read(cx).shortcut_analysis_rebuilds(),
            })).unwrap()).unwrap();
            assert_eq!(
                view.read(cx).shortcut_analysis_rebuilds(),
                rebuilds,
                "unchanged settings frames reparsed all bindings"
            );
            click_scrolled(
                window,
                "record-shortcut-app.new".into(),
                "settings-section-scroll",
                cx,
            );
            assert!(view.read(cx).preferences_ui.recording.is_some());
            window.render_frame(cx);
            let capture = window.find("shortcut-recording-target").bounds();
            let cell = window.find("record-shortcut-app.new").bounds();
            assert!(capture.left() >= cell.left() && capture.right() <= cell.right());
            assert!(
                capture.top() >= cell.top() && capture.bottom() <= cell.bottom(),
                "key capture escaped its binding cell"
            );
            window.render_to_image().unwrap().save(database.with_file_name("settings-inline-recording.png")).unwrap();
            window.press("ctrl-s", cx);
            window.render_frame(cx);
            assert!(view.read(cx).preferences_ui.recording.is_none());
            assert_eq!(
                view.read(cx).shortcut_analysis_rebuilds(),
                rebuilds + 1,
                "one captured binding must rebuild analysis exactly once"
            );
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
    // Windows resize queues SetWindowPos asynchronously; yield before reading
    // the viewport or asserting responsive layout, unlike an in-process draw.
    let original_size = window
        .update(cx, |_, window, cx| {
            window.click(("nav", 4_usize), cx);
            window.click("settings-section-shortcuts", cx);
            let original = window.viewport_size();
            crate::smoke_library::resize(window, size(px(1024.), px(720.)));
            original
        })
        .unwrap();
    for attempt in 0..100 {
        cx.background_executor()
            .timer(std::time::Duration::from_millis(40))
            .await;
        let compact = window
            .update(cx, |_, window, _| window.viewport_size().width <= px(1024.))
            .unwrap();
        if compact {
            break;
        }
        assert!(attempt < 99, "native resize did not reach compact viewport");
    }
    window
        .update(cx, |_, window, cx| {
            window.render_frame(cx);
            let nav = window.find("settings-category-navigation").bounds();
            let content = window.find("settings-section-scroll").bounds();
            assert!(
                nav.bottom() <= content.top(),
                "compact nav overlaps form: viewport={:?}, nav={nav:?}, content={content:?}",
                window.viewport_size()
            );
            assert!(
                content.size.height > px(220.),
                "compact settings lost its usable content viewport"
            );
            for (_, id, _) in SettingsSection::ALL {
                let category = window.find(SharedString::from(format!("settings-section-{id}")));
                assert!(
                    category.visible(),
                    "category {id} became unreachable in compact settings"
                );
                assert!(category.bounds().right() <= nav.right());
            }
            assert!(window.find("save-settings").visible());
            window
                .render_to_image()
                .unwrap()
                .save(database.with_file_name("settings-compact.png"))
                .unwrap();
            crate::smoke_library::resize(window, original_size);
        })
        .unwrap();
    for attempt in 0..100 {
        cx.background_executor()
            .timer(std::time::Duration::from_millis(40))
            .await;
        let restored = window
            .update(cx, |_, window, _| window.viewport_size() == original_size)
            .unwrap();
        if restored {
            break;
        }
        assert!(attempt < 99, "native viewport did not restore");
    }
    window
        .update(cx, |_, window, cx| window.click(("nav", 1_usize), cx))
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
        "PASS: all six categories preserve one draft and byte-identical settings files; fixed save bar remains visible; wide category rail and settled 1024px compact navigation have bounded form content; inline recording stays inside its binding cell; 20 unchanged frames reuse one analysis and one captured edit rebuilds once; platform Ctrl+S and Ctrl+N record without dispatch; both conflicting rows displayed in screenshot; strict save rejects; F8 appends, Escape cancels, clear unbinds, focus loss disarms and normal input resumes; reset remains draft until save, discard restores baseline; light/dark theme persistence, native/editor token coherence and full native screenshots, original appearance restored. Key events use native-window GPUI test dispatch; separate strict Win32 input gates remain required.\n").unwrap();
}
