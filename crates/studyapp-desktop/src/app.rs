use crate::{
    components::*,
    drafts::{DraftStore, NoteDraft},
    worker::{Work, Worker},
};
use chrono::{Local, Utc};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{InputState, TextareaState},
};
use gpui_kit::*;
use std::{collections::HashSet, sync::Arc, time::Duration};
use studyapp_core::{Command, SessionStatus, Snapshot};
use studyapp_desktop::presentation::{parse_local_time, safe_markdown_html};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Today,
    Notes,
    Reviews,
    Stats,
    Settings,
}

#[derive(Clone)]
pub enum EditorMode {
    Pending(String),
    Manual,
    Edit(String),
}

pub struct StudyApp {
    pub worker: Worker,
    pub snapshot: Option<Arc<Snapshot>>,
    pub page: Page,
    pub notice: Option<Result<String, String>>,
    pub search: Entity<InputState>,
    pub summary: Entity<TextareaState>,
    pub minutes: Entity<InputState>,
    pub start: Entity<InputState>,
    pub interval: Entity<InputState>,
    pub deadline: Entity<InputState>,
    pub review_days: Entity<InputState>,
    pub editor: Option<EditorMode>,
    pub chosen_notes: HashSet<String>,
    pub chosen_versions: HashSet<String>,
    pub saving: bool,
    pub selected_note: Option<String>,
    pub selected_version: Option<String>,
    pub preview_key: String,
    pub preview_html: String,
    pub history: bool,
    pub stats_days: usize,
    pub record_filter: usize,
    pub record_limit: usize,
    pub review_filter: usize,
    pub last_library: String,
    pub drafts: DraftStore,
    pub note_title: Entity<InputState>,
    pub note_content: Entity<TextareaState>,
    pub note_editor: Option<NoteDraft>,
    pub recoverable_draft: Option<NoteDraft>,
    pub draft_loading: bool,
    pub draft_failed: bool,
    pub record_date: Entity<InputState>,
    pub note_saving: bool,
    pub note_preview: bool,
    pub sync_panel: Entity<crate::sync_ui::SyncPanel>,
    notified: HashSet<String>,
    _poll: Task<()>,
}

