use crate::controls::Button;
use crate::{
    app::{EditorMode, StudyApp},
    components::*,
    workspace_layout::Pane,
};
use gpui_kit::{
    component::{
        Disableable, Icon, IconName, Selectable, Sizable,
        button::{Button as MenuButton, ButtonVariants},
        input::Input,
        menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem},
    },
    prelude::FluentBuilder,
    *,
};
use studyapp_core::Snapshot;
use studyapp_desktop::presentation::{display_path, stamp};

#[derive(Clone, Copy)]
enum ReaderAction {
    Export,
    Move,
    Unmanage,
    Relink,
}

/// The host places this above its normal content and hides the existing native
/// child for the duration of an external hover. Rendering does not own the drag
/// payload, inspect paths, or touch the editor document/selection.
pub fn external_drop_overlay(
    state: &crate::external_drop::ExternalDropState,
    cx: &Context<StudyApp>,
) -> impl IntoElement {
    use crate::external_drop::DropBlock;
    let (title, hint, color) = match state.blocked() {
        Some(DropBlock::Busy) => ("暂时无法导入", "资料库正在处理中，请稍后重试", MUTED),
        Some(DropBlock::Modal) => ("先完成当前操作", "关闭对话框后，再拖入文件", MUTED),
        None => ("松开以导入", "Markdown、图片或文件夹", GREEN),
    };
    div()
        .id("external-drop-overlay")
        .test_support()
        .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
            this.drop_external_files(paths.paths().to_vec(), window, cx);
        }))
        .absolute()
        .inset_0()
        .p_4()
        .flex()
        .items_center()
        .justify_center()
        .bg(crate::theme::color(BG).opacity(0.96))
        .child(
            column()
                .id("external-drop-surface")
                .test_support()
                .w_full()
                .h_full()
                .min_w_0()
                .min_h_0()
                .gap_2()
                .items_center()
                .justify_center()
                .rounded_xl()
                .border_2()
                .border_color(crate::theme::color(color))
                .bg(crate::theme::color(0xFFFFFF))
                .child(
                    div()
                        .id("external-drop-plus")
                        .test_support()
                        .text_size(px(76.))
                        .line_height(px(84.))
                        .text_color(crate::theme::color(color))
                        .child("＋"),
                )
                .child(heading(title).text_color(crate::theme::color(INK)))
                .child(muted(hint))
                .child(muted(format!("{} 个项目", state.item_count())).text_xs()),
        )
}

impl StudyApp {
    /// Track only the menu's actual native occlusion. The document stays live;
    /// the token prevents a released old menu from closing a newer popup.
    pub(crate) fn library_menu_state(
        owner: &WeakEntity<Self>,
        cx: &mut Context<PopupMenu>,
    ) -> Option<(studyapp_core::LibrarySort, bool, bool)> {
        let opened = owner
            .update(cx, |this, cx| {
                this.snapshot.as_ref()?;
                this.note_menu_sequence = this.note_menu_sequence.wrapping_add(1);
                let token = this.note_menu_sequence;
                this.active_note_menu = Some(token);
                cx.notify();
                Some((
                    token,
                    this.layout.library_sort,
                    this.show_detached,
                    this.library_controls_busy(cx),
                ))
            })
            .ok()??;
        let (token, sort, detached, busy) = opened;
        Self::track_menu_surface(owner.clone(), token, cx);
        let dismissed = owner.clone();
        cx.subscribe_self(move |_, _: &DismissEvent, cx| {
            let _ = dismissed.update(cx, |this, cx| {
                if this.active_note_menu == Some(token) {
                    this.active_note_menu = None;
                    this.clear_menu_surface(cx);
                    cx.notify();
                }
            });
        })
        .detach();
        let released = owner.clone();
        cx.on_release(move |_, cx| {
            let _ = released.update(cx, |this, cx| {
                if this.active_note_menu == Some(token) {
                    this.active_note_menu = None;
                    this.clear_menu_surface(cx);
                    cx.notify();
                }
            });
        })
        .detach();
        Some((sort, detached, busy))
    }

