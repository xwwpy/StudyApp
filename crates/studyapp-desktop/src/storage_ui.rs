use crate::{app::StudyApp, components::*, editor_lifecycle::NoteIntent, worker::Work};
use gpui_kit::{
    component::{
        Disableable, Sizable,
        button::{Button, ButtonVariants},
    },
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

impl StudyApp {
    pub fn storage_work(&mut self, work: Work, cx: &mut Context<Self>) {
        if self.storage_busy
            || self.note_saving
            || self.draft_discard.is_some()
            || self.draft_persist.is_some()
        {
            return;
        }
        self.storage_busy = true;
        self.folders
            .update(cx, |panel, cx| panel.set_busy(true, cx));
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
            self.notice = Some(Err("后台服务已停止，请重新启动应用".into()));
        }
        cx.notify();
    }

    pub fn choose_storage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.storage_busy
            || self.note_saving
            || self.draft_loading
            || self.draft_discard.is_some()
            || self.sync_panel.read(cx).status.busy
        {
            return;
        }
        if self.draft_failed {
            self.notice = Some(Err("请先重试草稿保存，再迁移资料库。".into()));
            cx.notify();
        } else if self.note_editor.is_some() {
            self.request_note_action(NoteIntent::PrepareMigration, window, cx);
        } else {
            self.pick_migration(window, cx);
        }
    }

    pub fn pick_migration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.storage_busy = true;
        self.storage_confirm = None;
        self.folders
            .update(cx, |panel, cx| panel.set_busy(true, cx));
        cx.spawn_in(window, async move |this, cx| {
            let path = cx
                .background_executor()
                .spawn(async {
                    rfd::FileDialog::new()
                        .set_title("选择新的空文件夹（迁移后保留原资料）")
                        .pick_folder()
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.storage_busy = false;
                this.folders
                    .update(cx, |panel, cx| panel.set_busy(false, cx));
                if let Some(path) = path {
                    this.storage_work(Work::MigrateLibrary(path), cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn storage_view(&self, cx: &mut Context<Self>) -> Div {
        let blocked = self.storage_busy || self.sync_panel.read(cx).status.busy;
        card().child(
            row().child(heading("空间管理")).child(div().flex_1()).child(
                Button::new("refresh-storage").small().disabled(self.storage_busy)
                    .label(if self.storage_busy { "正在处理…" } else { "统计占用空间" })
                    .on_click(cx.listener(|this, _, _, cx| this.storage_work(Work::RefreshStorage, cx))),
            ),
        )
        .child(muted("只可逐个删除当前资料库的托管副本。跟踪的外部文件不可删除；学习记录与历史版本始终保留。"))
        .when_some(self.storage_report.as_ref().filter(|r| r.data_path == self.last_library), |view, report| {
            let total = report.database_bytes.saturating_add(report.managed_bytes)
                .saturating_add(report.retained_bytes).saturating_add(report.auxiliary_bytes)
                .saturating_add(report.backup_bytes);
            view.child(heading(format!("当前资料库合计 {}", size_label(total))))
                .child(muted(format!("数据库与历史 {} · 托管副本 {} · 保留文件 {} · 备份 {} · 缓存与配置 {}",
                    size_label(report.database_bytes), size_label(report.managed_bytes),
                    size_label(report.retained_bytes), size_label(report.backup_bytes), size_label(report.auxiliary_bytes))))
                .child(column().id("storage-copy-list").test_support().max_h(px(340.)).overflow_y_scroll()
                    .when(report.copies.is_empty(), |v| v.child(muted("没有可清理的托管副本。")))
                    .children(report.copies.iter().map(|copy| {
                        let id = copy.note_id.clone();
                        let confirm_id = id.clone();
                        let history_id = id.clone();
                        let in_draft = self.note_editor.as_ref().or(self.recoverable_draft.as_ref())
                            .is_some_and(|d| d.note_id.as_ref() == Some(&id));
                        column().gap_2().p_3().rounded_lg().bg(rgb(BG))
                            .child(row().child(div().flex_1().child(copy.title.clone()))
                                .child(muted(size_label(copy.bytes)))
                                .child(Button::new(SharedString::from(format!("copy-history-{id}")))
                                    .small().ghost().label("历史与恢复").disabled(blocked || self.note_editor.is_some())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.selected_note = Some(history_id.clone());
                                        this.selected_version = this.snapshot.as_ref().and_then(|s|
                                            s.versions.iter().rev().find(|v| v.note_id == history_id).map(|v| v.id.clone()));
                                        this.page = crate::app::Page::Notes;
                                        this.history = true;
                                        cx.notify();
                                    })))
                                .child(Button::new(SharedString::from(format!("delete-copy-{id}")))
                                    .small().ghost().label("删除托管副本")
                                    .disabled(blocked || !copy.deletable || in_draft)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.storage_confirm = Some(id.clone()); cx.notify();
                                    }))))
                            .child(muted(display_path(&copy.path)).text_xs().truncate())
                            .when(copy.detached, |v| v.child(muted("已取消管理")))
                            .when(in_draft, |v| v.child(muted("请先保存或删除该笔记的草稿，再清理副本。")))
                            .when_some(copy.reason.as_ref(), |v, reason| v.child(muted(reason.clone())))
                            .when(self.storage_confirm.as_ref() == Some(&confirm_id), |v| {
                                v.child(muted("将删除上方这一份文件并停止管理。历史快照保留，可通过“历史与恢复”导出或复制。"))
                                    .child(row().child(Button::new("confirm-delete-copy").small()
                                        .disabled(blocked || in_draft || !copy.deletable).label("确认删除这一份")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.storage_confirm = None;
                                            this.storage_work(Work::DeleteManagedCopy { note_id: confirm_id.clone() }, cx);
                                        })))
                                        .child(Button::new("cancel-delete-copy").small().ghost().label("取消")
                                            .on_click(cx.listener(|this, _, _, cx| { this.storage_confirm = None; cx.notify(); }))))
                            })
                    })))
        })
    }
}
