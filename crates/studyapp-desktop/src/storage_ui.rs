use crate::controls::Button;
use crate::{app::StudyApp, components::*, editor_lifecycle::NoteIntent, worker::Work};
use gpui_kit::{
    component::{Disableable, Sizable, button::ButtonVariants},
    prelude::FluentBuilder,
    *,
};
use studyapp_desktop::presentation::display_path;

fn size_label(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024. * 1024.))
    }
}

fn storage_copy_list(scroll: &ScrollHandle) -> Stateful<Div> {
    scroll_body("storage-copy-list", scroll)
        .flex_shrink_0()
        .max_h(px(340.))
        .pr_4()
        // GPUI dispatches the built-in scrolling before this bubble callback.
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
        let blocked =
            self.storage_busy || self.preferences_ui.busy || self.sync_panel.read(cx).status.busy;
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
            let timestamp = chrono::DateTime::from_timestamp(report.scanned_at, 0)
                .map(|time| time.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "尚未统计".into());
            view.child(heading(format!("已统计逻辑大小 {}{}", size_label(report.logical_bytes()), if report.incomplete { "（统计不完整）" } else { "" })))
                .child(muted(format!("统计时间：{timestamp} · 包含已登记的外置图片")))
                .child(muted("按文件路径的逻辑长度统计。硬链接可能共享磁盘空间；此数值不代表物理占用或清理后实际释放量，云端空间未计入。"))
                .child(storage_copy_list(&self.storage_copy_scroll).test_support()
                    .child(column().flex_shrink_0().gap_2().children([
                        ("数据库与历史", report.database_bytes, "笔记历史、学习记录与数据库日志"),
                        ("托管笔记", report.managed_bytes, "当前资料库自有的 Markdown 副本"),
                        ("本地图片", report.image_bytes, "库内图片及索引登记的当前、历史位置"),
                        ("草稿", report.draft_bytes, "未提交的笔记草稿，支持多份草稿"),
                        ("保留与恢复文件", report.retained_bytes, "发布暂存、原始副本及未归属文件"),
                        ("备份", report.backup_bytes, "资料库目录内的数据库备份"),
                        ("配置与其他", report.auxiliary_bytes, "设置版本、图片索引与其他文件"),
                    ].into_iter().map(|(title, bytes, description)| column().flex_shrink_0().gap_1().py_2()
                        .child(row().child(div().flex_1().child(title)).child(muted(size_label(bytes))))
                        .child(muted(description).text_xs()))))
                    .when(!report.image_locations.is_empty(), |view| view.child(column().flex_shrink_0()
                        .child(muted(format!("已登记图片位置：{} 处；只统计已知资产，不扫描外置目录中的其他文件。", report.image_locations.len())))
                        .when(show_file_paths, |view| view.children(report.image_locations.iter().map(|location| muted(display_path(location)).text_xs())))))
                    .when(report.incomplete, |view| view.child(column().flex_shrink_0()
                        .child(muted("部分位置未能统计，下方是原因；已统计的数据仍然有效。"))
                        .children(report.issues.iter().map(|issue| column().flex_shrink_0().gap_1()
                            .child(muted(issue.message.clone()))
                            .when(show_file_paths, |view| view.child(muted(display_path(&issue.path)).text_xs()))))))
                    .child(heading("托管副本").flex_shrink_0())
                    .when(report.copies.is_empty(), |v| v.child(muted("没有可清理的托管副本。")))
                    .children(report.copies.iter().map(|copy| {
                        let id = copy.note_id.clone();
                        let history_id = id.clone();
                        let reveal_path = display_path(&copy.path);
                        let path_id = SharedString::from(format!("storage-copy-path-{id}"));
                        let in_draft = self.draft_catalog.values().any(|r|r.active() && r.draft.note_id.as_ref()==Some(&id));
                        column().flex_shrink_0().gap_2().p_3().rounded_lg().bg(crate::theme::color(BG))
                            .child(row().child(div().flex_1().min_w_0().truncate().child(copy.title.clone()))
                                .child(muted(size_label(copy.bytes)).flex_shrink_0()))
                            .child(muted(copy.state.label()))
                            .child(row().flex_wrap()
                                .child(Button::new(SharedString::from(format!("copy-reveal-{id}")))
                                    .small().ghost().label("定位文件").disabled(!copy.deletable)
                                    .on_click(move |_, _, cx| cx.reveal_path(std::path::Path::new(&reveal_path))))
                                .child(Button::new(SharedString::from(format!("copy-history-{id}")))
                                    .small().ghost().label("历史与恢复").disabled(blocked)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if this.guard_settings(crate::settings_ui::SettingsContinuation::ViewNote(history_id.clone()), cx) {
                                            return;
                                        }
                                        let version=this.snapshot.as_ref().and_then(|s|s.versions.iter().rev().find(|v|v.note_id==history_id).map(|v|v.id.clone()));
                                        this.select_note_document(history_id.clone(),version,true,window,cx);
                                    })))
                                .child(Button::new(SharedString::from(format!("delete-copy-{id}")))
                                    .small().ghost().label("清理这一份副本")
                                    .disabled(blocked || !copy.deletable || in_draft)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if this.guard_settings(crate::settings_ui::SettingsContinuation::ConfirmDeleteCopy(id.clone()), cx) {
                                            return;
                                        }
                                        this.storage_confirm = Some(id.clone()); cx.notify();
                                    }))))
                            .when(show_file_paths, |v| v.child(muted(display_path(&copy.path))
                                .id(path_id).test_support().text_xs().truncate()))
                            .when(in_draft, |v| v.child(muted("请先保存或删除该笔记的草稿，再清理副本。")))
                            .when_some(copy.reason.as_ref(), |v, reason| v.child(muted(reason.clone())))
                    })))
        })
    }

    fn copy_confirmation_allowed(&self, note_id: &str, cx: &App) -> bool {
        !self.storage_busy
            && !self.note_saving
            && !self.sync_panel.read(cx).status.busy
            && self.draft_discard.is_none()
            && self.draft_persist.is_none()
            && !self
                .draft_catalog
                .values()
                .any(|r| r.active() && r.draft.note_id.as_deref() == Some(note_id))
            && self
                .storage_report
                .as_ref()
                .filter(|report| report.data_path == self.last_library)
                .is_some_and(|report| {
                    report
                        .copies
                        .iter()
                        .any(|copy| copy.note_id == note_id && copy.deletable)
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
            .and_then(|report| report.copies.iter().find(|copy| &copy.note_id == note_id));
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
                content = content.child(muted(display_path(&copy.path)));
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
        Context, InteractiveElement, IntoElement, ParentElement, Render, ScrollDelta, ScrollHandle,
        ScrollWheelEvent, StatefulInteractiveElement, Styled, TestAppContext, VisualTestContext,
        Window, div, point, px,
    };

    struct NestedStorage {
        outer: ScrollHandle,
        inner: ScrollHandle,
    }

    impl Render for NestedStorage {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("page-scroll")
                .w(px(400.))
                .h(px(500.))
                .overflow_y_scroll()
                .track_scroll(&self.outer)
                .child(
                    storage_copy_list(&self.inner)
                        .w(px(350.))
                        .children((0..20).map(|_| div().h(px(50.)).flex_shrink_0())),
                )
                .child(div().h(px(1000.)).flex_shrink_0())
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
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
        let inner = ScrollHandle::new();
        let outer = ScrollHandle::new();
        let (_, cx) = cx.add_window_view(|_, _| NestedStorage {
            inner: inner.clone(),
            outer: outer.clone(),
        });
        draw(cx);
        assert!(inner.max_offset().y > px(0.));
        assert!(outer.max_offset().y > px(0.));

        wheel(cx, 100., 100., -80.);
        assert_eq!(inner.offset().y, px(-80.));
        assert_eq!(outer.offset().y, px(0.), "inner wheel moved Settings");

        wheel(cx, 100., 100., -10000.);
        assert_eq!(inner.offset().y, -inner.max_offset().y);
        wheel(cx, 100., 100., -80.);
        assert_eq!(outer.offset().y, px(0.), "bottom boundary moved Settings");

        wheel(cx, 100., 100., 10000.);
        assert_eq!(inner.offset().y, px(0.));
        // Put Settings in the middle while keeping the top of the list visible.
        outer.set_offset(point(px(0.), px(-40.)));
        draw(cx);
        wheel(cx, 100., 100., 80.);
        assert_eq!(inner.offset().y, px(0.));
        assert_eq!(outer.offset().y, px(-40.), "top boundary moved Settings");

        // The narrow right-hand strip is outside the nested list.
        wheel(cx, 380., 100., -80.);
        assert_eq!(inner.offset().y, px(0.));
        assert_eq!(
            outer.offset().y,
            px(-120.),
            "Settings did not scroll outside list"
        );
    }
}