impl StudyApp {
    pub fn new(worker: Worker, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            |placeholder: &str, value: &str, window: &mut Window, cx: &mut Context<Self>| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder.to_owned())
                        .default_value(value.to_owned())
                })
            };
        let poll = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                if this
                    .update_in(cx, |this, window, cx| this.poll(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let search = input("搜索标题、路径与正文", "", window, cx);
        cx.subscribe(
            &search,
            |_, _, _: &gpui_kit::component::input::InputEvent, cx| cx.notify(),
        )
        .detach();
        let record_date = input("学习日期 YYYY-MM-DD", "", window, cx);
        cx.subscribe(
            &record_date,
            |this, _, _: &gpui_kit::component::input::InputEvent, cx| {
                this.record_limit = 50;
                cx.notify();
            },
        )
        .detach();
        let note_title = input("笔记标题", "", window, cx);
        let note_content =
            cx.new(|cx| TextareaState::new(window, cx).placeholder("使用标准 Markdown 记录知识…"));
        cx.subscribe(
            &note_title,
            |this, _, _: &gpui_kit::component::input::InputEvent, cx| this.capture_note_draft(cx),
        )
        .detach();
        cx.subscribe(
            &note_content,
            |this, _, _: &gpui_kit::component::input::InputEvent, cx| this.capture_note_draft(cx),
        )
        .detach();
        let sync_panel =
            cx.new(|cx| crate::sync_ui::SyncPanel::new(worker.sender.clone(), window, cx));
        Self {
            sync_panel,
            drafts: DraftStore::new(),
            note_title,
            note_content,
            record_date,
            draft_failed: false,
            note_editor: None,
            recoverable_draft: None,
            draft_loading: true,
            note_saving: false,
            note_preview: false,
            worker,
            snapshot: None,
            page: Page::Today,
            notice: None,
            search,
            summary: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("记录你学到了什么，也可以使用笔记变更摘要…")
            }),
            minutes: input("分钟", "30", window, cx),
            start: input(
                "YYYY-MM-DD HH:MM",
                &Local::now().format("%Y-%m-%d %H:%M").to_string(),
                window,
                cx,
            ),
            interval: input("5–240 分钟", "60", window, cx),
            deadline: input("1–30 分钟", "5", window, cx),
            review_days: input("用逗号分隔天数", "1, 3, 7, 14, 30", window, cx),
            editor: None,
            chosen_notes: HashSet::new(),
            chosen_versions: HashSet::new(),
            saving: false,
            selected_note: None,
            selected_version: None,
            preview_key: String::new(),
            preview_html: String::new(),
            history: false,
            stats_days: 30,
            record_filter: 0,
            record_limit: 50,
            review_filter: 0,
            last_library: String::new(),
            notified: HashSet::new(),
            _poll: poll,
        }
    }

    fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_panel
            .update(cx, |panel, cx| panel.poll(window, cx));
        let mut changed = self.poll_drafts(cx);
        while let Ok(update) = self.worker.receiver.try_recv() {
            if let Some(status) = update.sync_status {
                self.sync_panel.update(cx, |panel, cx| {
                    panel.accept(status, update.library_replaced, window, cx)
                });
            }
            if let Some(result) = update.note_result {
                self.note_saving = false;
                if let Ok(note_id) = result {
                    self.selected_note = Some(note_id);
                    self.selected_version = None;
                    self.note_editor = None;
                    self.recoverable_draft = None;
                    self.drafts.save(&self.last_library, None);
                    self.page = Page::Notes;
                }
            }
            if update.editor_result {
                self.saving = false;
                if update.notice.as_ref().is_some_and(Result::is_ok) {
                    self.editor = None;
                }
            }
            if let Some(notice) = update.notice {
                self.notice = Some(notice);
            }
            if let Some(snapshot) = update.snapshot {
                if update.library_replaced || self.last_library != snapshot.data_path {
                    self.capture_note_draft(cx);
                    self.note_editor = None;
                    self.recoverable_draft = None;
                    self.note_saving = false;
                    self.draft_loading = true;
                    self.draft_failed = false;
                    self.last_library = snapshot.data_path.clone();
                    self.drafts.load(&self.last_library);
                    self.selected_note = None;
                    self.selected_version = None;
                    self.preview_key.clear();
                    self.editor = None;
                    self.saving = false;
                    self.chosen_notes.clear();
                    self.chosen_versions.clear();
                    self.notified.clear();
                    self.interval.update(cx, |s, cx| {
                        s.set_value(snapshot.settings.interval_minutes.to_string(), window, cx)
                    });
                    self.deadline.update(cx, |s, cx| {
                        s.set_value(snapshot.settings.answer_minutes.to_string(), window, cx)
                    });
                    self.review_days.update(cx, |s, cx| {
                        s.set_value(
                            snapshot
                                .settings
                                .review_days
                                .iter()
                                .map(u32::to_string)
                                .collect::<Vec<_>>()
                                .join(", "),
                            window,
                            cx,
                        )
                    });
                }
                for session in &snapshot.sessions {
                    if session.status == SessionStatus::Pending
                        && self.notified.insert(session.id.clone())
                    {
                        cx.show_system_notification(SystemNotification {
                            tag: session.id.clone().into(),
                            title: "该回顾刚才的学习了".into(),
                            body: format!(
                                "请在 {} 分钟内记录学到了什么，未回答将记为未学习。",
                                snapshot.settings.answer_minutes
                            )
                            .into(),
                            actions: vec![],
                        });
                        if self.editor.is_none() {
                            self.editor = Some(EditorMode::Pending(session.id.clone()));
                            self.summary.update(cx, |s, cx| {
                                s.set_value(session.suggested_summary.clone(), window, cx)
                            });
                            self.chosen_notes = session.note_ids.iter().cloned().collect();
                            self.chosen_versions = session.version_ids.iter().cloned().collect();
                            self.seed_versions(&snapshot, session.ended_at);
                            self.page = Page::Today;
                        }
                    }
                }
                if self
                    .selected_note
                    .as_ref()
                    .is_none_or(|id| !snapshot.notes.iter().any(|n| &n.id == id))
                {
                    self.selected_note = snapshot.notes.first().map(|n| n.id.clone());
                    self.selected_version = None;
                }
                self.snapshot = Some(snapshot);
            }
            changed = true;
        }
        if changed {
            self.refresh_preview();
            cx.notify();
        }
    }

    pub fn send(&mut self, command: Command, cx: &mut Context<Self>) {
        if self
            .worker
            .sender
            .send(Work::Scoped {
                library: self.last_library.clone(),
                work: Box::new(Work::Apply(command)),
            })
            .is_err()
        {
            self.notice = Some(Err("后台服务已停止，请重新打开应用".into()));
            self.saving = false;
            self.note_saving = false;
        }
        cx.notify();
    }

    pub fn picker(&self, action: &'static str, note: Option<String>) {
        let sender = self.worker.sender.clone();
        let library = self.last_library.clone();
        std::thread::spawn(move || {
            let work = match action {
                "import" => rfd::FileDialog::new().set_title("导入 Markdown 笔记").add_filter("Markdown", &["md", "markdown"]).pick_files().map(Work::ImportPaths),
                "import-folder" => rfd::FileDialog::new().set_title("导入目录中的 Markdown").pick_folder().map(|p| Work::ImportPaths(vec![p])),
                "relink" => rfd::FileDialog::new().add_filter("Markdown", &["md", "markdown"]).pick_file().zip(note).map(|(path, note_id)| Work::Apply(Command::Relink { note_id, path })),
                "backup" => rfd::FileDialog::new().set_file_name(format!("StudyApp-{}.sqlite3", Local::now().format("%Y%m%d-%H%M%S"))).save_file().map(|p| Work::Apply(Command::Backup(p))),
                "restore" => rfd::FileDialog::new().set_title("选择 StudyApp 备份（恢复前自动保留当前资料）").add_filter("SQLite 备份", &["sqlite3", "sqlite", "db"]).pick_file().and_then(|p| {
                    let confirmed = rfd::MessageDialog::new().set_title("恢复资料库").set_description("将用选中的备份恢复当前资料库。恢复前会自动保存当前资料的备份；源 Markdown 文件不会改变。").set_buttons(rfd::MessageButtons::OkCancel).show();
                    (confirmed == rfd::MessageDialogResult::Ok).then_some(Work::Apply(Command::Restore(p)))
                }),
                "library" => rfd::FileDialog::new().set_title("选择本地资料库文件夹").pick_folder().map(Work::SwitchLibrary),
                _ => None,
            };
            if let Some(work) = work {
                let _ = sender.send(Work::Scoped {
                    library,
                    work: Box::new(work),
                });
            }
        });
    }

    pub fn open_editor(&mut self, mode: EditorMode, window: &mut Window, cx: &mut Context<Self>) {
        let mut text = String::new();
        self.chosen_notes.clear();
        self.chosen_versions.clear();
        let mut ended_at = Utc::now().timestamp();
        if let EditorMode::Pending(id) | EditorMode::Edit(id) = &mode {
            if let Some(session) = self
                .snapshot
                .as_ref()
                .and_then(|s| s.sessions.iter().find(|s| &s.id == id))
            {
                text = if matches!(mode, EditorMode::Pending(_)) {
                    session.suggested_summary.clone()
                } else {
                    session.summary.clone()
                };
                self.chosen_notes = session.note_ids.iter().cloned().collect();
                self.chosen_versions = session.version_ids.iter().cloned().collect();
                ended_at = session.ended_at;
            }
        } else {
            let start = Local::now() - chrono::Duration::minutes(30);
            self.start.update(cx, |s, cx| {
                s.set_value(start.format("%Y-%m-%d %H:%M").to_string(), window, cx)
            });
            self.minutes
                .update(cx, |s, cx| s.set_value("30", window, cx));
            if let Some(note) = &self.selected_note {
                self.chosen_notes.insert(note.clone());
            }
        }
        if let Some(snapshot) = self.snapshot.clone() {
            self.seed_versions(&snapshot, ended_at);
        }
        self.summary
            .update(cx, |s, cx| s.set_value(text, window, cx));
        self.editor = Some(mode);
        self.page = Page::Today;
        self.saving = false;
        cx.notify();
    }

    pub fn save_editor(&mut self, cx: &mut Context<Self>) {
        let summary = self.summary.read(cx).value().to_string();
        let note_ids = self.chosen_notes.iter().cloned().collect();
        let version_ids = Some(self.snapshot.as_ref().map_or_else(Vec::new, |snapshot| {
            snapshot
                .versions
                .iter()
                .filter(|v| self.chosen_versions.contains(&v.id))
                .map(|v| v.id.clone())
                .collect()
        }));
        let result = (|| -> anyhow::Result<Command> {
            Ok(
                match self
                    .editor
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("没有待保存记录"))?
                {
                    EditorMode::Pending(session_id) => Command::Submit {
                        session_id,
                        summary,
                        note_ids,
                        version_ids,
                    },
                    EditorMode::Edit(session_id) => Command::EditSession {
                        session_id,
                        summary,
                        learned: true,
                        note_ids,
                        version_ids,
                    },
                    EditorMode::Manual => Command::Manual {
                        started_at: parse_local_time(&self.start.read(cx).value())?,
                        minutes: self
                            .minutes
                            .read(cx)
                            .value()
                            .trim()
                            .parse()
                            .map_err(|_| anyhow::anyhow!("学习分钟数必须是整数"))?,
                        summary,
                        note_ids,
                        version_ids,
                    },
                },
            )
        })();
        match result {
            Ok(command) => {
                self.saving = true;
                self.send(command, cx);
            }
            Err(error) => {
                self.notice = Some(Err(error.to_string()));
                cx.notify();
            }
        }
    }

    fn seed_versions(&mut self, snapshot: &Snapshot, ended_at: i64) {
        self.chosen_versions.retain(|id| {
            snapshot
                .versions
                .iter()
                .any(|v| &v.id == id && self.chosen_notes.contains(&v.note_id))
        });
        for note in &self.chosen_notes {
            if !snapshot
                .versions
                .iter()
                .any(|v| &v.note_id == note && self.chosen_versions.contains(&v.id))
            {
                let version = snapshot
                    .versions
                    .iter()
                    .rev()
                    .find(|v| &v.note_id == note && v.captured_at <= ended_at)
                    .or_else(|| snapshot.versions.iter().rev().find(|v| &v.note_id == note));
                if let Some(version) = version {
                    self.chosen_versions.insert(version.id.clone());
                }
            }
        }
    }

    pub fn toggle_note(&mut self, id: &str, selected: bool) {
        if selected {
            self.chosen_notes.insert(id.to_owned());
        } else {
            self.chosen_notes.remove(id);
        }
        if let Some(snapshot) = self.snapshot.clone() {
            let ended_at = match &self.editor {
                Some(EditorMode::Pending(id) | EditorMode::Edit(id)) => snapshot
                    .sessions
                    .iter()
                    .find(|s| &s.id == id)
                    .map_or(Utc::now().timestamp(), |s| s.ended_at),
                _ => Utc::now().timestamp(),
            };
            self.seed_versions(&snapshot, ended_at);
        }
    }

    pub fn show_version(&mut self, note: String, version: String, cx: &mut Context<Self>) {
        self.selected_note = Some(note);
        self.selected_version = Some(version);
        self.history = false;
        self.page = Page::Notes;
        self.refresh_preview();
        cx.notify();
    }

    pub fn refresh_preview(&mut self) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let version = snapshot.versions.iter().rev().find(|v| {
            self.selected_note.as_ref() == Some(&v.note_id)
                && self.selected_version.as_ref().is_none_or(|id| id == &v.id)
        });
        if let Some(version) = version {
            if self.preview_key != version.id {
                let text: String = version.content.chars().take(120_000).collect();
                self.preview_html = safe_markdown_html(&text);
                if text.len() < version.content.len() {
                    self.preview_html
                        .push_str("<p>预览显示前 120,000 字符，完整内容已保存在历史快照中。</p>");
                }
                self.preview_key = version.id.clone();
            }
        } else {
            self.preview_html.clear();
            self.preview_key.clear();
        }
    }

    fn navigation(&self, cx: &mut Context<Self>) -> Div {
        let items = [
            (Page::Today, "今日学习", IconName::House),
            (Page::Notes, "笔记库", IconName::BookOpen),
            (Page::Reviews, "复习计划", IconName::CircleCheck),
            (Page::Stats, "学习统计", IconName::ChartBar),
            (Page::Settings, "设置", IconName::Settings),
        ];
        column()
            .w(px(210.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(0xFFFFFF))
            .border_r_1()
            .border_color(rgb(BORDER))
            .p_5()
            .child(
                column()
                    .gap_1()
                    .py_5()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(GREEN))
                            .child("拾光学习"),
                    )
                    .child(muted("STUDYAPP  /  本地资料库")),
            )
            .children(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, (page, label, icon))| {
                        Button::new(("nav", i))
                            .label(label)
                            .icon(icon)
                            .ghost()
                            .selected(self.page == page)
                            .w_full()
                            .justify_start()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.page = page;
                                cx.notify();
                            }))
                    }),
            )
            .child(div().flex_1())
            .child(
                card()
                    .bg(rgb(LIGHT))
                    .p_3()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(GREEN))
                            .child("让学习留下痕迹"),
                    )
                    .child(muted("记录 · 回顾 · 再次掌握")),
            )
            .child(muted(format!(
                "v{}  ·  数据保存在此设备",
                env!("CARGO_PKG_VERSION")
            )))
    }
}

