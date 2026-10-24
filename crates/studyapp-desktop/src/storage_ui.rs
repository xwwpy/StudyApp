use crate::controls::Button;
use crate::storage_backend::viewmodel::{ManagedCopyView, StoragePresentation, StorageRow};
use crate::{app::StudyApp, components::*, editor_lifecycle::NoteIntent, worker::Work};
use gpui_kit::{
    component::{Disableable, Sizable, button::ButtonVariants, scroll::ScrollableElement},
    prelude::FluentBuilder,
    *,
};
use std::{cell::RefCell, sync::Arc};

#[derive(Clone, Copy, PartialEq, Eq)]
struct StorageLayout {
    show_paths: bool,
    draft_revision: u64,
    font_size: u16,
    spacing: u16,
}

pub struct StorageUi {
    pub scroll: ListState,
    source: RefCell<Option<(Arc<StoragePresentation>, StorageLayout)>>,
    #[cfg(feature = "ui-test")]
    pub eager_baseline: bool,
    #[cfg(feature = "ui-test")]
    pub frame_rows: std::cell::Cell<u32>,
    #[cfg(feature = "ui-test")]
    pub total_builds: std::cell::Cell<u64>,
    #[cfg(feature = "ui-test")]
    eager_scroll: ScrollHandle,
}

impl Default for StorageUi {
    fn default() -> Self {
        Self {
            scroll: ListState::new(0, ListAlignment::Top, px(48.)),
            source: RefCell::new(None),
            #[cfg(feature = "ui-test")]
            eager_baseline: false,
            #[cfg(feature = "ui-test")]
            frame_rows: std::cell::Cell::new(0),
            #[cfg(feature = "ui-test")]
            total_builds: std::cell::Cell::new(0),
            #[cfg(feature = "ui-test")]
            eager_scroll: ScrollHandle::new(),
        }
    }
}

impl StorageUi {
    fn sync(&self, presentation: &Arc<StoragePresentation>, layout: StorageLayout) {
        let mut source = self.source.borrow_mut();
        if let Some((previous, previous_layout)) = source.as_ref() {
            if Arc::ptr_eq(previous, presentation) && previous_layout == &layout {
                return;
            }
            let top = self.scroll.logical_scroll_top();
            let row = previous
                .rows(previous_layout.show_paths)
                .get(top.item_ix)
                .copied();
            if Arc::ptr_eq(previous, presentation)
                && previous_layout.show_paths == layout.show_paths
            {
                // A draft lock or typography change can change row height.
                self.scroll.remeasure();
            } else {
                self.scroll.reset_with_uniform_height(
                    presentation.rows(layout.show_paths).len(),
                    px(150.),
                );
                if let Some(row) = row {
                    let exact = presentation.relocate_row(previous, row, layout.show_paths);
                    if let Some(next) = exact.or_else(|| {
                        presentation.relocate_or_nearest(previous, row, layout.show_paths)
                    }) {
                        self.scroll.scroll_to(ListOffset {
                            item_ix: next,
                            offset_in_item: if exact.is_some() {
                                top.offset_in_item
                            } else {
                                px(0.)
                            },
                        });
                    }
                }
            }
        } else {
            self.scroll
                .reset_with_uniform_height(presentation.rows(layout.show_paths).len(), px(150.));
        }
        *source = Some((presentation.clone(), layout));
    }
}

