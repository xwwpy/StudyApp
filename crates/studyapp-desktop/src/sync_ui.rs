use crate::{
    components::*,
    sync_runtime::{SyncSettings, SyncStatus, SyncTarget},
    worker::Work,
};
use gpui_kit::{
    component::{
        Disableable, Selectable,
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        input::{Input, InputState},
    },
    *,
};
use std::sync::mpsc;

pub struct SyncPanel {
    sender: mpsc::Sender<Work>,
    pub status: SyncStatus,
    pub target: usize,
    pub folder: Entity<InputState>,
    pub url: Entity<InputState>,
    pub username: Entity<InputState>,
    pub password: Entity<InputState>,
    interval: Entity<InputState>,
    automatic: bool,
    initialized: bool,
    error: Option<String>,
    picked_sender: mpsc::Sender<(String, std::path::PathBuf)>,
    picked: mpsc::Receiver<(String, std::path::PathBuf)>,
}

impl SyncPanel {
    fn has_unsaved_changes(&self, cx: &App) -> bool {
        let target_matches = match (&self.status.settings.target, self.target) {
            (SyncTarget::Disabled, 0) => true,
            (SyncTarget::Folder { path }, 1) => path == self.folder.read(cx).value().trim(),
            (SyncTarget::WebDav { url, username }, 2) => {
                url == self.url.read(cx).value().trim()
                    && username == self.username.read(cx).value().trim()
            }
            _ => false,
        };
        !target_matches
            || !self.password.read(cx).value().is_empty()
            || self.automatic != self.status.settings.auto_minutes.is_some()
            || (self.automatic
                && self.interval.read(cx).value().trim().parse::<u32>().ok()
                    != self.status.settings.auto_minutes)
    }
    pub fn new(sender: mpsc::Sender<Work>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = |placeholder: &str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder.to_owned()))
        };
        let password = input("应用密码；留空保留已保存密码", window, cx);
        password.update(cx, |s, cx| s.set_masked(true, window, cx));
        let (picked_sender, picked) = mpsc::channel();
        let panel = Self {
            sender,
            status: SyncStatus::default(),
            target: 0,
            folder: input("选择本地文件夹、网盘同步目录或 NAS 路径", window, cx),
            url: input("https://dav.example.com/path/", window, cx),
            username: input("WebDAV 用户名", window, cx),
            password,
            interval: input("5–1440 分钟", window, cx),
            automatic: false,
            initialized: false,
            error: None,
            picked_sender,
            picked,
        };
        for input in [
            &panel.folder,
            &panel.url,
            &panel.username,
            &panel.password,
            &panel.interval,
        ] {
            cx.subscribe(
                input,
                |_, _, _: &gpui_kit::component::input::InputEvent, cx| cx.notify(),
            )
            .detach();
        }
        panel
    }

    pub fn accept(
        &mut self,
        status: SyncStatus,
        reset: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if reset
            || !self.initialized
            || status.library != self.status.library
            || status.settings != self.status.settings
        {
            self.target = match &status.settings.target {
                SyncTarget::Disabled => 0,
                SyncTarget::Folder { path } => {
                    self.folder
                        .update(cx, |s, cx| s.set_value(path.clone(), window, cx));
                    1
                }
                SyncTarget::WebDav { url, username } => {
                    self.url
                        .update(cx, |s, cx| s.set_value(url.clone(), window, cx));
                    self.username
                        .update(cx, |s, cx| s.set_value(username.clone(), window, cx));
                    2
                }
            };
            self.password
                .update(cx, |s, cx| s.set_value("", window, cx));
            self.automatic = status.settings.auto_minutes.is_some();
            self.interval.update(cx, |s, cx| {
                s.set_value(
                    status.settings.auto_minutes.unwrap_or(15).to_string(),
                    window,
                    cx,
                )
            });
            self.initialized = true;
        }
        self.status = status;
        cx.notify();
    }

    pub fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Ok((library, path)) = self.picked.try_recv() {
            if library != self.status.library {
                continue;
            }
            self.folder.update(cx, |s, cx| {
                s.set_value(path.to_string_lossy().into_owned(), window, cx)
            });
            cx.notify();
        }
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = (|| -> anyhow::Result<()> {
            let target = match self.target {
                1 => SyncTarget::Folder {
                    path: self.folder.read(cx).value().trim().into(),
                },
                2 => SyncTarget::WebDav {
                    url: self.url.read(cx).value().trim().into(),
                    username: self.username.read(cx).value().trim().into(),
                },
                _ => SyncTarget::Disabled,
            };
            let auto_minutes = if self.automatic {
                Some(
                    self.interval
                        .read(cx)
                        .value()
                        .trim()
                        .parse()
                        .map_err(|_| anyhow::anyhow!("自动同步间隔必须为整数"))?,
                )
            } else {
                None
            };
            let raw = self.password.read(cx).value().to_string();
            let password = (!raw.is_empty()).then(|| zeroize::Zeroizing::new(raw));
            self.sender
                .send(Work::Scoped {
                    library: self.status.library.clone(),
                    work: Box::new(Work::ConfigureSync {
                        settings: SyncSettings {
                            target,
                            auto_minutes,
                        },
                        password,
                    }),
                })
                .map_err(|_| anyhow::anyhow!("后台服务已停止"))?;
            self.password
                .update(cx, |s, cx| s.set_value("", window, cx));
            Ok(())
        })();
        self.error = result.err().map(|e| e.to_string());
        cx.notify();
    }

    fn start(&mut self, test_only: bool, cx: &mut Context<Self>) {
        if self.has_unsaved_changes(cx) {
            self.error = Some("同步设置有未保存的修改，请先保存。".into());
            cx.notify();
            return;
        }
        self.error = self
            .sender
            .send(Work::Scoped {
                library: self.status.library.clone(),
                work: Box::new(Work::StartSync { test_only }),
            })
            .err()
            .map(|_| "后台服务已停止".into());
        cx.notify();
    }
}