    fn clear_menu_surface(&mut self, cx: &mut Context<Self>) {
        self.note_content.update(cx, |editor, cx| {
            editor.set_overlay_regions("note-menu", vec![], cx);
        });
    }

    fn track_menu_surface(owner: WeakEntity<Self>, token: u64, cx: &mut Context<PopupMenu>) {
        cx.subscribe_self(
            move |_, event: &gpui_kit::component::menu::PopupMenuLayout, cx| {
                let _ = owner.update(cx, |this, cx| {
                    if this.active_note_menu == Some(token) {
                        this.note_content.update(cx, |editor, cx| {
                            editor.set_overlay_regions("note-menu", event.0.clone(), cx);
                        });
                    }
                });
            },
        )
        .detach();
    }

    pub(crate) fn library_controls_busy(&self, cx: &App) -> bool {
        self.storage_busy
            || self.media_busy()
            || self.preferences_ui.busy
            || self.preferences_ui.replacing
            || self.note_saving
            || self.has_modal(cx)
    }

    pub(crate) fn library_add_button(&self, cx: &Context<Self>) -> AnyElement {
        let owner = cx.weak_entity();
        MenuButton::new("library-add-menu")
            .small()
            .ghost()
            .icon(IconName::Plus)
            .accessibility_label("新建或导入")
            .tooltip("新建或导入")
            .cursor_pointer()
            .disabled(self.library_controls_busy(cx))
            .dropdown_menu(move |menu, _, cx| {
                let Some((_, _, busy)) = Self::library_menu_state(&owner, cx) else {
                    return menu;
                };
                let create_note = owner.clone();
                let create_folder = owner.clone();
                let import_files = owner.clone();
                let import_folder = owner.clone();
                let can_create = owner
                    .update(cx, |this, cx| this.folders.read(cx).can_create_folder())
                    .unwrap_or(false);
                menu.min_w(px(196.))
                    .item(
                        PopupMenuItem::new("新建 Markdown")
                            .icon(IconName::FileText)
                            .disabled(busy)
                            .on_click(move |_, window, cx| {
                                let _ = create_note.update(cx, |this, cx| {
                                    if !this.library_controls_busy(cx) {
                                        this.new_note(window, cx);
                                    }
                                });
                            }),
                    )
                    .item(
                        PopupMenuItem::new("新建文件夹")
                            .icon(IconName::Folder)
                            .disabled(busy || !can_create)
                            .on_click(move |_, window, cx| {
                                let _ = create_folder.update(cx, |this, cx| {
                                    if !this.library_controls_busy(cx) {
                                        this.folders.update(cx, |panel, cx| {
                                            panel.begin_create_folder(window, cx)
                                        });
                                    }
                                });
                            }),
                    )
                    .separator()
                    .item(PopupMenuItem::new("导入文件…").disabled(busy).on_click(
                        move |_, _, cx| {
                            let _ = import_files.update(cx, |this, cx| {
                                if !this.library_controls_busy(cx) {
                                    this.settings_picker("import", None, cx);
                                }
                            });
                        },
                    ))
                    .item(PopupMenuItem::new("导入文件夹…").disabled(busy).on_click(
                        move |_, _, cx| {
                            let _ = import_folder.update(cx, |this, cx| {
                                if !this.library_controls_busy(cx) {
                                    this.settings_picker("import-folder", None, cx);
                                }
                            });
                        },
                    ))
            })
            .into_any_element()
    }