fn storage_copy_list(
    scroll: &ListState,
    render: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
) -> impl IntoElement + Styled {
    div()
        .id("storage-copy-list")
        .test_support()
        .relative()
        .flex_shrink_0()
        .min_w_0()
        .h(px(340.))
        .overflow_hidden()
        .vertical_scrollbar(scroll)
        .child(list(scroll.clone(), render).size_full())
        // The list handles scrolling first. Contain the wheel at both ends too.
        // Consume every wheel event here, even after the offset reaches an end.
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

impl StudyApp {
    pub fn storage_work(&mut self, work: Work, cx: &mut Context<Self>) {
        let migrating = matches!(&work, Work::MigrateLibrary(_));
        if self.storage_busy
            || self.media_jobs_active()
            || self.note_saving
            || self.draft_discard.is_some()
            || self.draft_persist.is_some()
            || self.preferences_ui.busy
            || self.preferences_ui.replacing
            || (migrating && self.sync_panel.read(cx).status.busy)
        {
            return;
        }
        self.storage_busy = true;
        self.folders
            .update(cx, |panel, cx| panel.set_busy(true, cx));
        if migrating {
            self.preferences_ui.replacing = true;
            self.sync_panel
                .update(cx, |panel, cx| panel.set_editing_locked(true, cx));
        }
        if self
            .worker
            .sender
            .send(Work::Scoped {
                library: self.last_library.clone(),
                work: Box::new(work),
            })
            .is_err()
        {
            self.storage_busy = false;
            self.folders
                .update(cx, |panel, cx| panel.set_busy(false, cx));
            if migrating {
                self.preferences_ui.replacing = false;
                self.sync_panel
                    .update(cx, |panel, cx| panel.set_editing_locked(false, cx));
            }
            self.push_notice(Err("后台服务已停止，请重新启动应用".into()), cx);
        }
        cx.notify();
    }

    pub fn choose_storage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.media_jobs_active() {
            self.push_notice(Err("正在处理图片，请稍候再迁移资料库。".into()), cx);
            return;
        }
        if self.guard_settings(crate::settings_ui::SettingsContinuation::Migrate, cx) {
            return;
        }
        if self.storage_busy
            || self.note_saving
            || self.draft_loading
            || self.draft_discard.is_some()
            || self.sync_panel.read(cx).status.busy
        {
            return;
        }
        if self.draft_failed {
            self.push_notice(Err("请先重试草稿保存，再迁移资料库。".into()), cx);
            cx.notify();
        } else if self.note_editor.is_some() {
            self.request_note_action(NoteIntent::PrepareMigration, window, cx);
        } else {
            self.begin_draft_drain(NoteIntent::PrepareMigration, cx);
        }
    }

    pub fn pick_migration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.storage_busy
            || self.preferences_ui.busy
            || self.preferences_ui.replacing
            || self.sync_panel.read(cx).status.busy
        {
            return;
        }
        let library = self.last_library.clone();
        self.storage_busy = true;
        self.storage_confirm = None;
        self.folders
            .update(cx, |panel, cx| panel.set_busy(true, cx));
        self.sync_panel
            .update(cx, |panel, cx| panel.set_editing_locked(true, cx));
        cx.spawn_in(window, async move |this, cx| {
            let initial_directory = std::path::PathBuf::from(&library);
            let path = cx
                .background_executor()
                .spawn(async move {
                    crate::folder_picker::pick_folder(
                        "选择新的空文件夹（迁移后保留原资料）",
                        Some(initial_directory),
                    )
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                // A reply from another library must not release the new
                // library's operation locks or migrate its data.
                if library != this.last_library {
                    return;
                }
                this.storage_busy = false;
                this.folders
                    .update(cx, |panel, cx| panel.set_busy(false, cx));
                this.sync_panel
                    .update(cx, |panel, cx| panel.set_editing_locked(false, cx));
                if let Some(path) = path {
                    this.accept_picker_result(library, Work::MigrateLibrary(path), cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn storage_view(&self, cx: &mut Context<Self>) -> Div {
        let show_file_paths = self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.settings.show_file_paths);
        card().child(
            row().child(heading("空间管理")).child(div().flex_1()).child(
                Button::new("refresh-storage").small().disabled(self.storage_busy)
                    .label(if self.storage_busy { "正在处理…" } else { "统计占用空间" })
                    .on_click(cx.listener(|this, _, _, cx| this.storage_work(Work::RefreshStorage, cx))),
            ),
        )
        .child(muted("只可逐个删除当前资料库的托管副本。跟踪的外部文件不可删除；学习记录与历史版本始终保留。"))
        .when_some(self.storage_report.as_ref().filter(|r| r.data_path == self.last_library), |view, report| {
            let presentation = report.presentation.clone();
            let theme = crate::theme::palette();
            self.storage_ui.sync(&presentation, StorageLayout {
                show_paths: show_file_paths,
                draft_revision: self.storage_draft_revision,
                font_size: theme.font_size,
                spacing: theme.spacing,
            });
            #[cfg(feature = "ui-test")]
            self.storage_ui.frame_rows.set(0);
            let view = view.child(heading(presentation.logical_label.clone()))
                .child(muted(presentation.scanned_label.clone()))
                .child(muted("按文件路径的逻辑长度统计。硬链接可能共享磁盘空间；此数值不代表物理占用或清理后实际释放量，云端空间未计入。"))
                ;
            #[cfg(feature = "ui-test")]
            if self.storage_ui.eager_baseline {
                return view.child(scroll_body("storage-copy-list", &self.storage_ui.eager_scroll)
                    .test_support().flex_shrink_0().h(px(340.)).gap_0()
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .children(presentation.rows(show_file_paths).iter().copied()
                        .map(|row| self.storage_row(&presentation, row, show_file_paths, cx))));
            }
            let owner = cx.entity();
            view.child(storage_copy_list(&self.storage_ui.scroll, move |ix, _, cx| {
                let row = presentation.rows(show_file_paths)[ix];
                owner.update(cx, |owner, cx| owner.storage_row(&presentation, row, show_file_paths, cx)).into_any_element()
            }))
        })
    }

    fn storage_row(
        &self,
        presentation: &StoragePresentation,
        item: StorageRow,
        show_paths: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        #[cfg(feature = "ui-test")]
        {
            self.storage_ui
                .frame_rows
                .set(self.storage_ui.frame_rows.get() + 1);
            self.storage_ui
                .total_builds
                .set(self.storage_ui.total_builds.get() + 1);
        }
        let content = match item {
            StorageRow::Category(ix) => {
                let category = &presentation.categories[ix];
                column()
                    .gap_1()
                    .py_2()
                    .child(
                        row()
                            .child(div().flex_1().child(category.title))
                            .child(muted(category.bytes.clone())),
                    )
                    .child(muted(category.description).text_xs())
            }
            StorageRow::ImageSummary => column().child(muted(presentation.image_summary.clone())),
            StorageRow::ImageLocation(ix) => {
                column().child(muted(presentation.image_locations[ix].clone()).text_xs())
            }
            StorageRow::IssuesHeading => column().child(muted(
                "部分位置未能统计，下方是原因；已统计的数据仍然有效。",
            )),
            StorageRow::Issue(ix) => {
                let issue = &presentation.issues[ix];
                column()
                    .gap_1()
                    .child(muted(issue.message.clone()))
                    .when(show_paths, |view| {
                        view.child(muted(issue.path.clone()).text_xs())
                    })
            }
            StorageRow::CopiesHeading => column().child(heading("托管副本")),
            StorageRow::NoCopies => column().child(muted("没有可清理的托管副本。")),
            StorageRow::Copy(ix) => self.storage_copy_row(&presentation.copies[ix], show_paths, cx),
        };
        div().pr_4().pb_2().child(content)
    }

    fn storage_copy_row(
        &self,
        copy: &ManagedCopyView,
        show_paths: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let blocked =
            self.storage_busy || self.preferences_ui.busy || self.sync_panel.read(cx).status.busy;
        let id = copy.note_id.clone();
        let history_id = id.clone();
        let reveal_path = copy.path.clone();
        let path_id = SharedString::from(format!("storage-copy-path-{id}"));
        // Mutable draft protection belongs to the current UI read model, never the scan.
        let in_draft = self.storage_active_draft_notes.contains(&id);
        column()
            .flex_shrink_0()
            .gap_2()
            .p_3()
            .rounded_lg()
            .bg(crate::theme::color(BG))
            .child(
                row()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(copy.title.clone()),
                    )
                    .child(muted(copy.bytes.clone()).flex_shrink_0()),
            )
            .child(muted(copy.state.label()))
            .child(
                row()
                    .flex_wrap()
                    .child(
                        Button::new(SharedString::from(format!("copy-reveal-{id}")))
                            .small()
                            .ghost()
                            .label("定位文件")
                            .disabled(!copy.deletable)
                            .on_click(move |_, _, cx| {
                                cx.reveal_path(std::path::Path::new(&reveal_path))
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("copy-history-{id}")))
                            .small()
                            .ghost()
                            .label("历史与恢复")
                            .disabled(blocked)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if this.guard_settings(
                                    crate::settings_ui::SettingsContinuation::ViewNote(
                                        history_id.clone(),
                                    ),
                                    cx,
                                ) {
                                    return;
                                }
                                let version = this.snapshot.as_ref().and_then(|s| {
                                    s.versions
                                        .iter()
                                        .rev()
                                        .find(|v| v.note_id == history_id)
                                        .map(|v| v.id.clone())
                                });
                                this.select_note_document(
                                    history_id.clone(),
                                    version,
                                    true,
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("delete-copy-{id}")))
                            .small()
                            .ghost()
                            .label("清理这一份副本")
                            .disabled(blocked || !copy.deletable || in_draft)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.guard_settings(
                                    crate::settings_ui::SettingsContinuation::ConfirmDeleteCopy(
                                        id.clone(),
                                    ),
                                    cx,
                                ) {
                                    return;
                                }
                                this.storage_confirm = Some(id.clone());
                                cx.notify();
                            })),
                    ),
            )
            .when(show_paths, |v| {
                v.child(
                    muted(copy.path.clone())
                        .id(path_id)
                        .test_support()
                        .text_xs()
                        .truncate(),
                )
            })
            .when(in_draft, |v| {
                v.child(muted("请先保存或删除该笔记的草稿，再清理副本。"))
            })
            .when_some(copy.reason.as_ref(), |v, reason| {
                v.child(muted(reason.clone()))
            })
    }

    #[cfg(feature = "ui-test")]
    pub fn storage_render_work(&self) -> (u64, u32) {
        (
            self.storage_ui.total_builds.get(),
            self.storage_ui.frame_rows.get(),
        )
    }

    fn copy_confirmation_allowed(&self, note_id: &str, cx: &App) -> bool {
        !self.storage_busy
            && !self.note_saving
            && !self.sync_panel.read(cx).status.busy
            && self.draft_discard.is_none()
            && self.draft_persist.is_none()
            && !self.draft_catalog.values().any(|r| {
                r.active()
                    && (r.draft.note_id.as_deref() == Some(note_id)
                        || r.save_target.as_deref() == Some(note_id))
            })
            && self
                .storage_report
                .as_ref()
                .filter(|report| report.data_path == self.last_library)
                .is_some_and(|report| {
                    report
                        .presentation
                        .copy_by_id(note_id)
                        .is_some_and(|copy| copy.deletable)
                })
    }

    pub fn confirm_storage_copy(&mut self, cx: &mut Context<Self>) {
        let Some(note_id) = self.storage_confirm.clone() else {
            return;
        };
        if !self.copy_confirmation_allowed(&note_id, cx) {
            self.push_notice(
                Err("这份副本当前无法删除，请等待任务结束后重新统计空间。".into()),
                cx,
            );
            cx.notify();
            return;
        }
        self.storage_confirm = None;
        self.storage_work(Work::DeleteManagedCopy { note_id }, cx);
    }

    /// A confirmation for exactly one managed copy, mounted in the root dialog.
    pub fn storage_confirmation_view(&self, cx: &mut Context<Self>) -> Option<Div> {
        let note_id = self.storage_confirm.as_ref()?;
        let copy = self
            .storage_report
            .as_ref()
            .filter(|report| report.data_path == self.last_library)
            .and_then(|report| report.presentation.copy_by_id(note_id));
        let allowed = self.copy_confirmation_allowed(note_id, cx);
        let mut content = column().child(heading("删除这一份托管副本？")).child(muted(
            "将删除所选文件并停止管理。历史快照和学习记录保留，可通过“历史与恢复”导出或复制。",
        ));
        if let Some(copy) = copy {
            content = content.child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(copy.title.clone()),
            );
            if self
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.settings.show_file_paths)
            {
                content = content.child(muted(copy.path.clone()));
            }
        }
        if !allowed {
            content = content.child(muted(
                "目标或资料库状态已改变，当前无法删除。请取消后重新统计空间。",
            ));
        }
        Some(
            content.child(
                row()
                    .child(
                        Button::new("confirm-delete-copy")
                            .small()
                            .primary()
                            .disabled(!allowed)
                            .label("确认删除这一份")
                            .on_click(cx.listener(|this, _, _, cx| this.confirm_storage_copy(cx))),
                    )
                    .child(
                        Button::new("cancel-delete-copy")
                            .small()
                            .ghost()
                            .label("取消")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.storage_confirm = None;
                                cx.notify();
                            })),
                    ),
            ),
        )
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::storage_copy_list;
    use gpui_kit::{
        Context, InteractiveElement, IntoElement, ListAlignment, ListState, ParentElement, Render,
        ScrollDelta, ScrollHandle, ScrollWheelEvent, StatefulInteractiveElement, Styled,
        TestAppContext, VisualTestContext, Window, div, point, px,
    };
    use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

    #[test]
    fn storage_path_toggle_keeps_section_anchor_without_a_window() {
        use super::{StorageLayout, StorageUi};
        use crate::storage_backend::{StorageReport, viewmodel::StorageRow};
        let mut report = StorageReport {
            image_locations: vec!["C:/images-a".into(), "D:/images-b".into()],
            ..Default::default()
        };
        report.prepare_presentation();
        let presentation = &report.presentation;
        let state = StorageUi::default();
        let layout = StorageLayout {
            show_paths: true,
            draft_revision: 0,
            font_size: 14,
            spacing: 8,
        };
        state.sync(presentation, layout);
        let path_index = presentation
            .rows(true)
            .iter()
            .position(|row| *row == StorageRow::ImageLocation(1))
            .unwrap();
        state.scroll.scroll_to(gpui_kit::ListOffset {
            item_ix: path_index,
            offset_in_item: px(20.),
        });
        state.sync(
            presentation,
            StorageLayout {
                show_paths: false,
                ..layout
            },
        );
        let top = state.scroll.logical_scroll_top();
        assert_eq!(
            presentation.rows(false)[top.item_ix],
            StorageRow::ImageSummary
        );
        assert_eq!(
            top.offset_in_item,
            px(0.),
            "A different fallback row starts at its top"
        );
    }

    struct NestedStorage {
        outer: ScrollHandle,
        inner: ListState,
        rendered: Rc<RefCell<BTreeSet<usize>>>,
    }

    impl Render for NestedStorage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let rendered = self.rendered.clone();
            div()
                .id("page-scroll")
                .w(px(400.))
                .h(px(500.))
                .overflow_y_scroll()
                .track_scroll(&self.outer)
                .child(
                    storage_copy_list(&self.inner, move |ix, _, _| {
                        rendered.borrow_mut().insert(ix);
                        div().h(px(50.)).flex_shrink_0().into_any_element()
                    })
                    .w(px(350.)),
                )
                .child(div().h(px(1000.)).flex_shrink_0())
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            _ = window.draw(cx);
        });
    }

    fn wheel(cx: &mut VisualTestContext, x: f32, y: f32, delta: f32) {
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(x), px(y)),
            delta: ScrollDelta::Pixels(point(px(0.), px(delta))),
            ..Default::default()
        });
        draw(cx);
    }

    #[gpui_kit::test]
    fn storage_wheel_is_contained_while_scrolling_and_at_both_boundaries(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let inner =
            ListState::new(20, ListAlignment::Top, px(48.)).with_uniform_item_height(px(50.));
        let outer = ScrollHandle::new();
        let (_, cx) = cx.add_window_view(|_, _| NestedStorage {
            inner: inner.clone(),
            outer: outer.clone(),
            rendered: Rc::default(),
        });
        draw(cx);
        assert!(inner.max_offset_for_scrollbar().y > px(0.));
        assert!(outer.max_offset().y > px(0.));

        wheel(cx, 100., 100., -80.);
        assert_eq!(inner.scroll_px_offset_for_scrollbar().y, px(-80.));
        assert_eq!(outer.offset().y, px(0.), "inner wheel moved Settings");

        wheel(cx, 100., 100., -10000.);
        assert_eq!(
            inner.scroll_px_offset_for_scrollbar().y,
            -inner.max_offset_for_scrollbar().y
        );
        wheel(cx, 100., 100., -80.);
        assert_eq!(outer.offset().y, px(0.), "bottom boundary moved Settings");

        wheel(cx, 100., 100., 10000.);
        assert_eq!(inner.scroll_px_offset_for_scrollbar().y, px(0.));
        // Put Settings in the middle while keeping the top of the list visible.
        outer.set_offset(point(px(0.), px(-40.)));
        draw(cx);
        wheel(cx, 100., 100., 80.);
        assert_eq!(inner.scroll_px_offset_for_scrollbar().y, px(0.));
        assert_eq!(outer.offset().y, px(-40.), "top boundary moved Settings");

        // The narrow right-hand strip is outside the nested list.
        wheel(cx, 380., 100., -80.);
        assert_eq!(inner.scroll_px_offset_for_scrollbar().y, px(0.));
        assert_eq!(
            outer.offset().y,
            px(-120.),
            "Settings did not scroll outside list"
        );
    }

    #[gpui_kit::test]
    fn storage_list_builds_only_visible_rows_at_hundred_and_thousand_copy_scale(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        for count in [100, 1000] {
            let rendered = Rc::new(RefCell::new(BTreeSet::new()));
            let inner = ListState::new(count, ListAlignment::Top, px(48.))
                .with_uniform_item_height(px(50.));
            let (_, cx) = cx.add_window_view(|_, _| NestedStorage {
                inner: inner.clone(),
                outer: ScrollHandle::new(),
                rendered: rendered.clone(),
            });
            draw(cx);
            assert!(
                rendered.borrow().len() < 12,
                "constructed offscreen rows at {count} copies"
            );
            assert!(!rendered.borrow().contains(&(count - 1)));
            rendered.borrow_mut().clear();
            inner.scroll_to_end();
            draw(cx);
            assert!(
                rendered.borrow().contains(&(count - 1)),
                "final copy must be reachable"
            );
            assert!(rendered.borrow().len() < 12);
            assert_eq!(
                inner.scroll_px_offset_for_scrollbar().y,
                -inner.max_offset_for_scrollbar().y
            );
        }
    }
}
