//! Settings are edited as a copy. Only a successful worker commit replaces the
//! baseline; background snapshots must not overwrite unfinished input.
use crate::{
    app::{Page, StudyApp},
    components::*,
    controls::{Button, Checkbox},
    preferences::{PreferenceSnapshot, SavePreferences},
    worker::Work,
};
use gpui_kit::{
    component::{
        Disableable, Sizable,
        button::ButtonVariants,
        input::{Input, InputEvent, InputState},
    },
    *,
};
use std::collections::BTreeMap;

pub enum SettingsContinuation {
    Page(Page),
    NewNote,
    Picker(&'static str, Option<String>),
    Migrate,
    Close,
    ViewNote(String),
    ConfirmDeleteCopy(String),
    Record(crate::app::EditorMode),
    PickedWork(String, Box<Work>),
}

#[derive(Default)]
pub struct PreferencesUi {
    pub saved: Option<PreferenceSnapshot>,
    pub baseline: Option<PreferenceSnapshot>,
    pub draft: Option<studyapp_core::Settings>,
    pub shortcuts: BTreeMap<String, Entity<InputState>>,
    pub busy: bool,
    pub replacing: bool,
    pub leave: Option<SettingsContinuation>,
    pub reset_confirmation: bool,
    pub shortcuts_expanded: bool,
    pub error: Option<String>,
}

impl StudyApp {
    pub fn handle_shortcut(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.preferences_ui.busy || self.preferences_ui.replacing || keystroke.modifiers.platform
        {
            return false;
        }
        let modifiers = keystroke.modifiers;
        let key = if modifiers.shift {
            match keystroke.key.as_str() {
                "!" => "1",
                "@" => "2",
                "#" => "3",
                "$" => "4",
                "%" => "5",
                "^" => "6",
                "&" => "7",
                "*" => "8",
                "(" => "9",
                ")" => "0",
                key => key,
            }
        } else {
            keystroke.key.as_str()
        };
        let combination = format!(
            "{}{}{}{}",
            if modifiers.control { "Ctrl+" } else { "" },
            if modifiers.alt { "Alt+" } else { "" },
            if modifiers.shift { "Shift+" } else { "" },
            key
        );
        let Ok(combination) = crate::shortcuts::normalize_binding(&combination) else {
            return false;
        };
        let Some(saved) = &self.preferences_ui.saved else {
            return false;
        };
        if crate::native_shortcuts::handle_native_edit(&combination, &saved.shortcuts, window, cx) {
            return true;
        }
        if self.has_modal(cx) {
            return false;
        }
        let action = saved.shortcuts.iter().find_map(|(action, binding)| {
            binding
                .split(';')
                .any(|alias| {
                    crate::shortcuts::normalize_binding(alias)
                        .is_ok_and(|alias| alias == combination)
                })
                .then(|| action.clone())
        });
        action.is_some_and(|action| self.dispatch_shortcut(&action, window, cx))
    }

    pub fn dispatch_shortcut(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.has_modal(cx)
            || self.preferences_ui.busy
            || self.preferences_ui.replacing
            || self.storage_busy
        {
            return false;
        }
        match action {
            "app.new" => self.new_note(window, cx),
            "app.import" => self.settings_picker("import", None, cx),
            "app.settings" => self.navigate(Page::Settings, window, cx),
            "app.library" => self.navigate(Page::Notes, window, cx),
            "app.navigation-toggle" => self.toggle_navigation(cx),
            "app.folders-toggle" if self.page == Page::Notes => self.toggle_folders(cx),
            "app.search" => {
                self.navigate(Page::Notes, window, cx);
                if self.page == Page::Notes {
                    self.search.read(cx).focus_handle(cx).focus(window, cx);
                }
            }
            "editor.save" if self.page == Page::Notes && self.note_editor.is_some() => {
                self.save_note(window, cx)
            }
            _ => return false,
        }
        true
    }

