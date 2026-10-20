use crate::{
    app::StudyApp,
    note_actions::{NoteAction, NoteActionKind},
};
use gpui_kit::*;
use studyapp_core::Command;
use studyapp_desktop::presentation::display_path;

impl StudyApp {
    pub fn handle_note_action(&mut self, action: NoteAction, cx: &mut Context<Self>) {
        if action.library_path != self.last_library {
            return;
        }
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let note = match action.resolve(snapshot, self.storage_busy) {
            Ok(note) => note,
            Err(message) => {
                self.notice = Some(Err(message.into()));
                cx.notify();
                return;
            }
        };
        match action.kind {
            NoteActionKind::MoveTo(folder_id) => {
                if !self.storage_busy && note.folder_id != folder_id {
                    self.send(
                        Command::MoveNote {
                            note_id: note.id.clone(),
                            folder_id,
                        },
                        cx,
                    );
                }
            }
            NoteActionKind::CopyPath => {
                cx.write_to_clipboard(ClipboardItem::new_string(display_path(&note.path)));
                self.notice = Some(Ok("已复制文件路径".into()));
            }
            NoteActionKind::Reveal => {
                let path = note.path.clone();
                let note_id = note.id.clone();
                let library = self.last_library.clone();
                cx.spawn(async move |this, cx| {
                    let checked_path = path.clone();
                    let exists =
                        cx.background_executor()
                            .spawn(async move {
                                std::fs::metadata(checked_path).is_ok_and(|m| m.is_file())
                            })
                            .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.last_library != library
                            || !this.snapshot.as_ref().is_some_and(|s| {
                                s.notes.iter().any(|n| n.id == note_id && n.path == path)
                            })
                        {
                            return;
                        }
                        if exists {
                            cx.reveal_path(std::path::Path::new(&display_path(&path)));
                        } else {
                            this.notice = Some(Err(
                                "原文件不存在或暂时无法访问；笔记历史仍可查看和导出。".into(),
                            ));
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        cx.notify();
    }
}
