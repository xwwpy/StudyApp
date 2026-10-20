use crate::{
    app::{Page, StudyApp},
    components::*,
    drafts::{DraftStore, DraftUpdate, NoteDraft},
};
use gpui_kit::{
    component::{
        Disableable, Sizable,
        button::{Button, ButtonVariants},
        input::Input,
    },
    prelude::FluentBuilder,
    *,
};
use studyapp_core::Command;

impl StudyApp {
    pub fn capture_note_draft(&mut self, cx: &mut Context<Self>) {
        self.draft_dirty_since = None;
        if let Some(draft) = self.note_editor.as_mut() {
            draft.title = self.note_title.read(cx).value().to_string();
            draft.content = self.note_content.read(cx).value().to_string();
            self.drafts.save(&self.last_library, Some(draft.clone()));
        }
    }

    pub fn poll_drafts(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        while let Ok(update) = self.drafts.receiver.try_recv() {
            match update {
                DraftUpdate::Loaded(path, draft)
                    if path == DraftStore::path(&self.last_library)
                        && self.draft_discard.is_none()
                        && self.note_editor.is_none() =>
                {
                    self.draft_loading = false;
                    self.recoverable_draft = draft;
                }
                DraftUpdate::Loaded(_, _) => {}
                DraftUpdate::Persisted(path, token) => {
                    if self
                        .draft_persist
                        .as_ref()
                        .is_some_and(|p| p.token == token && path == DraftStore::path(&p.library))
                    {
                        let pending = self.draft_persist.take().expect("matching persist");
                        self.finish_draft_persist(pending.intent, window, cx);
                    }
                }
                DraftUpdate::PersistFailed(path, token, error) => {
                    if self
                        .draft_persist
                        .as_ref()
                        .is_some_and(|p| p.token == token && path == DraftStore::path(&p.library))
                    {
                        self.draft_persist = None;
                        self.note_saving = false;
                        self.draft_failed = true;
                        self.close_requested = false;
                        self.note_content
                            .update(cx, |editor, cx| editor.set_readonly(false, cx));
                        self.notice =
                            Some(Err(format!("{error}；窗口及编辑内容已保留，请重试保存。")));
                    }
                }
                DraftUpdate::Discarded(path, token) => {
                    if self.draft_discard.as_ref().is_some_and(|pending| {
                        pending.token == token && path == DraftStore::path(&pending.library)
                    }) {
                        self.draft_discard = None;
                        self.draft_failed = false;
                        self.notice = Some(Ok("草稿已删除，原笔记和历史版本已保留。".into()));
                        self.finish_library_transition(window, cx);
                    }
                }
                DraftUpdate::DiscardFailed(path, token, error) => {
                    if self.draft_discard.as_ref().is_some_and(|pending| {
                        pending.token == token && path == DraftStore::path(&pending.library)
                    }) {
                        let pending = self.draft_discard.take().expect("matching discard");
                        if let Some(draft) = pending.draft {
                            self.start_note_draft(draft, window, cx);
                        }
                        self.close_requested = false;
                        self.notice = Some(Err(format!("{error}；草稿内容仍可恢复。")));
                    }
                }
                DraftUpdate::Error(path, error) if path == DraftStore::path(&self.last_library) => {
                    self.draft_loading = false;
                    self.draft_failed = true;
                    self.notice = Some(Err(error));
                }
                DraftUpdate::Error(_, _) => {}
            }
            changed = true;
        }
        if changed {
            cx.notify();
        }
        changed
    }

    fn start_note_draft(&mut self, draft: NoteDraft, window: &mut Window, cx: &mut Context<Self>) {
        self.confirmation = None;
        self.note_title
            .update(cx, |s, cx| s.set_value(draft.title.clone(), window, cx));
        self.note_content
            .update(cx, |s, cx| s.set_value(draft.content.clone(), window, cx));
        self.note_editor = Some(draft);
        self.reader_key.clear();
        self.history = false;
        self.note_saving = false;
        self.page = Page::Notes;
        self.capture_note_draft(cx);
    }

    pub fn open_note_editor(&mut self, copy: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.confirmation.is_some() {
            return;
        }
        self.page = Page::Notes;
        if self.note_editor.is_some() {
            cx.notify();
            return;
        }
        if self.draft_loading
            || self.draft_failed
            || self.recoverable_draft.is_some()
            || self.draft_discard.is_some()
        {
            self.notice = Some(Err(
                "请先恢复未保存草稿，保存或保留草稿后再开始另一份笔记。".into(),
            ));
            cx.notify();
            return;
        }
        let draft = self.snapshot.as_ref().and_then(|s| {
            let note = s
                .notes
                .iter()
                .find(|n| Some(&n.id) == self.selected_note.as_ref() && (!n.detached || copy))?;
            let version = s.versions.iter().rev().find(|v| {
                v.note_id == note.id
                    && (!copy || self.selected_version.as_ref().is_none_or(|id| id == &v.id))
            })?;
            Some(NoteDraft {
                note_id: (!copy).then(|| note.id.clone()),
                expected_hash: (!copy).then(|| version.hash.clone()),
                title: if copy {
                    format!("{}（副本）", note.title)
                } else {
                    note.title.clone()
                },
                content: version.content.clone(),
            })
        });
        if let Some(draft) = draft {
            self.start_note_draft(draft, window, cx);
        }
    }

