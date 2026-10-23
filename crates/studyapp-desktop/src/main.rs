#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod components;
mod controls;
mod credentials;
mod drafts;
mod editor_lifecycle;
mod feedback;
mod folders_ui;
mod history_preview;
mod imports;
mod markdown_editor;
mod native_shortcuts;
mod note_actions;
mod note_commands;
mod note_editor;
mod notes_view;
mod preferences;
mod resize_overlay;
mod settings_ui;
mod shortcuts;
mod smoke;
#[cfg(feature = "ui-test")]
mod smoke_editor;
#[cfg(all(windows, feature = "ui-test"))]
mod smoke_interactions;
#[cfg(all(windows, feature = "ui-test"))]
mod smoke_library;
#[cfg(all(windows, feature = "ui-test"))]
mod smoke_visibility;
#[cfg(all(windows, feature = "ui-test"))]
mod smoke_workspace;
mod storage_backend;
mod storage_ui;
mod sync_runtime;
mod sync_ui;
mod views;
#[cfg(windows)]
mod web_editor_host;
mod worker;
mod workspace_layout;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use std::path::PathBuf;

fn paths() -> anyhow::Result<(PathBuf, Option<PathBuf>, bool)> {
    let args: Vec<_> = std::env::args().collect();
    let smoke = args.iter().any(|a| a == "--smoke-test");
    let project = directories::ProjectDirs::from("io", "StudyApp", "StudyApp")
        .ok_or_else(|| anyhow::anyhow!("无法定位用户数据目录"))?;
    let config = project.config_dir().join("location.json");
    let explicit = args
        .iter()
        .position(|a| a == "--data-dir")
        .map(|index| {
            args.get(index + 1)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("--data-dir 后需要目录路径"))
        })
        .transpose()?;
    if smoke && explicit.is_none() {
        anyhow::bail!("--smoke-test 必须配合 --data-dir 指定隔离的测试目录");
    }
    let directory = if let Some(path) = &explicit {
        PathBuf::from(path)
    } else if config.exists() {
        serde_json::from_slice::<PathBuf>(&std::fs::read(&config)?)?
    } else {
        project.data_dir().to_path_buf()
    };
    std::fs::create_dir_all(&directory)?;
    Ok((
        directory.join("studyapp.sqlite3"),
        explicit.is_none().then_some(config),
        smoke,
    ))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("WW-StudyApp: {error:#}");
        rfd::MessageDialog::new()
            .set_title("WW-StudyApp 启动失败")
            .set_description(format!("{error:#}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let (database, config, smoke_test) = paths()?;
    if smoke_test {
        smoke::prepare(&database)?;
    }
    let worker = worker::Worker::start(database.clone(), config);
    application()
        .with_assets(assets::StudyAssets)
        .run(move |cx| {
            init(cx);
            Theme::change(ThemeMode::Light, None, cx);
            let theme = Theme::global_mut(cx);
            theme.font_family = "Microsoft YaHei UI".into();
            theme.font_size = px(14.);
            theme.colors.primary = rgb(components::GREEN).into();
            theme.colors.primary_foreground = rgb(0xFFFFFF).into();
            theme.colors.ring = rgb(components::GREEN).into();
            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("WW-StudyApp".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1280.), px(860.)),
                    cx,
                ))),
                window_min_size: Some(size(px(1000.), px(700.))),
                ..Default::default()
            };
            let (window, view) = open_window(options, cx, |window, cx| {
                cx.new(|cx| app::StudyApp::new(worker, window, cx))
            })
            .expect("创建 WW-StudyApp 窗口失败");
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            if smoke_test {
                smoke::exercise(window, view, database, cx);
            }
            cx.activate(true);
        });
    Ok(())
}