impl Render for SyncPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.status.busy;
        let unsaved = self.has_unsaved_changes(cx);
        let mut panel = card().child(heading("笔记文件同步"))
            .child(muted("连接自己的 WebDAV 或文件夹/NAS。同步 Markdown 文件，完整历史和学习记录由本地资料库与备份管理。"))
            .child(row().children(["关闭同步", "文件夹 / NAS", "WebDAV"].into_iter().enumerate().map(|(i, label)| {
                Button::new(("sync-target", i)).label(label).selected(self.target == i).disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| { this.target = i; cx.notify(); }))
            })));
        if self.target == 1 {
            panel = panel.child(
                row()
                    .child(Input::new(&self.folder).disabled(busy).flex_1())
                    .child(
                        Button::new("pick-sync-folder")
                            .label("选择文件夹")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, _| {
                                let sender = this.picked_sender.clone();
                                let library = this.status.library.clone();
                                std::thread::spawn(move || {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .set_title("选择同步根目录")
                                        .pick_folder()
                                    {
                                        let _ = sender.send((library, path));
                                    }
                                });
                            })),
                    ),
            );
        } else if self.target == 2 {
            panel = panel
                .child(Input::new(&self.url).disabled(busy))
                .child(
                    row()
                        .child(Input::new(&self.username).disabled(busy).flex_1())
                        .child(Input::new(&self.password).disabled(busy).flex_1()),
                )
                .child(muted(if self.status.credential_saved {
                    "密码已由当前 Windows 用户保护；此字段留空可继续使用。"
                } else {
                    "建议使用服务商提供的应用密码；凭据不进入学习数据库或备份。"
                }));
        }
        panel.child(row().child(Checkbox::new("auto-sync").label("自动同步").checked(self.automatic).disabled(busy)
            .on_click(cx.listener(|this, checked, _, cx| { this.automatic = *checked; cx.notify(); })))
            .child(Input::new(&self.interval).disabled(busy || !self.automatic).w(px(150.))).child(muted("分钟")))
            .child(row().child(Button::new("save-sync-settings").primary().label("保存同步设置").disabled(busy).on_click(cx.listener(|this, _, window, cx| this.save(window, cx))))
                .child(Button::new("test-sync").label("测试连接").disabled(busy || unsaved || matches!(self.status.settings.target, SyncTarget::Disabled)).on_click(cx.listener(|this, _, _, cx| this.start(true, cx))))
                .child(Button::new("sync-now").label(if busy { "正在同步…" } else { "立即同步" }).disabled(busy || unsaved || matches!(self.status.settings.target, SyncTarget::Disabled)).on_click(cx.listener(|this, _, _, cx| this.start(false, cx)))))
            .child(muted(if unsaved { "同步设置有未保存的修改，请先保存。" } else { "使用已保存的同步设置" }))
            .child(muted(self.error.as_ref().unwrap_or(&self.status.message).clone()))
            .child(muted(format!("最近成功：{}", self.status.last_success.map(studyapp_desktop::presentation::stamp).unwrap_or_else(|| "尚未完成".into()))))
            .child(muted("远端专用目录：StudyApp/notes；不同修改会保留冲突副本。不传播删除，导入的外部原文件不会被同步改写。"))
    }
}