    pub fn settings_dirty(&self, cx: &App) -> bool {
        let Some(baseline) = &self.preferences_ui.baseline else {
            return false;
        };
        self.preferences_ui.draft.as_ref() != Some(&baseline.settings)
            || self.interval.read(cx).value().trim()
                != baseline.settings.interval_minutes.to_string()
            || self.deadline.read(cx).value().trim() != baseline.settings.answer_minutes.to_string()
            || self
                .review_days
                .read(cx)
                .value()
                .replace([' ', '，'], ",")
                .split(',')
                .filter(|part| !part.is_empty())
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
                .ok()
                .as_ref()
                != Some(&baseline.settings.review_days)
            || self.preferences_ui.shortcuts.iter().any(|(id, input)| {
                Some(input.read(cx).value().as_str())
                    != baseline.shortcuts.get(id).map(String::as_str)
            })
            || self.sync_panel.read(cx).has_unsaved_changes(cx)
    }

    fn fill_settings(
        &mut self,
        value: PreferenceSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.interval.update(cx, |input, cx| {
            input.set_value(value.settings.interval_minutes.to_string(), window, cx)
        });
        self.deadline.update(cx, |input, cx| {
            input.set_value(value.settings.answer_minutes.to_string(), window, cx)
        });
        self.review_days.update(cx, |input, cx| {
            input.set_value(
                value
                    .settings
                    .review_days
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                window,
                cx,
            )
        });
        self.layout = value.settings.layout.clone();
        self.preferences_ui.draft = Some(value.settings.clone());
        for action in crate::shortcuts::SHORTCUT_ACTIONS {
            let binding = value.shortcuts.get(action.id).cloned().unwrap_or_default();
            if let Some(input) = self.preferences_ui.shortcuts.get(action.id) {
                input.update(cx, |input, cx| input.set_value(binding, window, cx));
            } else {
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("未绑定；例如 Ctrl+Shift+S")
                        .default_value(binding)
                });
                cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify())
                    .detach();
                self.preferences_ui
                    .shortcuts
                    .insert(action.id.into(), input);
            }
        }
        self.sync_panel.update(cx, |panel, cx| {
            panel.reset_draft(value.sync.clone(), window, cx)
        });
        self.preferences_ui.error = None;
        cx.notify();
    }

    pub fn begin_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(saved) = self.preferences_ui.saved.clone() {
            self.fill_settings(saved.clone(), window, cx);
            self.preferences_ui.baseline = Some(saved);
        }
    }

    pub fn accept_preferences(
        &mut self,
        preferences: PreferenceSnapshot,
        replaced: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_content.update(cx, |editor, cx| {
            editor.set_shortcuts(preferences.shortcuts.clone(), cx)
        });
        self.preferences_ui.saved = Some(preferences);
        if replaced || self.preferences_ui.baseline.is_none() {
            self.preferences_ui.leave = None;
            self.preferences_ui.busy = false;
            self.preferences_ui.replacing = self.pending_library.is_some();
            self.preferences_ui.reset_confirmation = false;
            self.begin_settings(window, cx);
        }
    }

    pub fn stage_layout(&mut self, cx: &mut Context<Self>) {
        if let Some(draft) = &mut self.preferences_ui.draft {
            draft.layout = self.layout.clone();
        }
        cx.notify();
    }

    pub fn guard_settings(
        &mut self,
        continuation: SettingsContinuation,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.preferences_ui.busy || self.preferences_ui.replacing {
            return true;
        }
        if self.page == Page::Settings && self.settings_dirty(cx) {
            if self.preferences_ui.leave.is_none() {
                self.preferences_ui.leave = Some(continuation);
                self.preferences_ui.error = None;
            }
            cx.notify();
            return true;
        }
        false
    }

    pub fn navigate(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if page == self.page || self.guard_settings(SettingsContinuation::Page(page), cx) {
            return;
        }
        if page == Page::Settings {
            self.begin_settings(window, cx);
        }
        self.page = page;
        cx.notify();
    }

    pub fn settings_picker(
        &mut self,
        action: &'static str,
        note: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if !self.guard_settings(SettingsContinuation::Picker(action, note.clone()), cx) {
            self.picker(action, note);
        }
    }

    fn resume_settings_action(
        &mut self,
        action: SettingsContinuation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            SettingsContinuation::Page(page) => self.navigate(page, window, cx),
            SettingsContinuation::NewNote => self.new_note(window, cx),
            SettingsContinuation::Picker(action, note) => self.picker(action, note),
            SettingsContinuation::Migrate => self.choose_storage(window, cx),
            SettingsContinuation::Close => {
                if self.handle_close(window, cx) {
                    self.close_allowed = true;
                    window.remove_window();
                }
            }
            SettingsContinuation::ViewNote(id) => {
                self.selected_version = self.snapshot.as_ref().and_then(|snapshot| {
                    snapshot
                        .versions
                        .iter()
                        .rev()
                        .find(|version| version.note_id == id)
                        .map(|version| version.id.clone())
                });
                self.selected_note = Some(id);
                self.history = true;
                self.page = Page::Notes;
                cx.notify();
            }
            SettingsContinuation::ConfirmDeleteCopy(id) => {
                self.storage_confirm = Some(id);
                cx.notify();
            }
            SettingsContinuation::Record(mode) => self.open_editor(mode, window, cx),
            SettingsContinuation::PickedWork(library, work) => {
                self.accept_picker_result(library, *work, cx)
            }
        }
    }

    pub fn accept_picker_result(&mut self, library: String, work: Work, cx: &mut Context<Self>) {
        if library != self.last_library {
            self.notice = Some(Err("资料库已切换，已取消旧文件选择操作。".into()));
        } else if self.storage_busy || self.preferences_ui.busy || self.preferences_ui.replacing {
            // Native pickers can finish out of order. Preserve their scoped
            // result until the operation owning the current locks completes.
            let _ = self.picker_sender.send((library, work));
        } else if self.page == Page::Settings && self.settings_dirty(cx) {
            self.guard_settings(
                SettingsContinuation::PickedWork(library, Box::new(work)),
                cx,
            );
        } else if matches!(&work, Work::MigrateLibrary(_)) {
            self.storage_work(work, cx);
        } else {
            let replacing = matches!(
                &work,
                Work::SwitchLibrary(_) | Work::Apply(studyapp_core::Command::Restore(_))
            );
            if self
                .worker
                .sender
                .send(Work::Scoped {
                    library,
                    work: Box::new(work),
                })
                .is_err()
            {
                self.notice = Some(Err("后台服务已停止，请重新打开应用。".into()));
            } else if replacing {
                self.preferences_ui.replacing = true;
                self.sync_panel.update(cx, |panel, cx| {
                    panel.set_editing_locked(true, cx);
                });
            }
        }
        cx.notify();
    }

    pub fn discard_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preferences_ui.busy {
            return;
        }
        let action = self.preferences_ui.leave.take();
        self.begin_settings(window, cx);
        if let Some(action) = action {
            self.resume_settings_action(action, window, cx);
        }
    }

    pub fn save_preferences(&mut self, cx: &mut Context<Self>) {
        if self.preferences_ui.busy
            || self.preferences_ui.replacing
            || self.storage_busy
            || self.sync_panel.read(cx).status.busy
        {
            return;
        }
        let prepared = (|| -> anyhow::Result<SavePreferences> {
            let baseline = self
                .preferences_ui
                .baseline
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("配置尚未加载"))?;
            let mut settings = self
                .preferences_ui
                .draft
                .clone()
                .ok_or_else(|| anyhow::anyhow!("配置尚未加载"))?;
            settings.interval_minutes = self
                .interval
                .read(cx)
                .value()
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("询问间隔必须是整数"))?;
            settings.answer_minutes = self
                .deadline
                .read(cx)
                .value()
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("回答期限必须是整数"))?;
            settings.review_days = self
                .review_days
                .read(cx)
                .value()
                .replace('，', ",")
                .split(',')
                .map(|part| {
                    part.trim()
                        .parse::<u32>()
                        .map_err(|_| anyhow::anyhow!("复习间隔请填写逗号分隔的正整数"))
                })
                .collect::<anyhow::Result<_>>()?;
            let shortcuts = self
                .preferences_ui
                .shortcuts
                .iter()
                .map(|(id, input)| (id.clone(), input.read(cx).value().trim().to_owned()))
                .collect::<BTreeMap<_, _>>();
            crate::shortcuts::validate_bindings(&shortcuts)?;
            let (sync, password) = self.sync_panel.read(cx).draft_settings(cx)?;
            Ok(SavePreferences {
                expected_revision: baseline.revision.clone(),
                settings,
                shortcuts,
                sync,
                password,
            })
        })();
        match prepared {
            Ok(prepared) => {
                if self
                    .worker
                    .sender
                    .send(Work::Scoped {
                        library: self.last_library.clone(),
                        work: Box::new(Work::SavePreferences(prepared)),
                    })
                    .is_ok()
                {
                    self.preferences_ui.busy = true;
                    self.sync_panel.update(cx, |panel, cx| {
                        panel.set_editing_locked(true, cx);
                    });
                    self.preferences_ui.error = None;
                } else {
                    self.preferences_ui.error = Some("后台服务已停止，修改仍保留".into());
                }
            }
            Err(error) => self.preferences_ui.error = Some(error.to_string()),
        }
        if let Some(error) = &self.preferences_ui.error {
            self.notice = Some(Err(error.clone()));
        }
        cx.notify();
    }

    pub fn preferences_finished(
        &mut self,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.preferences_ui.busy {
            return;
        }
        self.preferences_ui.busy = false;
        self.sync_panel.update(cx, |panel, cx| {
            panel.set_editing_locked(false, cx);
        });
        match result {
            Ok(()) => {
                let action = self.preferences_ui.leave.take();
                self.begin_settings(window, cx);
                self.notice = Some(Ok("全部设置已保存".into()));
                if let Some(action) = action {
                    self.resume_settings_action(action, window, cx);
                }
            }
            Err(error) => {
                self.preferences_ui.error = Some(error.clone());
                self.notice = Some(Err(format!("设置未保存：{error}；当前修改仍保留")));
            }
        }
        cx.notify();
    }

    pub fn settings_modal(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.preferences_ui.reset_confirmation {
            return Some(column().gap_4().child(heading("恢复初始设置？"))
                .child(muted("将学习、界面、快捷键和同步选项恢复为默认草稿。保存后才生效，笔记和学习记录保留。"))
                .child(row().justify_end()
                    .child(Button::new("cancel-reset-settings").label("取消").on_click(cx.listener(|this, _, _, cx| {
                        this.preferences_ui.reset_confirmation = false; cx.notify();
                    })))
                    .child(Button::new("confirm-reset-settings").primary().label("恢复默认草稿").on_click(cx.listener(|this, _, window, cx| {
                        this.preferences_ui.reset_confirmation = false;
                        this.fill_settings(PreferenceSnapshot::defaults(), window, cx);
                    }))))
                .into_any_element());
        }
        self.preferences_ui.leave.as_ref()?;
        let busy = self.preferences_ui.busy;
        Some(
            column()
                .gap_4()
                .child(heading("设置尚未保存"))
                .child(muted("是否保存本次修改后继续？放弃只撤销当前配置草稿。"))
                .children(
                    self.preferences_ui
                        .error
                        .as_ref()
                        .map(|error| muted(error.clone()).text_color(rgb(0xA34829))),
                )
                .child(
                    row()
                        .flex_wrap()
                        .justify_end()
                        .child(
                            Button::new("settings-continue-editing")
                                .label("继续编辑")
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.preferences_ui.leave = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("settings-discard")
                                .label("放弃修改")
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.discard_settings(window, cx)
                                })),
                        )
                        .child(
                            Button::new("settings-save-and-continue")
                                .primary()
                                .label(if busy {
                                    "正在保存…"
                                } else {
                                    "保存并继续"
                                })
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| this.save_preferences(cx))),
                        ),
                )
                .into_any_element(),
        )
    }

    pub fn preference_controls(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        let dirty = self.settings_dirty(cx);
        let draft = self.preferences_ui.draft.clone().unwrap_or_default();
        let mut shortcuts = column().gap_3();
        let mut group = "";
        for action in crate::shortcuts::SHORTCUT_ACTIONS {
            if action.group != group {
                group = action.group;
                shortcuts = shortcuts.child(heading(group));
            }
            if let Some(input) = self.preferences_ui.shortcuts.get(action.id) {
                shortcuts = shortcuts.child(
                    row().child(div().w(px(180.)).child(action.label)).child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(input).small().disabled(busy)),
                    ),
                );
            }
        }
        column().gap_4()
            .child(card().child(row().child(heading("设置")).child(div().flex_1())
                .child(muted(if dirty { "有未保存的修改" } else { "已保存" }))
                .child(Button::new("reset-settings").label("恢复初始设置").disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| { this.preferences_ui.reset_confirmation = true; cx.notify(); })))
                .child(Button::new("save-settings").primary().label(if self.preferences_ui.busy { "正在保存…" } else { "保存全部设置" })
                    .disabled(busy || self.preferences_ui.saved.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.save_preferences(cx))))))
            .child(card().child(heading("学习节奏"))
                .child(row().items_start()
                    .child(column().flex_1().gap_2().child(muted("询问间隔（5–240 分钟）")).child(Input::new(&self.interval).disabled(busy)))
                    .child(column().flex_1().gap_2().child(muted("回答期限（1–30 分钟）")).child(Input::new(&self.deadline).disabled(busy))))
                .child(column().gap_2().child(muted("复习间隔（天，以逗号分隔）")).child(Input::new(&self.review_days).disabled(busy)))
                .child(muted("保存后生效；询问间隔从下一时间段生效，复习间隔只影响新建计划。")))
            .child(card().child(heading("界面"))
                .child(Checkbox::new("show-file-paths").label("显示笔记文件的存储路径").checked(draft.show_file_paths).disabled(busy)
                    .on_click(cx.listener(|this, checked, _, cx| {
                        if let Some(draft) = &mut this.preferences_ui.draft { draft.show_file_paths = *checked; }
                        cx.notify();
                    })))
                .child(row()
                    .child(Checkbox::new("settings-navigation-visible").label("显示主导航").checked(draft.layout.navigation_visible).disabled(busy)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.layout.navigation_visible = *checked; this.stage_layout(cx);
                        })))
                    .child(Checkbox::new("settings-folders-visible").label("显示笔记目录").checked(draft.layout.folders_visible).disabled(busy)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.layout.folders_visible = *checked; this.stage_layout(cx);
                        }))))
                .child(muted("界面与侧栏调整在本页作为草稿保存；右键始终可以定位文件或复制路径。")))
            .child(card().child(row().child(heading("快捷键")).child(div().flex_1())
                .child(Button::new("expand-shortcuts").label(if self.preferences_ui.shortcuts_expanded { "收起快捷键" } else { "编辑快捷键" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.preferences_ui.shortcuts_expanded = !this.preferences_ui.shortcuts_expanded; cx.notify();
                    }))))
                .child(muted("组合键可自定义，多个组合用分号分隔；留空取消绑定。保存时检查冲突。Windows 系统快捷键由操作系统管理。"))
                .children(self.preferences_ui.shortcuts_expanded.then_some(shortcuts)))
    }
}