    fn library_sort_button(&self, cx: &Context<Self>) -> AnyElement {
        let owner = cx.weak_entity();
        MenuButton::new("library-sort-menu")
            .small()
            .ghost()
            .label("↕")
            .accessibility_label("排序方式")
            .tooltip("排序方式")
            .cursor_pointer()
            .disabled(self.library_controls_busy(cx))
            .dropdown_menu(move |mut menu, _, cx| {
                let Some((selected, _, busy)) = Self::library_menu_state(&owner, cx) else {
                    return menu;
                };
                menu = menu.min_w(px(208.));
                for (sort, label) in crate::note_actions::LIBRARY_SORT_OPTIONS {
                    let action_owner = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .checked(selected == sort)
                            .disabled(busy)
                            .on_click(move |_, _, cx| {
                                let _ = action_owner.update(cx, |this, cx| {
                                    if !this.library_controls_busy(cx) {
                                        this.layout.library_sort = sort;
                                        this.persist_layout(cx);
                                    }
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    fn library_more_button(&self, cx: &Context<Self>) -> AnyElement {
        let owner = cx.weak_entity();
        MenuButton::new("library-more-menu")
            .small()
            .ghost()
            .label("⋯")
            .accessibility_label("笔记库选项")
            .tooltip("笔记库选项")
            .cursor_pointer()
            .disabled(self.library_controls_busy(cx))
            .dropdown_menu(move |menu, _, cx| {
                let Some((_, detached, busy)) = Self::library_menu_state(&owner, cx) else {
                    return menu;
                };
                let action_owner = owner.clone();
                menu.min_w(px(208.)).item(
                    PopupMenuItem::new("查看已取消管理")
                        .checked(detached)
                        .disabled(busy)
                        .on_click(move |_, _, cx| {
                            let _ = action_owner.update(cx, |this, cx| {
                                if this.library_controls_busy(cx) {
                                    return;
                                }
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
                            });
                        }),
                )
            })
            .into_any_element()
    }

    fn reader_more_button(
        &self,
        note: &studyapp_core::Note,
        version: Option<String>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let owner = cx.weak_entity();
        let note_id = note.id.clone();
        let library = self.last_library.clone();
        MenuButton::new("note-more-menu")
            .small()
            .ghost()
            .label("⋯")
            .cursor_pointer()
            .accessibility_label("更多笔记操作")
            .tooltip("更多笔记操作")
            .disabled(self.library_controls_busy(cx))
            .dropdown_menu(move |mut menu, _, cx| {
                let Some((_, _, busy)) = Self::library_menu_state(&owner, cx) else {
                    return menu;
                };
                let current = owner
                    .update(cx, |this, cx| {
                        let snapshot = this.snapshot.as_ref().filter(|s| s.data_path == library)?;
                        let note = snapshot.notes.iter().find(|n| n.id == note_id)?.clone();
                        Some((note, this.sync_panel.read(cx).status.busy))
                    })
                    .ok()
                    .flatten();
                let Some((note, sync_busy)) = current else {
                    return menu
                        .item(PopupMenuItem::new("笔记已移除或资料库已切换").disabled(true));
                };
                menu = menu.min_w(px(210.));
                for (label, action, visible, disabled) in [
                    ("导出 Markdown…", ReaderAction::Export, true, busy),
                    ("移动到目录…", ReaderAction::Move, true, busy),
                    (
                        "取消管理…",
                        ReaderAction::Unmanage,
                        !note.detached,
                        busy || sync_busy,
                    ),
                    (
                        "重新关联文件…",
                        ReaderAction::Relink,
                        !note.managed && !note.detached,
                        busy,
                    ),
                ] {
                    if !visible {
                        continue;
                    }
                    let action_owner = owner.clone();
                    let action_id = note_id.clone();
                    let action_library = library.clone();
                    let action_version = version.clone();
                    menu = menu.item(PopupMenuItem::new(label).disabled(disabled).on_click(
                        move |_, _, cx| {
                            let _ = action_owner.update(cx, |this, cx| {
                                if this.library_controls_busy(cx)
                                    || this.snapshot.as_ref().is_none_or(|s| {
                                        s.data_path != action_library
                                            || !s.notes.iter().any(|n| n.id == action_id)
                                    })
                                {
                                    return;
                                }
                                match action {
                                    ReaderAction::Export => {
                                        this.export_note(action_id.clone(), action_version.clone())
                                    }
                                    ReaderAction::Move => this.folders.update(cx, |panel, cx| {
                                        panel.begin_move_note(action_id.clone(), cx)
                                    }),
                                    ReaderAction::Unmanage => {
                                        if this.sync_panel.read(cx).status.busy {
                                            return;
                                        }
                                        this.confirmation = Some(
                                            crate::editor_lifecycle::NoteConfirmation::Unmanage(
                                                action_id.clone(),
                                            ),
                                        );
                                        cx.notify();
                                    }
                                    ReaderAction::Relink => {
                                        this.picker("relink", Some(action_id.clone()))
                                    }
                                }
                            });
                        },
                    ));
                }
                menu
            })
            .into_any_element()
    }

    pub fn notes(&self, snapshot: &Snapshot, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let layout = self.effective_layout(window);
        let query = self.search.read(cx).value().to_lowercase();
        let index = self.folders.read(cx).library_index();
        let visible = self.folders.update(cx, |panel, _| {
            panel.visible_notes(
                &query,
                self.layout.library_sort,
                self.selected_note.as_deref(),
                self.show_detached,
            )
        });
        let notes = visible
            .iter()
            .filter_map(|&position| snapshot.notes.get(position))
            .collect::<Vec<_>>();
        let note_count = self.folders.read(cx).matching_note_count();
        let mut note_rows: std::collections::HashMap<Option<String>, Vec<AnyElement>> =
            std::collections::HashMap::new();
        for note in notes {
            let id = note.id.clone();
            let drag = crate::note_actions::NoteDrag::new(note, snapshot);
            let menu_note_id = id.clone();
            let menu_owner = cx.weak_entity();
            let metadata = index.note(&id);
            let mut tip = metadata
                .map(|m| format!("{}\n修改时间：{}", m.tip, m.updated_stamp))
                .unwrap_or_default();
            tip.push_str(if note.detached {
                "\n已取消管理 · 学习记录保留"
            } else if note.managed {
                "\n应用副本 · 由资料库管理"
            } else {
                "\n外部文件 · 跟踪原文件的变化"
            });
            if snapshot.settings.show_file_paths {
                tip.push_str(&format!("\n{}", display_path(&note.path)));
            }
            let chosen = self.active_draft_id.is_none() && self.selected_note.as_ref() == Some(&id);
            let element = library_entry()
                .id(SharedString::from(format!("note-{id}")))
                .test_support()
                .aria_selected(chosen)
                .px_2()
                .h(px(LIBRARY_ROW_HEIGHT))
                .rounded_md()
                .border_1()
                .border_color(rgba(0))
                .flex_shrink_0()
                .min_w_0()
                .gap_2()
                .cursor_pointer()
                .hover(|style| style.bg(crate::theme::color(BG)))
                .when(chosen, |c| c.bg(crate::theme::selection_color()))
                .child(
                    Icon::new(IconName::FileText)
                        .small()
                        .text_color(crate::theme::color(MUTED)),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .text_sm()
                        .child(note.title.clone()),
                )
                .when(
                    metadata.is_some_and(|m| {
                        m.status != studyapp_desktop::library_tree::StudyStatus::Recorded
                    }),
                    |r| {
                        let status = metadata.unwrap().status;
                        r.child(
                            div()
                                .id(SharedString::from(format!("note-status-{}", note.id)))
                                .test_support()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(crate::theme::color(
                                    if status == studyapp_desktop::library_tree::StudyStatus::Added
                                    {
                                        GREEN
                                    } else {
                                        0x9B5719
                                    },
                                ))
                                .child(
                                    if status == studyapp_desktop::library_tree::StudyStatus::Added
                                    {
                                        "A"
                                    } else {
                                        "M"
                                    },
                                ),
                        )
                    },
                )
                .when(!note.managed || note.detached || note.missing, |r| {
                    r.child(
                        library_tag(
                            if note.missing {
                                "待定位"
                            } else if note.detached {
                                "已移除"
                            } else {
                                "外部"
                            },
                            note.missing,
                        )
                        .id(SharedString::from(format!("note-source-{}", note.id)))
                        .test_support(),
                    )
                })
                // Anchor within the sidebar so native editor children cannot
                // cover learning/review timestamps near the row's right edge.
                .tooltip(move |window, cx| sidebar_tooltip(tip.clone(), window, cx))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_note_document(id.clone(), None, false, window, cx);
                }))
                .on_drag(drag, |drag, source_offset, _, cx| {
                    cx.new(|_| crate::note_actions::NoteDragPreview::new(drag, source_offset))
                })
                .context_menu(move |menu, window, cx| {
                    let opened = menu_owner.update(cx, |this, cx| {
                        let snapshot = this.snapshot.clone()?;
                        let note = snapshot
                            .notes
                            .iter()
                            .find(|n| n.id == menu_note_id)?
                            .clone();
                        this.note_menu_sequence = this.note_menu_sequence.wrapping_add(1);
                        let token = this.note_menu_sequence;
                        this.active_note_menu = Some(token);
                        cx.notify();
                        Some((
                            snapshot,
                            note,
                            token,
                            this.storage_busy || this.media_busy(),
                        ))
                    });
                    let Ok(Some((snapshot, note, token, busy))) = opened else {
                        return menu;
                    };
                    Self::track_menu_surface(menu_owner.clone(), token, cx);
                    let dismissed = menu_owner.clone();
                    cx.subscribe_self(move |_, _: &DismissEvent, cx| {
                        let _ = dismissed.update(cx, |this, cx| {
                            if this.active_note_menu == Some(token) {
                                this.active_note_menu = None;
                                this.clear_menu_surface(cx);
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
                                this.clear_menu_surface(cx);
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
                            let _ = action_owner
                                .update(cx, |this, cx| this.handle_note_action(action, cx));
                        },
                    )
                });
            note_rows
                .entry(note.folder_id.clone())
                .or_default()
                .push(element.into_any_element());
        }
        let draft_count = self.draft_library.active_count();
        let draft_entries = self
            .draft_library
            .ids()
            .iter()
            .filter_map(|id| self.draft_catalog.get(id))
            .map(|record| {
                let id = record.id.clone();
                let title = if record.draft.title.trim().is_empty() {
                    "未命名草稿".to_owned()
                } else {
                    record.draft.title.clone()
                };
                let active = self.active_draft_id.as_ref() == Some(&id);
                let error = self.draft_errors.contains_key(&id);
                library_entry()
                    .id(SharedString::from(format!("draft-row-{id}")))
                    .test_support()
                    .aria_selected(active)
                    .min_w_0()
                    .gap_2()
                    .px_2()
                    .cursor_pointer()
                    .hover(|r| r.bg(crate::theme::color(BG)))
                    .when(active, |r| r.bg(crate::theme::selection_color()))
                    .child(
                        Icon::new(IconName::FileText)
                            .small()
                            .text_color(crate::theme::color(MUTED)),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(title))
                    .child(
                        muted(if error {
                            "保存失败".into()
                        } else if active {
                            "编辑中".into()
                        } else {
                            stamp(record.updated_at / 1000)
                        })
                        .text_xs(),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_draft(id.clone(), window, cx)
                    }))
                    .into_any_element()
            })
            .collect();
        let tree = self.folders.update(cx, |panel, cx| {
            panel.set_draft_entries(draft_count, draft_entries);
            panel.library_tree(
                note_rows,
                &query,
                self.layout.library_sort,
                self.selected_note.as_deref(),
                cx,
            )
        });
        let list = column()
            .id("notes-pane")
            .test_support()
            .w(layout.notes)
            .h_full()
            .flex_shrink_0()
            .min_h_0()
            .min_w_0()
            .gap_2()
            .bg(crate::theme::color(0xFFFFFF))
            .overflow_hidden()
            .child(
                row()
                    .px_2()
                    .gap_1()
                    .h(px(46.))
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(crate::theme::color(BORDER))
                    .child(
                        Button::new("toggle-folders")
                            .small()
                            .ghost()
                            .icon(IconName::PanelLeft)
                            .accessibility_label("展开或收起笔记库")
                            .tooltip(if self.layout.folders_visible {
                                "收起笔记库"
                            } else {
                                "展开笔记库"
                            })
                            .disabled(
                                self.storage_busy
                                    || self.preferences_ui.busy
                                    || self.preferences_ui.replacing,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_folders(cx))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(if self.show_detached {
                                "已取消管理"
                            } else {
                                "笔记库"
                            }),
                    )
                    .child(muted(note_count.to_string()).text_xs())
                    .child(self.library_add_button(cx))
                    .child(self.library_sort_button(cx))
                    .child(self.library_more_button(cx)),
            )
            .child(div().px_2().child(Input::new(&self.search).cleanable(true)))
            .child(tree);
        let selected = self
            .selected_note
            .as_deref()
            .and_then(|id| self.read_models.note_position(id))
            .and_then(|position| snapshot.notes.get(position));
        let detail = if self.note_editor.is_some() {
            self.note_editor_view(cx)
        } else if let Some(note) = selected {
            let id = note.id.clone();
            let managed = note.managed && !note.detached;
            let export_version = self.selected_version.clone();
            let versions: Vec<_> = self
                .read_models
                .version_positions(&id)
                .iter()
                .map(|&position| &snapshot.versions[position])
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
                        .child(heading(note.title.clone()).flex_1().min_w_0().truncate())
                        .child(Button::new("record-note").small().label("记录").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.open_editor(EditorMode::Manual, window, cx)
                            }),
                        ))
                        .child(
                            Button::new("edit-note")
                                .small()
                                .ghost()
                                .label(if historical {
                                    "复制版本"
                                } else if managed {
                                    "编辑"
                                } else {
                                    "创建托管副本"
                                })
                                .disabled(self.storage_busy)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_note_editor(copy, window, cx)
                                })),
                        )
                        .child(self.reader_more_button(note, export_version, cx)),
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
                    muted(
                        index
                            .note(&note.id)
                            .map(|m| m.tip.clone())
                            .unwrap_or_default()
                            .replace('\n', " · "),
                    )
                    .truncate()
                    .text_xs()
                    .flex_shrink_0(),
                );
            if self.history {
                use gpui_kit::component::scroll::ScrollableElement;
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
                        .relative()
                        .track_scroll(&self.history_versions_scroll)
                        .horizontal_scrollbar(&self.history_versions_scroll)
                        .h(px(42.))
                        .w_full()
                        .overflow_x_scroll()
                } else {
                    column()
                        .id("history-versions-scroll")
                        .relative()
                        .track_scroll(&self.history_versions_scroll)
                        .vertical_scrollbar(&self.history_versions_scroll)
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
                .child(muted("当前草稿尚未可靠保存；当前正文仍保留。其他草稿不会被覆盖，修复目录权限后可重试。"))
                .child(Button::new("retry-note-draft").label("重试草稿读取").on_click(cx.listener(|this, _, _, cx| {
                    if this.note_editor.is_none() { this.draft_failed = false; this.draft_loading = true; this.drafts.load(&this.last_library); }
                    cx.notify();
                })))))
            .when(self.recoverable_draft.is_some(), |c| c.child(row().id("draft-recovery")
                .h(px(40.)).flex_shrink_0().min_w_0().px_2().mb_2().rounded_md()
                .bg(crate::theme::color(0xFFFFFF))
                .child(Icon::new(IconName::FileText).small().text_color(crate::theme::color(MUTED)))
                .child(div().flex_1().min_w_0().truncate().child(muted("未保存的笔记已保留在草稿箱")))
                    .child(Button::new("recover-note-draft").disabled(self.storage_busy || self.note_saving || self.media_busy()).small().ghost().label("恢复最近草稿").on_click(cx.listener(|this, _, window, cx| this.recover_note(window, cx))))
                    .child(Button::new("discard-recovered-draft").disabled(self.storage_busy || self.note_saving || self.media_busy()).small().ghost().label("删除草稿").on_click(cx.listener(|this, _, _, cx| {
                        this.confirmation = this.recoverable_draft_id().map(crate::editor_lifecycle::NoteConfirmation::DiscardDraft);
                        cx.notify();
                    })))
                ))
            .child(row().items_stretch().gap_0().flex_1().min_h_0().min_w_0().w_full().overflow_hidden()
                .when(self.layout.folders_visible, |view| view.child(list).child(self.resize_handle(Pane::Notes, cx)))
                .child(div().id("document-pane").test_support().flex().flex_1().h_full().min_h_0().min_w_0().overflow_hidden().child(detail)))
    }
}
