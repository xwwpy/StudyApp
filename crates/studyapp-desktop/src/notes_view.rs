use crate::controls::Button;
use crate::{
    app::{EditorMode, StudyApp},
    components::*,
    views::note_tip,
    workspace_layout::Pane,
};
use gpui_kit::{
    component::{
        Disableable, Icon, IconName, Selectable, Sizable, button::ButtonVariants, input::Input,
        menu::ContextMenuExt,
    },
    prelude::FluentBuilder,
    *,
};
use studyapp_core::Snapshot;
use studyapp_desktop::presentation::{display_path, stamp};

impl StudyApp {
    pub fn notes(&self, snapshot: &Snapshot, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let layout = self.effective_layout(window);
        let query = self.search.read(cx).value().to_lowercase();
        let notes = snapshot
            .notes
            .iter()
            .filter(|n| {
                n.detached == self.show_detached && self.folders.read(cx).filter().matches(n)
            })
            .filter(|n| {
                query.is_empty()
                    || n.title.to_lowercase().contains(&query)
                    || n.path.to_lowercase().contains(&query)
                    || snapshot
                        .versions
                        .iter()
                        .rev()
                        .find(|v| v.note_id == n.id)
                        .is_some_and(|v| v.content.to_lowercase().contains(&query))
            })
            .collect::<Vec<_>>();
        let list = column()
            .id("notes-pane")
            .test_support()
            .w(layout.notes)
            .h_full()
            .flex_shrink_0()
            .min_h_0()
            .min_w_0()
            .gap_2()
            .bg(rgb(0xFFFFFF))
            .overflow_hidden()
            .child(
                row()
                    .px_3()
                    .h(px(46.))
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(Icon::new(IconName::FileText).small().text_color(rgb(GREEN)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("笔记"),
                    )
                    .child(pill(notes.len().to_string(), BG)),
            )
            .child(div().px_2().child(Input::new(&self.search).cleanable(true)))
            .child(
                Button::new("show-detached-notes")
                    .small()
                    .ghost()
                    .selected(self.show_detached)
                    .label(if self.show_detached {
                        "返回管理中的笔记"
                    } else {
                        "查看已取消管理"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_detached = !this.show_detached;
                        this.selected_note = this.snapshot.as_ref().and_then(|s| {
                            s.notes
                                .iter()
                                .find(|n| {
                                    n.detached == this.show_detached
                                        && this.folders.read(cx).filter().matches(n)
                                })
                                .map(|n| n.id.clone())
                        });
                        this.selected_version = None;
                        cx.notify();
                    })),
            )
            .child(muted("拖入 Markdown 或文件夹").text_xs().px_3())
            .child(
                Button::new("import-note-folder")
                    .small()
                    .ghost()
                    .label("导入文件夹")
                    .on_click(cx.listener(|this, _, _, _| this.picker("import-folder", None))),
            )
            .child(
                column()
                    .id("note-list-scroll")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .px_2()
                    .pb_2()
                    .gap_2()
                    .overflow_y_scroll()
                    .children(notes.into_iter().map(|note| {
                        let id = note.id.clone();
                        let drag = crate::note_actions::NoteDrag::new(note, snapshot);
                        let menu_note_id = id.clone();
                        let menu_owner = cx.weak_entity();
                        let tip = note_tip(snapshot, &id);
                        let chosen = self.selected_note.as_ref() == Some(&id);
                        card()
                            .id(SharedString::from(format!("note-{id}")))
                            .test_support()
                            .p_3()
                            .rounded_lg()
                            .flex_shrink_0()
                            .min_w_0()
                            .gap_2()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(BG)).border_color(rgb(0xCEDCD3)))
                            .when(chosen, |c| c.border_color(rgb(GREEN)).bg(rgb(LIGHT)))
                            .child(
                                row()
                                    .gap_2()
                                    .min_w_0()
                                    .child(
                                        Icon::new(IconName::FileText)
                                            .small()
                                            .text_color(rgb(GREEN)),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .truncate()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(note.title.clone()),
                                    ),
                            )
                            .child(
                                muted(format!(
                                    "{} 个版本  ·  {}",
                                    snapshot.versions.iter().filter(|v| v.note_id == id).count(),
                                    stamp(note.updated_at)
                                ))
                                .text_xs()
                                .truncate(),
                            )
                            .when(snapshot.settings.show_file_paths, |c| {
                                c.child(
                                    muted(display_path(&note.path))
                                        .id(SharedString::from(format!(
                                            "note-row-path-{}",
                                            note.id
                                        )))
                                        .test_support()
                                        .truncate(),
                                )
                            })
                            .when(note.missing, |c| c.child(pill("原文件失联", 0xFFF0E9)))
                            // Anchor within the sidebar so native editor children cannot
                            // cover learning/review timestamps near the row's right edge.
                            .tooltip(move |window, cx| sidebar_tooltip(tip.clone(), window, cx))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_note = Some(id.clone());
                                this.selected_version = None;
                                cx.notify();
                            }))
                            .on_drag(drag, |drag, source_offset, _, cx| {
                                cx.new(|_| {
                                    crate::note_actions::NoteDragPreview::new(drag, source_offset)
                                })
                            })
                            .context_menu(move |menu, window, cx| {
                                let opened = menu_owner.update(cx, |this, cx| {
                                    let snapshot = this.snapshot.clone()?;
                                    let note = snapshot
                                        .notes
                                        .iter()
                                        .find(|n| n.id == menu_note_id)?
                                        .clone();
                                    this.note_menu_sequence =
                                        this.note_menu_sequence.wrapping_add(1);
                                    let token = this.note_menu_sequence;
                                    this.active_note_menu = Some(token);
                                    this.note_content
                                        .update(cx, |editor, cx| editor.set_visible(false, cx));
                                    cx.notify();
                                    Some((snapshot, note, token, this.storage_busy))
                                });
                                let Ok(Some((snapshot, note, token, busy))) = opened else {
                                    return menu;
                                };
                                let dismissed = menu_owner.clone();
                                cx.subscribe_self(move |_, _: &DismissEvent, cx| {
                                    let _ = dismissed.update(cx, |this, cx| {
                                        if this.active_note_menu == Some(token) {
                                            this.active_note_menu = None;
                                            cx.notify();
                                        }
                                    });
                                })
                                .detach();
                                let released = menu_owner.clone();
                                cx.on_release(move |_, cx| {
                                    let _ = released.update(cx, |this, cx| {
                                        if this.active_note_menu == Some(token) {
                                            this.active_note_menu = None;
                                            cx.notify();
                                        }
                                    });
                                })
                                .detach();
                                let action_owner = menu_owner.clone();
                                crate::note_actions::note_context_menu(
                                    menu,
                                    &snapshot,
                                    &note,
                                    busy,
                                    window,
                                    cx,
                                    move |action, _, cx| {
                                        let _ = action_owner.update(cx, |this, cx| {
                                            this.handle_note_action(action, cx)
                                        });
                                    },
                                )
                            })
                    })),
            );
        let selected = snapshot
            .notes
            .iter()
            .find(|n| Some(&n.id) == self.selected_note.as_ref());
        let detail = if self.note_editor.is_some() {
            self.note_editor_view(cx)
        } else if let Some(note) = selected {
            let id = note.id.clone();
            let managed = note.managed && !note.detached;
            let move_id = id.clone();
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
            let historical =
                version.is_some_and(|v| versions.last().is_some_and(|last| last.id != v.id));
            let copy = !managed || historical;
            let mut reader = card()
                .flex_1()
                .h_full()
                .min_h_0()
                .min_w_0()
                .overflow_hidden()
                .rounded_none()
                .border_0()
                .p_4()
                .gap_2()
                .child(
                    muted(format!(
                        "{} / {}",
                        self.folders.read(cx).selection_label(),
                        note.title
                    ))
                    .text_xs()
                    .truncate()
                    .flex_shrink_0(),
                )
                .child(
                    row()
                        .flex_shrink_0()
                        .flex_wrap()
                        .child(
                            heading(note.title.clone())
                                .max_w(px(280.))
                                .min_w_0()
                                .truncate(),
                        )
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
                                .label(if historical {
                                    "复制此版本后编辑"
                                } else if managed {
                                    "编辑笔记"
                                } else {
                                    "复制后编辑"
                                })
                                .disabled(self.storage_busy)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_note_editor(copy, window, cx)
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
                        .child(
                            Button::new("move-note-folder")
                                .small()
                                .ghost()
                                .label("移动到目录")
                                .disabled(self.storage_busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.folders.update(cx, |panel, cx| {
                                        panel.begin_move_note(move_id.clone(), cx)
                                    });
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
                .when(snapshot.settings.show_file_paths, |c| {
                    c.child(
                        muted(display_path(&note.path))
                            .id("note-file-path")
                            .test_support()
                            .truncate()
                            .flex_shrink_0(),
                    )
                })
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
                        .map(|error| pill(error.clone(), 0xFFF0E9).truncate()),
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
                .child(
                    muted(note_tip(snapshot, &note.id).replace('\n', " · "))
                        .truncate()
                        .text_xs()
                        .flex_shrink_0(),
                );
            if self.history {
                // At narrow widths, a vertical version rail would leave the
                // actual diff only a few characters wide. Use a horizontal
                // strip above it until the document has room for both columns.
                let compact_history = window.viewport_size().width
                    - layout.navigation
                    - layout.folders
                    - layout.notes
                    < px(560.);
                let timeline = if compact_history {
                    row()
                        .id("history-versions-scroll")
                        .h(px(42.))
                        .w_full()
                        .overflow_x_scroll()
                } else {
                    column()
                        .id("history-versions-scroll")
                        .w(px(145.))
                        .h_full()
                        .overflow_y_scroll()
                }
                .flex_shrink_0()
                .min_h_0()
                .gap_2()
                .children(versions.iter().rev().map(|v| {
                    let id = v.id.clone();
                    let chosen = version.is_some_and(|selected| selected.id == id);
                    Button::new(SharedString::from(format!("version-{id}")))
                        .small()
                        .flex_shrink_0()
                        .selected(chosen)
                        .label(format!(
                            "{}{}",
                            stamp(v.captured_at),
                            if v.offline { " · 离线" } else { "" }
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected_version = Some(id.clone());
                            cx.notify();
                        }))
                }));
                let mut diff = column()
                    .flex_1()
                    .h_full()
                    .min_h_0()
                    .min_w_0()
                    .gap_2()
                    .overflow_hidden();
                if let Some(version) = version {
                    diff = diff
                        .child(muted(version.summary.clone()).truncate().flex_shrink_0())
                        .child(
                            muted(if version.offline {
                                "离线变更 · 不计入当前学习时段"
                            } else {
                                "+ 新增  /  − 删除 · 滚动查看全部差异"
                            })
                            .text_xs()
                            .flex_shrink_0(),
                        );
                    if version.diff.is_empty() {
                        diff = diff.child(muted("首次导入的基线快照，没有前一版本差异。"));
                    } else {
                        diff = diff.child(self.history_preview.render());
                    }
                }
                reader = reader.child(
                    (if compact_history {
                        column().gap_2()
                    } else {
                        row()
                    })
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_hidden()
                    .child(timeline)
                    .child(diff),
                );
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
        column().flex_1().h_full().min_h_0().min_w_0().gap_0()
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
                    .child(Button::new("recover-note-draft").disabled(self.storage_busy).primary().label("恢复草稿").on_click(cx.listener(|this, _, window, cx| this.recover_note(window, cx))))
                    .child(Button::new("discard-recovered-draft").disabled(self.storage_busy).ghost().label("删除草稿").on_click(cx.listener(|this, _, _, cx| {
                        this.confirmation = Some(crate::editor_lifecycle::NoteConfirmation::DiscardDraft);
                        cx.notify();
                    })))
                )))
            .child(row().items_stretch().gap_0().flex_1().min_h_0().min_w_0().w_full().overflow_hidden()
                .child(div().id("folders-pane").test_support().w(layout.folders).h_full().min_h_0().min_w_0()
                    .flex_shrink_0().bg(rgb(0xF7F9F6)).child(self.folders.clone()))
                .child(self.resize_handle(Pane::Folders, cx))
                .child(list)
                .child(self.resize_handle(Pane::Notes, cx))
                .child(div().id("document-pane").test_support().flex().flex_1().h_full().min_h_0().min_w_0().overflow_hidden().child(detail)))
    }
}
