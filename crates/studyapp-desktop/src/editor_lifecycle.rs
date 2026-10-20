use crate::controls::Button;
use crate::{
    app::{Page, StudyApp},
    components::*,
    drafts::NoteDraft,
    markdown_editor::MarkdownEvent,
};
use gpui_kit::{
    component::{Disableable, Sizable, button::ButtonVariants},
    *,
};
use std::sync::atomic::{AtomicU64, Ordering};
use studyapp_core::Command;

#[derive(Clone)]
pub enum NoteIntent {
    Save { copy: bool },
    Keep,
    Discard,
    SwitchLibrary,
    PrepareMigration,
    Close,
}

#[derive(Clone)]
pub enum NoteConfirmation {
    DiscardDraft,
    Unmanage(String),
}

pub struct DiscardPending {
    pub token: u64,
    pub library: String,
    pub draft: Option<NoteDraft>,
}
pub struct PersistPending {
    pub token: u64,
    pub library: String,
    pub intent: NoteIntent,
}

impl StudyApp {
    pub fn mark_note_dirty(&mut self) {
        if self.note_editor.is_some() && self.draft_discard.is_none() {
            self.draft_dirty_since
                .get_or_insert_with(std::time::Instant::now);
        }
    }

    pub fn request_note_action(
        &mut self,
        intent: NoteIntent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.note_editor.is_none()
            || self.storage_busy
            || self.note_saving
            || self.draft_discard.is_some()
            || (self.confirmation.is_some()
                && !matches!(
                    intent,
                    NoteIntent::Discard | NoteIntent::Close | NoteIntent::SwitchLibrary
                ))
        {
            return;
        }
        self.note_saving = true;
        match self
            .note_content
            .update(cx, |editor, cx| editor.request_flush(cx))
        {
            Ok(request) => self.note_intent = Some((request, intent)),
            Err(error) => {
                self.note_saving = false;
                self.notice = Some(Err(error.to_string()));
            }
        }
        cx.notify();
    }

