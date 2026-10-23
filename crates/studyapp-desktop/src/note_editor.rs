use crate::controls::Button;
use crate::{
    app::{Page, StudyApp},
    components::*,
    drafts::{DraftStore, DraftUpdate, NoteDraft},
};
use gpui_kit::{
    component::{
        Disableable, Sizable,
        button::{Button as MenuButton, ButtonVariants},
        input::Input,
        menu::{DropdownMenu, PopupMenuItem},
    },
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
                        self.push_notice(
                            Err(format!("{error}；窗口及编辑内容已保留，请重试保存。")),
                            cx,
                        );
                    }
                }
                DraftUpdate::Discarded(path, token) => {
                    if self.draft_discard.as_ref().is_some_and(|pending| {
                        pending.token == token && path == DraftStore::path(&pending.library)
                    }) {
                        self.draft_discard = None;
                        self.draft_failed = false;
                        self.push_notice(Ok("草稿已删除，原笔记和历史版本已保留。".into()), cx);
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
                        self.push_notice(Err(format!("{error}；草稿内容仍可恢复。")), cx);
                    }
                }
                DraftUpdate::Error(path, error) if path == DraftStore::path(&self.last_library) => {
                    self.draft_loading = false;
                    self.draft_failed = true;
                    self.push_notice(Err(error), cx);
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

    pub(crate) fn start_note_draft(
        &mut self,
        mut draft: NoteDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if draft.note_id.is_some() || draft.asset_group.is_empty() {
            draft.asset_group =
                crate::media_ui::asset_group(draft.note_id.as_deref(), &draft.content);
        }
        if draft.folder_id.as_ref().is_some_and(|id| {
            self.snapshot
                .as_ref()
                .is_none_or(|s| !s.folders.iter().any(|f| &f.id == id))
        }) {
            draft.folder_id = None;
            self.push_notice(Ok("草稿原目录已不存在，已转为未分类。".into()), cx);
        }
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
        if self.confirmation.is_some() || self.storage_busy || self.media_busy() {
            return;
        }
        self.page = Page::Notes;
        if self.note_editor.is_some() {
            cx.notify();
            return;
        }
        if !copy
            && self
                .recoverable_draft
                .as_ref()
                .is_some_and(|d| d.note_id.is_some() && d.note_id == self.selected_note)
        {
            self.recover_note(window, cx);
            return;
        }
        if self.draft_loading
            || self.draft_failed
            || self.recoverable_draft.is_some()
            || self.draft_discard.is_some()
        {
            self.push_notice(
                Err("请先恢复未保存草稿，保存或保留草稿后再开始另一份笔记。".into()),
                cx,
            );
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
                asset_group: if copy {
                    crate::media::new_group()
                } else {
                    crate::media_ui::asset_group(Some(&note.id), &version.content)
                },
                note_id: (!copy).then(|| note.id.clone()),
                expected_hash: (!copy).then(|| version.hash.clone()),
                folder_id: note.folder_id.clone(),
                title: if copy {
                    format!("{}（副本）", note.title)
                } else {
                    note.title.clone()
                },
                content: version.content.clone(),
            })
        });
        if let Some(draft) = draft {
            if copy {
                let source_path = self.snapshot.as_ref().and_then(|snapshot| {
                    snapshot
                        .notes
                        .iter()
                        .find(|note| Some(&note.id) == self.selected_note.as_ref())
                        .map(|note| std::path::PathBuf::from(&note.path))
                });
                self.begin_media_adoption(draft, source_path, window, cx);
            } else {
                self.start_note_draft(draft, window, cx);
            }
        }
    }

    pub fn new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.guard_settings(crate::settings_ui::SettingsContinuation::NewNote, cx) {
            return;
        }
        #[cfg(feature = "ui-test")]
        eprintln!("native new_note: requested");
        if self.confirmation.is_some() || self.storage_busy || self.media_busy() {
            return;
        }
        self.page = Page::Notes;
        if self.note_editor.is_some()
            || self.draft_loading
            || self.draft_failed
            || self.recoverable_draft.is_some()
            || self.draft_discard.is_some()
        {
            self.push_notice(Err("已有未保存的笔记，请先恢复或保存草稿。".into()), cx);
            cx.notify();
            return;
        }
        self.start_note_draft(
            NoteDraft {
                asset_group: crate::media::new_group(),
                note_id: None,
                expected_hash: None,
                folder_id: self
                    .folders
                    .read(cx)
                    .filter()
                    .folder_id()
                    .map(str::to_owned),
                title: "新笔记".into(),
                content: String::new(),
            },
            window,
            cx,
        );
        #[cfg(feature = "ui-test")]
        eprintln!("native new_note: draft activated");
    }

    pub fn recover_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.storage_busy
            && !self.media_busy()
            && self.note_editor.is_none()
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
            Command::CreateNoteWithId {
                note_id: draft.asset_group,
                folder_id: draft.folder_id,
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
        let busy = self.note_saving || self.storage_busy || self.media_busy();
        let owner = cx.weak_entity();
        let overflow = MenuButton::new("note-editor-menu")
            .small()
            .ghost()
            .label("⋯")
            .accessibility_label("笔记更多操作")
            .tooltip("笔记更多操作")
            .disabled(busy)
            .dropdown_menu(move |mut menu, _, cx| {
                let Some((_, _, busy)) = Self::library_menu_state(&owner, cx) else {
                    return menu;
                };
                let can_copy = owner
                    .update(cx, |this, _| {
                        this.note_editor
                            .as_ref()
                            .is_some_and(|draft| draft.note_id.is_some())
                    })
                    .unwrap_or(false);
                if can_copy {
                    let copy = owner.clone();
                    menu = menu.item(PopupMenuItem::new("另存新笔记").disabled(busy).on_click(
                        move |_, window, cx| {
                            let _ = copy.update(cx, |this, cx| {
                                this.request_note_action(
                                    crate::editor_lifecycle::NoteIntent::Save { copy: true },
                                    window,
                                    cx,
                                )
                            });
                        },
                    ));
                }
                let keep = owner.clone();
                let discard = owner.clone();
                menu.min_w(px(188.))
                    .item(
                        PopupMenuItem::new("保留草稿并返回")
                            .disabled(busy)
                            .on_click(move |_, window, cx| {
                                let _ = keep.update(cx, |this, cx| {
                                    this.request_note_action(
                                        crate::editor_lifecycle::NoteIntent::Keep,
                                        window,
                                        cx,
                                    )
                                });
                            }),
                    )
                    .separator()
                    .item(PopupMenuItem::new("删除草稿").disabled(busy).on_click(
                        move |_, _, cx| {
                            let _ = discard.update(cx, |this, cx| {
                                if !this.note_saving && !this.storage_busy && !this.media_busy() {
                                    this.confirmation = Some(
                                        crate::editor_lifecycle::NoteConfirmation::DiscardDraft,
                                    );
                                    cx.notify();
                                }
                            });
                        },
                    ))
            });
        let view = card()
            .id("note-editor")
            .flex_1()
            .min_h_0()
            .h_full()
            .min_w_0()
            .child(
                row()
                    .id("note-editor-toolbar")
                    .flex_shrink_0()
                    .child(div().id("note-title").flex_1().min_w_0().child(
                        Input::new(&self.note_title).disabled(busy || draft.note_id.is_some()),
                    ))
                    .child(
                        Button::new("save-note")
                            .loading(self.note_saving || self.media_busy())
                            .small()
                            .primary()
                            .disabled(busy)
                            .label(if self.note_saving {
                                "保存中…"
                            } else if self.media_busy() {
                                "图片处理中…"
                            } else {
                                "保存"
                            })
                            .on_click(
                                cx.listener(|this, _, window, cx| this.save_note(window, cx)),
                            ),
                    )
                    .child(overflow),
            )
            .children(self.snapshot.as_ref().and_then(|snapshot| {
                if !snapshot.settings.show_file_paths {
                    return None;
                }
                draft.note_id.as_ref().and_then(|id| {
                    snapshot
                        .notes
                        .iter()
                        .find(|note| &note.id == id)
                        .map(|note| {
                            muted(studyapp_desktop::presentation::display_path(&note.path))
                                .id("editor-file-path")
                                .test_support()
                                .truncate()
                                .flex_shrink_0()
                        })
                })
            }))
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