impl Render for StudyApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match self.page {
            Page::Today => "今天，学到了什么？",
            Page::Notes => "你的知识，持续生长",
            Page::Reviews => "把学过的，真正记住",
            Page::Stats => "看见每一次积累",
            Page::Settings => "按照你的节奏学习",
        };
        let body = if let Some(snapshot) = self.snapshot.clone() {
            match self.page {
                Page::Today => self.today(&snapshot, window, cx).into_any_element(),
                Page::Notes => self.notes(&snapshot, cx).into_any_element(),
                Page::Reviews => self.reviews(&snapshot, cx).into_any_element(),
                Page::Stats => self.statistics(&snapshot, cx).into_any_element(),
                Page::Settings => self.settings(&snapshot, cx).into_any_element(),
            }
        } else {
            empty("正在打开本地资料库", "所有学习内容保存在你选择的目录").into_any_element()
        };
        let notice = self.notice.as_ref().map(|value| {
            let (message, color) = match value {
                Ok(s) => (s.clone(), LIGHT),
                Err(s) => (s.clone(), 0xFFF0E9),
            };
            row()
                .w_full()
                .rounded_lg()
                .bg(rgb(color))
                .px_4()
                .py_3()
                .child(div().flex_1().text_sm().child(message))
                .child(
                    Button::new("dismiss-notice")
                        .ghost()
                        .small()
                        .label("关闭")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        })),
                )
        });
        row()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                if this
                    .worker
                    .sender
                    .send(Work::Scoped {
                        library: this.last_library.clone(),
                        work: Box::new(Work::ImportPaths(paths.paths().to_vec())),
                    })
                    .is_err()
                {
                    this.notice = Some(Err("后台服务已停止".into()));
                } else {
                    this.notice = Some(Ok("正在后台导入拖入的 Markdown 文件…".into()));
                }
                cx.notify();
            }))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.modifiers.control
                    && event.keystroke.key.eq_ignore_ascii_case("s")
                    && this.page == Page::Notes
                    && this.note_editor.is_some()
                {
                    this.save_note(cx);
                    cx.stop_propagation();
                }
            }))
            .gap_0()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(INK))
            .font_family("Microsoft YaHei UI")
            .text_sm()
            .child(self.navigation(cx))
            .child(
                column()
                    .gap_0()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        row()
                            .h(px(86.))
                            .flex_shrink_0()
                            .px_8()
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .child(
                                column()
                                    .gap_1()
                                    .flex_1()
                                    .child(
                                        div()
                                            .text_2xl()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(title),
                                    )
                                    .child(muted(
                                        Local::now().format("%Y 年 %m 月 %d 日").to_string(),
                                    )),
                            )
                            .child(Button::new("new-note").label("新建笔记").on_click(
                                cx.listener(|this, _, window, cx| this.new_note(window, cx)),
                            ))
                            .child(
                                Button::new("header-import")
                                    .icon(IconName::Plus)
                                    .primary()
                                    .label("导入笔记")
                                    .on_click(
                                        cx.listener(|this, _, _, _| this.picker("import", None)),
                                    ),
                            ),
                    )
                    .child(
                        column()
                            .id("page-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_8()
                            .children(notice)
                            .child(body),
                    )
                    .child(
                        row()
                            .px_8()
                            .py_2()
                            .border_t_1()
                            .border_color(rgb(BORDER))
                            .child(muted("本地保存"))
                            .child(div().flex_1())
                            .child(muted(format!(
                                "{}  ·  自动记录运行时间",
                                Utc::now().with_timezone(&Local).format("%H:%M")
                            ))),
                    ),
            )
    }
}
