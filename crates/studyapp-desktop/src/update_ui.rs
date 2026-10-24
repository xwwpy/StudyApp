use crate::{components::*, controls::Button, update_backend::*};
use gpui_kit::{
    component::{Disableable, button::ButtonVariants},
    *,
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

pub enum UpdateEvent {
    InstallRequested,
    AvailabilityChanged,
}

enum UpdateReply {
    Checked(UpdateResult<UpdateCheck>),
    Progress(UpdateProgress),
    Prepared(String, UpdateResult<PreparedUpdate>),
    Previous(UpdateResult<Option<UpdateStatus>>),
}

/// Background checks and downloads invalidate only this panel, never the editor.
pub struct UpdatePanel {
    library: String,
    isolated_launch: bool,
    busy: bool,
    release: Option<UpdateRelease>,
    prepared: Option<(String, PreparedUpdate)>,
    status: String,
    error: Option<String>,
    previous: Option<UpdateStatus>,
    notes_scroll: ScrollHandle,
    receiver: mpsc::Receiver<UpdateReply>,
    sender: mpsc::Sender<UpdateReply>,
    _poll: Task<()>,
}

impl EventEmitter<UpdateEvent> for UpdatePanel {}

impl UpdatePanel {
    pub fn available_version(&self) -> Option<&str> {
        self.release
            .as_ref()
            .map(|release| release.version.as_str())
    }
    pub fn new(cx: &mut Context<Self>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let status_sender = sender.clone();
        std::thread::spawn(move || {
            let _ = status_sender.send(UpdateReply::Previous(read_last_update_status()));
        });
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                if this.update(cx, |this, cx| this.poll(cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            library: String::new(),
            busy: false,
            release: None,
            prepared: None,
            isolated_launch: std::env::args_os().any(|arg| arg == "--data-dir"),
            status: "手动检查最新正式版本".into(),
            error: None,
            previous: None,
            notes_scroll: ScrollHandle::new(),
            receiver,
            sender,
            _poll: poll,
        }
    }

    pub fn set_library(&mut self, library: String, cx: &mut Context<Self>) {
        if self.library == library {
            return;
        }
        self.library = library;
        if self.prepared.take().is_some() {
            self.status = "资料库已切换，请重新准备更新以保留当前打开位置。".into();
        }
        cx.notify();
    }

    pub fn prepared(&self, library: &str) -> Option<PreparedUpdate> {
        self.prepared
            .as_ref()
            .filter(|(saved, _)| saved == library)
            .map(|(_, update)| update.clone())
    }

    pub fn install_failed(&mut self, message: String, cx: &mut Context<Self>) {
        self.error = Some(message);
        cx.notify();
    }

    fn check(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        self.status = "正在检查正式版本…".into();
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let _ = sender.send(UpdateReply::Checked(check_for_update(env!(
                "CARGO_PKG_VERSION"
            ))));
        });
        cx.notify();
    }

    fn download(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.library.is_empty() {
            return;
        }
        let Some(release) = self.release.clone() else {
            return;
        };
        self.busy = true;
        self.error = None;
        self.prepared = None;
        self.status = "正在准备下载…".into();
        let sender = self.sender.clone();
        let library = self.library.clone();
        let isolated = self.isolated_launch;
        std::thread::spawn(move || {
            let directory = restart_data_directory(&library, isolated);
            let result = prepare_update(&release, directory, |progress| {
                let _ = sender.send(UpdateReply::Progress(progress));
            });
            let _ = sender.send(UpdateReply::Prepared(library, result));
        });
        cx.notify();
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        while let Ok(reply) = self.receiver.try_recv() {
            changed = true;
            match reply {
                UpdateReply::Checked(result) => {
                    self.busy = false;
                    match result {
                        Ok(UpdateCheck::Available(release)) => {
                            if self
                                .release
                                .as_ref()
                                .is_none_or(|old| !old.same_artifacts(&release))
                            {
                                self.prepared = None;
                            }
                            self.status = format!(
                                "发现新版本 {} · {:.1} MB",
                                release.version,
                                release.package_size as f64 / 1_048_576.
                            );
                            self.release = Some(*release);
                        }
                        Ok(UpdateCheck::UpToDate { current_version }) => {
                            self.status = format!("当前 {current_version} 已是最新正式版本");
                            self.release = None;
                            self.prepared = None;
                        }
                        Err(error) => self.show_error(error),
                    }
                    cx.emit(UpdateEvent::AvailabilityChanged);
                }
                UpdateReply::Progress(progress) => self.status = progress_text(&progress),
                UpdateReply::Prepared(library, result) => {
                    self.busy = false;
                    match result {
                        Ok(prepared) if library == self.library => {
                            self.status =
                                format!("{} 已下载并校验，可以重启安装", prepared.version);
                            self.prepared = Some((library, prepared));
                        }
                        Ok(_) => self.status = "资料库已切换，请重新准备更新。".into(),
                        Err(error) => self.show_error(error),
                    }
                }
                UpdateReply::Previous(Ok(status)) => self.previous = status,
                UpdateReply::Previous(Err(_)) => {
                    // An unreadable old receipt must not block checks or show a
                    // recurring launch-time error unrelated to the current task.
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    fn show_error(&mut self, error: UpdateError) {
        if matches!(error.kind, UpdateErrorKind::ManualUpdate) {
            self.status = "请使用手动更新".into();
        }
        self.error = Some(error.to_string());
    }
}

fn restart_data_directory(library: &str, isolated: bool) -> Option<&Path> {
    isolated.then(|| Path::new(library).parent()).flatten()
}

fn progress_text(progress: &UpdateProgress) -> String {
    match progress {
        UpdateProgress::Downloading { downloaded, total } => format!(
            "正在下载 · {:.1} / {:.1} MB",
            *downloaded as f64 / 1_048_576.,
            *total as f64 / 1_048_576.
        ),
        UpdateProgress::Verifying => "正在校验下载文件…".into(),
        UpdateProgress::Preparing => "正在准备更新并检查安装位置…".into(),
    }
}

impl Render for UpdatePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let release_url = self
            .release
            .as_ref()
            .map(|release| release.page_url.clone())
            .unwrap_or_else(|| "https://github.com/xwwpy/StudyApp/releases".into());
        card().id("application-updates").test_support()
            .child(heading("软件更新"))
            .child(muted(format!("WW-StudyApp {}", env!("CARGO_PKG_VERSION"))))
            .child(div().child(self.status.clone()))
            .children(self.error.as_ref().map(|error| div().text_color(crate::theme::color(0xB45142)).child(error.clone())))
            .child(row().flex_wrap()
                .child(Button::new("check-for-updates").label("检查更新").disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.check(cx))))
                .children((self.release.is_some() && self.prepared.is_none()).then(||
                    Button::new("download-update").label("下载更新").disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.download(cx)))))
                .children(self.prepared.as_ref().map(|_| Button::new("restart-to-update")
                    .label("重启并安装").disabled(self.busy)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(UpdateEvent::InstallRequested)))))
                .child(Button::new("open-releases").ghost().label("发布页面")
                    .on_click(move |_, _, cx| cx.open_url(&release_url))))
            .children(self.release.as_ref().map(|release|
                scroll_body("update-release-notes", &self.notes_scroll).max_h(px(180.)).pr_3()
                    .child(div().child(release.notes.clone()))))
            .children(self.prepared.as_ref().map(|(_, update)| {
                let status = update.status_path.clone();
                column().gap_1().child(muted(format!("安装位置：{}", studyapp_desktop::presentation::display_path(&update.installation_directory.to_string_lossy()))))
                    .child(Button::new("reveal-update-record").ghost().label("查看更新记录")
                        .on_click(move |_, _, cx| cx.reveal_path(&status)))
            }))
            .children(self.previous.as_ref().map(|status| column().gap_1()
                .child(muted(format!("{} · {}", status.version, status.user_message())))
                .children((!status.backup_path.is_empty()).then(|| {
                    let path = PathBuf::from(&status.backup_path);
                    Button::new("reveal-update-backup").ghost().label("定位旧版本备份")
                        .on_click(move |_, _, cx| cx.reveal_path(&path))
                }))
                .children((!status.distribution_path.is_empty()).then(|| {
                    let path = PathBuf::from(&status.distribution_path);
                    Button::new("reveal-update-distribution").ghost().label("查看发行文件")
                        .on_click(move |_, _, cx| cx.reveal_path(&path))
                }))))
            .child(muted("安装前会保存草稿并处理未保存的设置。自动识别当前程序位置，保留旧版本备份和资料库；安装目录不可写时可从发布页面手动更新。"))
    }
}

impl crate::app::StudyApp {
    pub fn request_install_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.guard_settings(crate::settings_ui::SettingsContinuation::InstallUpdate, cx) {
            return;
        }
        let Some(prepared) = self.update_panel.read(cx).prepared(&self.last_library) else {
            self.push_notice(Err("请先下载更新；切换资料库后需要重新准备。".into()), cx);
            return;
        };
        self.pending_update = Some(prepared);
        self.close_requested = true;
        self.continue_close(window, cx);
    }

    /// Called only after settings and the active editor's durable-close barrier.
    pub fn complete_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(prepared) = self.pending_update.take()
            && let Err(error) = launch_prepared_update(&prepared)
        {
            self.close_requested = false;
            self.update_panel
                .update(cx, |panel, cx| panel.install_failed(error.to_string(), cx));
            self.push_notice(Err(error.to_string()), cx);
            return;
        }
        self.close_allowed = true;
        window.remove_window();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn update_restart_preserves_normal_and_isolated_launch_modes() {
        let library = std::path::Path::new("C:/fixtures/library/studyapp.sqlite3");
        let text = library.to_string_lossy();
        assert_eq!(super::restart_data_directory(&text, false), None);
        assert_eq!(super::restart_data_directory(&text, true), library.parent());
    }
}
