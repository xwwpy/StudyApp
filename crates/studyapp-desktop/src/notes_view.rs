use crate::{
    app::{EditorMode, StudyApp},
    components::*,
    views::note_tip,
};
use gpui_kit::{
    component::{
        Disableable, Selectable, Sizable,
        button::{Button, ButtonVariants},
        input::Input,
        tooltip::Tooltip,
    },
    prelude::FluentBuilder,
    *,
};
use studyapp_core::Snapshot;
use studyapp_desktop::presentation::stamp;

impl StudyApp {
    pub fn notes(&self, snapshot: &Snapshot, cx: &mut Context<Self>) -> Div {
        let query = self.search.read(cx).value().to_lowercase();
        let notes = snapshot.notes.iter().filter(|n| !n.detached).filter(|n| {
            query.is_empty()
                || n.title.to_lowercase().contains(&query)
                || n.path.to_lowercase().contains(&query)
                || snapshot
                    .versions
                    .iter()
                    .rev()
                    .find(|v| v.note_id == n.id)
                    .is_some_and(|v| v.content.to_lowercase().contains(&query))
        });
        let list = column()
            .id("note-list-scroll")
            .w(px(265.))
            .h_full()
            .min_h_0()
            .overflow_y_scroll()
            .flex_shrink_0()
            .child(Input::new(&self.search).cleanable(true))
            .child(muted("可拖入多个 Markdown 或目录（最多 200 份，8 层）。"))
            .child(
                Button::new("import-note-folder")
                    .small()
                    .ghost()
                    .label("导入文件夹")
                    .on_click(cx.listener(|this, _, _, _| this.picker("import-folder", None))),
            )
            .children(notes.map(|note| {
                let id = note.id.clone();
                let tip = note_tip(snapshot, &id);
                let chosen = self.selected_note.as_ref() == Some(&id);
                card()
                    .id(SharedString::from(format!("note-{id}")))
                    .p_4()
                    .gap_2()
                    .cursor_pointer()
                    .when(chosen, |c| c.border_color(rgb(GREEN)).bg(rgb(LIGHT)))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(note.title.clone()),
                    )
                    .child(muted(format!(
                        "{} 个版本  ·  {}",
                        snapshot.versions.iter().filter(|v| v.note_id == id).count(),
                        stamp(note.updated_at)
                    )))
                    .when(note.missing, |c| c.child(pill("原文件失联", 0xFFF0E9)))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_note = Some(id.clone());
                        this.selected_version = None;
                        cx.notify();
                    }))
            }));
        let selected = snapshot
            .notes
            .iter()
            .find(|n| Some(&n.id) == self.selected_note.as_ref());
        let detail = if self.note_editor.is_some() {
            self.note_editor_view(cx)
        } else if let Some(note) = selected {
            let id = note.id.clone();
            let managed = note.managed && !note.detached;
            let unmanage_id = id.clone();
            let export_id = id.clone();
            let export_version = self.selected_version.clone();
            let versions: Vec<_> = snapshot
                .versions
                .iter()
                .filter(|v| v.note_id == id)
                .collect();
            let version = versions
                .iter()
                .rev()
                .find(|v| self.selected_version.as_ref().is_none_or(|id| id == &v.id));
            let mut reader = card()
                .flex_1()
                .h_full()
                .min_h_0()
                .min_w_0()
                .child(
                    row()
                        .flex_wrap()
                        .child(heading(note.title.clone()))
                        .child(div().flex_1())
                        .child(
                            Button::new("record-note")
                                .small()
                                .primary()
                                .label("记录学习")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_editor(EditorMode::Manual, window, cx)
                                })),
                        )
                        .child(
                            Button::new("edit-note")
                                .small()
                                .ghost()
                                .label(if managed {
                                    "编辑笔记"
                                } else {
                                    "复制后编辑"
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_note_editor(!managed, window, cx)
                                })),
                        )
                        .child(
                            Button::new("export-note")
                                .small()
                                .ghost()
                                .label("导出 Markdown")
                                .on_click(cx.listener(move |this, _, _, _| {
                                    this.export_note(export_id.clone(), export_version.clone())
                                })),
                        )
                        .when(!note.detached, |r| {
                            r.child(
                                Button::new("unmanage-note")
                                    .small()
                                    .ghost()
                                    .disabled(self.sync_panel.read(cx).status.busy)
                                    .label("取消管理")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.confirmation = Some(
                                            crate::editor_lifecycle::NoteConfirmation::Unmanage(
                                                unmanage_id.clone(),
                                            ),
                                        );
                                        cx.notify();
                                    })),
                            )
                        })
                        .when(!note.managed && !note.detached, |r| {
                            r.child(
                                Button::new("relink-note")
                                    .small()
                                    .ghost()
                                    .label("重新关联")
                                    .on_click(cx.listener(move |this, _, _, _| {
                                        this.picker("relink", Some(id.clone()))
                                    })),
                            )
                        }),
                )
                .when(note.detached, |c| {
                    c.child(muted("已取消管理 · 当前仅查看历史快照"))
                })
                .child(muted(note.path.clone()))
                .child(muted(format!(
                    "{} · 创建：{} · 修改：{}",
                    if note.managed {
                        "托管笔记"
                    } else {
                        "外部文件 · 只读"
                    },
                    stamp(note.created_at),
                    stamp(note.updated_at)
                )))
                .children(
                    note.error
                        .as_ref()
                        .map(|error| pill(error.clone(), 0xFFF0E9)),
                )
                .child(
                    row()
                        .child(
                            Button::new("preview-tab")
                                .small()
                                .ghost()
                                .selected(!self.history)
                                .label("阅读笔记")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.history = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("history-tab")
                                .small()
                                .ghost()
                                .selected(self.history)
                                .label(format!("变更历史 ({})", versions.len()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.history = true;
                                    cx.notify();
                                })),
                        ),
                )
                .child(muted(note_tip(snapshot, &note.id)));
            if self.history {
                reader = reader.child(row().flex_wrap().children(versions.iter().rev().map(|v| {
                    let id = v.id.clone();
                    Button::new(SharedString::from(format!("version-{id}")))
                        .small()
                        .selected(self.selected_version.as_ref() == Some(&id))
                        .label(format!(
                            "{}{}",
                            stamp(v.captured_at),
                            if v.offline { " · 离线" } else { "" }
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected_version = Some(id.clone());
                            cx.notify();
                        }))
                })));
                if let Some(version) = version {
                    reader = reader
                        .child(muted(version.summary.clone()))
                        .child(muted(if version.offline {
                            "离线期间的变更仅形成历史，不算作当前学习时间段。"
                        } else {
                            "新增行以 + 标记，删除行以 − 标记。"
                        }))
                        .child(
                            column().gap_0().p_4().bg(rgb(BG)).rounded_lg().children(
                                version
                                    .diff
                                    .lines()
                                    .take(1500)
                                    .enumerate()
                                    .map(|(i, line)| {
                                        let color = if line.starts_with('+') {
                                            GREEN
                                        } else if line.starts_with('-') {
                                            0xB45142
                                        } else {
                                            MUTED
                                        };
                                        div()
                                            .id(("diff-line", i))
                                            .font_family("Consolas")
                                            .text_xs()
                                            .text_color(rgb(color))
                                            .child(line.to_owned())
                                    }),
                            ),
                        );
                    if version.diff.is_empty() {
                        reader = reader.child(muted("首次导入的基线快照，没有前一版本差异。"));
                    }
                    if version.diff.lines().count() > 1500 {
                        reader =
                            reader.child(muted("差异预览显示前 1,500 行；完整内容保存在快照中。"));
                    }
                }
            } else {
                reader = reader.child(self.note_content.clone());
            }
            reader
        } else {
            empty(
                "选择一份笔记",
                "在左侧选择笔记，阅读内容或查看每次更改。悬停可查看学习与复习时间。",
            )
            .flex_1()
        };
        column().flex_1().h_full().min_h_0()
            .when(self.confirmation.is_some(), |c| c.child(self.confirmation_view(cx)))
            .when(self.draft_discard.is_some(), |c| c.child(muted("正在删除草稿…")))
            .when(self.draft_loading, |c| c.child(muted("正在检查本地草稿…")))
            .when(self.draft_failed, |c| c.child(card().child(heading("草稿文件暂时无法读取或保存"))
                .child(muted("为保护已有草稿，暂时不能开始另一份笔记。修复目录权限后可重试；当前编辑内容仍可保存为笔记。"))
                .child(Button::new("retry-note-draft").label("重试草稿读取").on_click(cx.listener(|this, _, _, cx| {
                    if this.note_editor.is_none() { this.draft_failed = false; this.draft_loading = true; this.drafts.load(&this.last_library); }
                    cx.notify();
                })))))
            .when(self.recoverable_draft.is_some(), |c| c.child(card().id("draft-recovery").child(heading("发现未保存的笔记草稿"))
                .child(muted("草稿仍保留在当前资料库，恢复后可继续编辑或另存为新笔记。"))
                .child(row()
                    .child(Button::new("recover-note-draft").primary().label("恢复草稿").on_click(cx.listener(|this, _, window, cx| this.recover_note(window, cx))))
                    .child(Button::new("discard-recovered-draft").ghost().label("删除草稿").on_click(cx.listener(|this, _, _, cx| {
                        this.confirmation = Some(crate::editor_lifecycle::NoteConfirmation::DiscardDraft);
                        cx.notify();
                    })))
                )))
            .child(row().items_start().flex_1().min_h_0().w_full().child(list).child(detail))
    }
}