    pub fn editor_event(
        &mut self,
        event: MarkdownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            MarkdownEvent::Save => self.save_note(window, cx),
            MarkdownEvent::Error(error) => {
                self.note_intent = None;
                self.note_saving = false;
                self.close_requested = false;
                self.notice = Some(Err(error));
                cx.notify();
            }
            MarkdownEvent::Flushed(request) => {
                if self
                    .note_intent
                    .as_ref()
                    .is_none_or(|(id, _)| *id != request)
                {
                    return;
                }
                let (_, intent) = self.note_intent.take().expect("matching editor intent");
                self.capture_note_draft(cx);
                match intent {
                    NoteIntent::Save { copy } => {
                        if copy && let Some(draft) = &mut self.note_editor {
                            draft.note_id = None;
                            draft.expected_hash = None;
                        }
                        self.commit_note(cx);
                    }
                    NoteIntent::Discard => {
                        self.note_saving = false;
                        self.begin_discard(window, cx);
                    }
                    NoteIntent::Keep
                    | NoteIntent::SwitchLibrary
                    | NoteIntent::Close
                    | NoteIntent::PrepareMigration => {
                        static NEXT_PERSIST: AtomicU64 = AtomicU64::new(1);
                        let token = NEXT_PERSIST.fetch_add(1, Ordering::Relaxed);
                        self.draft_persist = Some(PersistPending {
                            token,
                            library: self.last_library.clone(),
                            intent,
                        });
                        self.drafts
                            .persist(&self.last_library, self.note_editor.clone(), token);
                    }
                }
                cx.notify();
            }
        }
    }

    pub fn finish_draft_persist(
        &mut self,
        intent: NoteIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_saving = false;
        self.draft_failed = false;
        self.confirmation = None;
        match intent {
            NoteIntent::Keep => {
                self.recoverable_draft = self.note_editor.take();
                self.reader_key.clear();
                self.finish_library_transition(window, cx);
            }
            NoteIntent::SwitchLibrary => {
                self.note_editor = None;
                self.recoverable_draft = None;
                self.finish_library_transition(window, cx);
            }
            NoteIntent::PrepareMigration => {
                self.recoverable_draft = self.note_editor.take();
                self.reader_key.clear();
                self.pick_migration(window, cx);
            }
            NoteIntent::Close => {
                self.close_allowed = true;
                window.remove_window();
            }
            _ => {}
        }
    }

    pub fn finish_library_transition(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.note_editor.is_none()
            && self.draft_discard.is_none()
            && self.draft_persist.is_none()
            && let Some((snapshot, replaced)) = self.pending_library.take()
        {
            self.accept_snapshot(snapshot, replaced, window, cx);
        }
    }

    pub fn handle_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.close_allowed {
            return true;
        }
        if !self.note_content.read(cx).creating()
            && !self.storage_busy
            && self.note_editor.is_none()
            && self.draft_discard.is_none()
        {
            return true;
        }
        self.close_requested = true;
        self.continue_close(window, cx);
        false
    }

    pub fn continue_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.close_requested
            || self.close_allowed
            || self.note_saving
            || self.draft_discard.is_some()
            || self.note_content.read(cx).creating()
            || self.storage_busy
        {
            return;
        }
        if self.note_editor.is_some() {
            self.request_note_action(NoteIntent::Close, window, cx);
        } else {
            self.close_allowed = true;
            window.remove_window();
        }
    }

    pub fn begin_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.storage_busy
            || self.note_saving
            || self.draft_loading
            || self.draft_discard.is_some()
        {
            return;
        }
        static NEXT_DISCARD: AtomicU64 = AtomicU64::new(1);
        let token = NEXT_DISCARD.fetch_add(1, Ordering::Relaxed);
        let draft = self
            .note_editor
            .take()
            .or_else(|| self.recoverable_draft.take());
        if draft.is_none() {
            return;
        }
        self.draft_discard = Some(DiscardPending {
            token,
            library: self.last_library.clone(),
            draft,
        });
        self.recoverable_draft = None;
        self.draft_dirty_since = None;
        // Remove draft ownership before resetting inputs; blur and delayed IPC
        // cannot enqueue another save or revive the discarded document.
        self.note_content.update(cx, |editor, cx| {
            editor.set_visible(false, cx);
            editor.set_value(String::new(), window, cx);
        });
        self.reader_key.clear();
        self.drafts.discard(&self.last_library, token);
        cx.notify();
    }

    pub fn confirm_note_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.storage_busy || self.note_saving {
            return;
        }
        match self.confirmation.take() {
            Some(NoteConfirmation::DiscardDraft) if self.note_editor.is_some() => {
                self.request_note_action(NoteIntent::Discard, window, cx)
            }
            Some(NoteConfirmation::DiscardDraft) => self.begin_discard(window, cx),
            Some(NoteConfirmation::Unmanage(note_id)) => {
                self.send(Command::UnmanageNote { note_id }, cx);
            }
            None => {}
        }
        cx.notify();
    }

    /// Dialog content only; the window-level shell owns blocking and focus.
    pub fn confirmation_view(&self, cx: &mut Context<Self>) -> Div {
        let message = match self.confirmation.as_ref() {
            Some(NoteConfirmation::DiscardDraft) => {
                "删除这份未保存的草稿？原笔记文件和已保存的历史版本会保留。"
            }
            Some(NoteConfirmation::Unmanage(_)) => {
                "取消管理这份笔记？将停止扫描和同步，磁盘文件、历史版本及学习复习记录会保留。"
            }
            None => return div(),
        };
        column()
            .child(heading("确认操作"))
            .child(div().child(message))
            .child(
                row()
                    .child(
                        Button::new("confirm-note-action")
                            .disabled(self.storage_busy || self.note_saving)
                            .small()
                            .primary()
                            .label("确认")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm_note_action(window, cx)
                            })),
                    )
                    .child(
                        Button::new("cancel-note-action")
                            .small()
                            .ghost()
                            .label("取消")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirmation = None;
                                cx.notify();
                            })),
                    ),
            )
    }

    /// Load a read-only document into the same persistent browser surface.
    /// Reading and writing therefore share highlighting and viewport limits.
    pub fn prepare_document_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut show = self.page == Page::Notes
            && self.confirmation.is_none()
            && self.storage_confirm.is_none()
            && !self.folders.read(cx).modal_open()
            && !self.is_layout_resizing()
            && !cx.has_active_drag()
            && self.draft_discard.is_none()
            && self.active_note_menu.is_none();
        if self.note_editor.is_none() {
            let version = self.snapshot.as_ref().and_then(|s| {
                s.versions.iter().rev().find(|v| {
                    Some(&v.note_id) == self.selected_note.as_ref()
                        && self.selected_version.as_ref().is_none_or(|id| id == &v.id)
                })
            });
            if self.history {
                self.history_preview
                    .prepare(version, f32::from(window.viewport_size().width));
            }
            show &= version.is_some() && !self.history;
            if show
                && let Some(version) = version
                && self.reader_key != version.id
            {
                let text = version.content.clone();
                self.reader_key = version.id.clone();
                self.note_content.update(cx, |editor, cx| {
                    editor.set_value(text, window, cx);
                    editor.set_readonly(true, cx);
                });
            }
        }
        self.note_content
            .update(cx, |editor, cx| editor.set_visible(show, cx));
    }
}