    pub fn new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.confirmation.is_some() {
            return;
        }
        self.page = Page::Notes;
        if self.note_editor.is_some()
            || self.draft_loading
            || self.draft_failed
            || self.recoverable_draft.is_some()
            || self.draft_discard.is_some()
        {
            self.notice = Some(Err("已有未保存的笔记，请先恢复或保存草稿。".into()));
            cx.notify();
            return;
        }
        self.start_note_draft(
            NoteDraft {
                note_id: None,
                expected_hash: None,
                title: "新笔记".into(),
                content: String::new(),
            },
            window,
            cx,
        );
    }

    pub fn recover_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.note_editor.is_none()
            && self.confirmation.is_none()
            && self.draft_discard.is_none()
            && let Some(draft) = self.recoverable_draft.take()
        {
            self.start_note_draft(draft, window, cx);
        }
    }

    pub fn save_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request_note_action(
            crate::editor_lifecycle::NoteIntent::Save { copy: false },
            window,
            cx,
        );
    }

    pub fn commit_note(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.note_editor.clone() else {
            return;
        };
        let command = if let Some(note_id) = draft.note_id {
            Command::SaveNote {
                note_id,
                expected_hash: draft.expected_hash.unwrap_or_default(),
                content: draft.content,
            }
        } else {
            Command::CreateNote {
                title: draft.title,
                content: draft.content,
            }
        };
        self.note_saving = true;
        self.note_content
            .update(cx, |editor, cx| editor.set_readonly(true, cx));
        self.send(command, cx);
    }

    pub fn note_editor_view(&self, cx: &mut Context<Self>) -> Div {
        let draft = self.note_editor.as_ref().expect("visible note editor");
        let view = card()
            .id("note-editor")
            .flex_1()
            .min_h_0()
            .h_full()
            .min_w_0()
            .child(
                row()
                    .flex_wrap()
                    .child(heading(if draft.note_id.is_some() {
                        "编辑 Markdown"
                    } else {
                        "新建 Markdown 笔记"
                    }))
                    .child(div().flex_1())
                    .child(
                        Button::new("save-note")
                            .small()
                            .primary()
                            .disabled(self.note_saving)
                            .label(if self.note_saving {
                                "保存中…"
                            } else {
                                "保存版本 · Ctrl+S"
                            })
                            .on_click(
                                cx.listener(|this, _, window, cx| this.save_note(window, cx)),
                            ),
                    )
                    .when(draft.note_id.is_some(), |r| {
                        r.child(
                            Button::new("save-note-copy")
                                .small()
                                .ghost()
                                .disabled(self.note_saving)
                                .label("另存新笔记")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.request_note_action(
                                        crate::editor_lifecycle::NoteIntent::Save { copy: true },
                                        window,
                                        cx,
                                    );
                                })),
                        )
                    })
                    .child(
                        Button::new("keep-note-draft")
                            .small()
                            .ghost()
                            .disabled(self.note_saving)
                            .label("保留草稿并返回")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_note_action(
                                    crate::editor_lifecycle::NoteIntent::Keep,
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .child(
                        Button::new("discard-note-draft")
                            .small()
                            .ghost()
                            .disabled(self.note_saving || self.draft_discard.is_some())
                            .label("删除草稿")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirmation =
                                    Some(crate::editor_lifecycle::NoteConfirmation::DiscardDraft);
                                cx.notify();
                            })),
                    ),
            )
            .child(muted(
                "草稿自动保存在当前资料库；保存生成历史版本。修改笔记不会自动计为学习。",
            ))
            .child(div().id("note-title").child(
                Input::new(&self.note_title).disabled(self.note_saving || draft.note_id.is_some()),
            ))
            .child(self.note_content.clone());
        column().flex_1().min_h_0().h_full().min_w_0().child(view)
    }

    pub fn export_note(&self, note_id: String, version_id: Option<String>) {
        let sender = self.worker.sender.clone();
        let library = self.last_library.clone();
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("导出 Markdown（请选择新文件名）")
                .add_filter("Markdown", &["md"])
                .set_file_name("笔记快照.md")
                .save_file()
            {
                let _ = sender.send(crate::worker::Work::Scoped {
                    library,
                    work: Box::new(crate::worker::Work::Apply(Command::ExportNote {
                        note_id,
                        version_id,
                        path,
                    })),
                });
            }
        });
    }
}
