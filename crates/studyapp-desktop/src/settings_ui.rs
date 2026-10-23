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

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SettingsSection {
    #[default]
    Learning,
    Appearance,
    Shortcuts,
    Storage,
    Sync,
    AboutAccount,
}

impl SettingsSection {
    pub const ALL: [(Self, &'static str, &'static str); 6] = [
        (Self::Learning, "learning", "学习"),
        (Self::Appearance, "appearance", "外观"),
        (Self::Shortcuts, "shortcuts", "快捷键"),
        (Self::Storage, "storage", "存储"),
        (Self::Sync, "sync", "同步"),
        (Self::AboutAccount, "about-account", "关于 / 账户"),
    ];
}

pub struct ShortcutRecording {
    pub action: String,
    pub append: bool,
}

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
    pub section: SettingsSection,
    pub recording: Option<ShortcutRecording>,
    pub recording_focus: Option<FocusHandle>,
    pub saved: Option<PreferenceSnapshot>,
    pub baseline: Option<PreferenceSnapshot>,
    pub draft: Option<studyapp_core::Settings>,
    pub shortcuts: BTreeMap<String, Entity<InputState>>,
    pub shortcut_search: Option<Entity<InputState>>,
    pub appearance_fields: BTreeMap<&'static str, Entity<InputState>>,
    pub appearance_touched: bool,
    pub editing_preset: Option<String>,
    pub image_root: Option<Entity<InputState>>,
    pub busy: bool,
    pub replacing: bool,
    pub leave: Option<SettingsContinuation>,
    pub reset_confirmation: bool,
    pub error: Option<String>,
}

fn select_custom_preset(
    settings: &mut studyapp_core::AppearanceSettings,
    pending: Option<studyapp_core::ThemePreset>,
    target: &str,
) -> anyhow::Result<()> {
    let mut next = settings.clone();
    if let Some(pending) = pending {
        if let Some(existing) = next
            .custom_presets
            .iter_mut()
            .find(|preset| preset.id == pending.id)
        {
            *existing = pending;
        } else {
            next.custom_presets.push(pending);
        }
    }
    next.mode = studyapp_core::ThemeMode::Custom;
    next.active_preset = Some(target.to_owned());
    next.validate()?;
    *settings = next;
    Ok(())
}

impl StudyApp {
    pub fn effective_appearance(&self) -> studyapp_core::AppearanceSettings {
        if self.page == Page::Settings
            && let Some(draft) = &self.preferences_ui.draft
        {
            return draft.appearance.clone();
        }
        self.preferences_ui
            .saved
            .as_ref()
            .map(|saved| saved.settings.appearance.clone())
            .unwrap_or_default()
    }

    fn fill_appearance_fields(
        &mut self,
        settings: &studyapp_core::AppearanceSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preset = settings
            .custom_presets
            .iter()
            .find(|preset| Some(&preset.id) == settings.active_preset.as_ref());
        let palette = preset
            .map(|preset| preset.palette.clone())
            .unwrap_or_default();
        self.preferences_ui.editing_preset = preset.map(|preset| preset.id.clone());
        for (key, value) in [
            (
                "name",
                preset
                    .map(|preset| preset.name.clone())
                    .unwrap_or_else(|| "我的主题".into()),
            ),
            ("accent", palette.accent),
            ("background", palette.background),
            ("surface", palette.surface),
            ("text", palette.text),
        ] {
            if let Some(input) = self.preferences_ui.appearance_fields.get(key) {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            } else {
                let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
                cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.preferences_ui.appearance_touched = true;
                    }
                    cx.notify();
                })
                .detach();
                self.preferences_ui.appearance_fields.insert(key, input);
            }
        }
        self.preferences_ui.appearance_touched = false;
    }

    fn custom_appearance_input(&self, cx: &Context<Self>) -> studyapp_core::ThemePreset {
        let value = |key| {
            self.preferences_ui
                .appearance_fields
                .get(key)
                .map(|input| input.read(cx).value().trim().to_owned())
                .unwrap_or_default()
        };
        let id = self
            .preferences_ui
            .editing_preset
            .clone()
            .unwrap_or_else(|| format!("theme-{}", crate::preferences::unique_revision()));
        studyapp_core::ThemePreset {
            id,
            name: value("name"),
            palette: studyapp_core::ThemePalette {
                accent: value("accent"),
                background: value("background"),
                surface: value("surface"),
                text: value("text"),
            },
        }
    }

    fn stage_custom_appearance(&mut self, cx: &mut Context<Self>) -> bool {
        let preset = self.custom_appearance_input(cx);
        let id = preset.id.clone();
        let Some(draft) = self.preferences_ui.draft.as_mut() else {
            return false;
        };
        match select_custom_preset(&mut draft.appearance, Some(preset), &id) {
            Ok(()) => {
                self.preferences_ui.editing_preset = Some(id);
                self.preferences_ui.appearance_touched = false;
                self.preferences_ui.error = None;
                cx.notify();
                true
            }
            Err(error) => {
                self.preferences_ui.error = Some(error.to_string());
                cx.notify();
                false
            }
        }
    }

    pub fn start_shortcut_recording(
        &mut self,
        action: &str,
        append: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preferences_ui.busy
            || self.preferences_ui.replacing
            || self.storage_busy
            || self.has_modal(cx)
        {
            return;
        }
        let focus = self
            .preferences_ui
            .recording_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        self.preferences_ui.recording = Some(ShortcutRecording {
            action: action.into(),
            append,
        });
        self.preferences_ui.error = None;
        focus.focus(window, cx);
        cx.notify();
    }

    /// Call before native editing, application dispatch, and focus actions.
    /// A true return consumes the keystroke even when it was invalid.
    pub fn capture_settings_keystroke(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(recording) = self.preferences_ui.recording.as_ref() else {
            return false;
        };
        if self.page != Page::Settings
            || self.has_modal(cx)
            || self.preferences_ui.busy
            || self.preferences_ui.replacing
            || !self
                .preferences_ui
                .recording_focus
                .as_ref()
                .is_some_and(|focus| focus.is_focused(window))
        {
            self.preferences_ui.recording = None;
            cx.notify();
            return false;
        }
        let action = recording.action.clone();
        let append = recording.append;
        let modifiers = keystroke.modifiers;
        match crate::shortcuts::capture_key(
            &keystroke.key,
            modifiers.control,
            modifiers.alt,
            modifiers.shift,
            modifiers.platform,
        ) {
            crate::shortcuts::CaptureKey::Pending => {}
            crate::shortcuts::CaptureKey::Cancel => {
                self.preferences_ui.recording = None;
                self.preferences_ui.error = None;
            }
            crate::shortcuts::CaptureKey::Invalid(error) => self.preferences_ui.error = Some(error),
            crate::shortcuts::CaptureKey::Binding(binding) => {
                if let Some(input) = self.preferences_ui.shortcuts.get(&action) {
                    match crate::shortcuts::record_binding(
                        input.read(cx).value().as_str(),
                        &binding,
                        append,
                    ) {
                        Ok(binding) => {
                            input.update(cx, |input, cx| input.set_value(binding, window, cx));
                            self.preferences_ui.recording = None;
                            self.preferences_ui.error = None;
                        }
                        Err(error) => self.preferences_ui.error = Some(error.to_string()),
                    }
                }
            }
        }
        if self.preferences_ui.recording.is_none()
            && let Some(input) = self.preferences_ui.shortcut_search.as_ref()
        {
            input.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
        true
    }

    pub fn handle_shortcut(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.capture_settings_keystroke(keystroke, window, cx) {
            return true;
        }
        if self.preferences_ui.busy || self.preferences_ui.replacing || keystroke.modifiers.platform
        {
            return false;
        }
        let modifiers = keystroke.modifiers;
        let Ok(combination) = crate::shortcuts::key_combination(
            &keystroke.key,
            modifiers.control,
            modifiers.alt,
            modifiers.shift,
        ) else {
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
        self.preferences_ui.appearance_touched
            || self
                .preferences_ui
                .image_root
                .as_ref()
                .is_some_and(|input| {
                    input.read(cx).value().trim()
                        != baseline.settings.images.root.as_deref().unwrap_or("")
                })
            || self.preferences_ui.draft.as_ref() != Some(&baseline.settings)
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
        self.preferences_ui.recording = None;
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
        self.fill_appearance_fields(&value.settings.appearance, window, cx);
        let root = value.settings.images.root.clone().unwrap_or_default();
        if let Some(input) = &self.preferences_ui.image_root {
            input.update(cx, |input, cx| input.set_value(root, window, cx));
        } else {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("留空使用资料库 images 目录")
                    .default_value(root)
            });
            cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify())
                .detach();
            self.preferences_ui.image_root = Some(input);
        }
        if self.preferences_ui.shortcut_search.is_none() {
            let input =
                cx.new(|cx| InputState::new(window, cx).placeholder("搜索操作、分组或快捷键"));
            cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify())
                .detach();
            self.preferences_ui.shortcut_search = Some(input);
        }
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
        self.refresh_appearance(window, cx);
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
        self.preferences_ui.recording = None;
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
        if self.media_busy() {
            self.push_notice(Err("正在保存图片，请稍候再切换页面。".into()), cx);
            return;
        }
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
        if self.media_busy() {
            self.push_notice(Err("正在保存图片，请稍候再操作资料库。".into()), cx);
            return;
        }
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
            self.push_notice(Err("资料库已切换，已取消旧文件选择操作。".into()), cx);
        } else if self.storage_busy
            || self.preferences_ui.busy
            || self.preferences_ui.replacing
            || self.media_jobs_active()
        {
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
                self.push_notice(Err("后台服务已停止，请重新打开应用。".into()), cx);
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
            || self.preferences_ui.recording.is_some()
            || self.storage_busy
            || self.sync_panel.read(cx).status.busy
        {
            return;
        }
        if self.preferences_ui.appearance_touched {
            let mode = self
                .preferences_ui
                .draft
                .as_ref()
                .map(|draft| draft.appearance.mode)
                .unwrap_or_default();
            if !self.stage_custom_appearance(cx) {
                return;
            }
            if mode != studyapp_core::ThemeMode::Custom
                && let Some(draft) = &mut self.preferences_ui.draft
            {
                draft.appearance.mode = mode;
            }
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
            settings.images.root = self
                .preferences_ui
                .image_root
                .as_ref()
                .map(|input| input.read(cx).value().trim().to_owned())
                .filter(|root| !root.is_empty());
            settings.validate()?;
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
            self.push_notice(Err(error.clone()), cx);
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
                self.push_notice(Ok("全部设置已保存".into()), cx);
                if let Some(action) = action {
                    self.resume_settings_action(action, window, cx);
                }
            }
            Err(error) => {
                self.preferences_ui.error = Some(error.clone());
                self.push_notice(Err(format!("设置未保存：{error}；当前修改仍保留")), cx);
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
                    self.preferences_ui.error.as_ref().map(|error| {
                        muted(error.clone()).text_color(crate::theme::color(0xA34829))
                    }),
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

    pub fn settings_header(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        let dirty = self.settings_dirty(cx);
        card()
            .child(
                row()
                    .flex_wrap()
                    .child(heading("设置"))
                    .child(div().flex_1())
                    .child(muted(if dirty {
                        "有未保存的修改"
                    } else {
                        "已保存"
                    }))
                    .child(
                        Button::new("reset-settings")
                            .small()
                            .label("恢复默认")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.preferences_ui.recording = None;
                                this.preferences_ui.reset_confirmation = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("save-settings")
                            .primary()
                            .small()
                            .label(if self.preferences_ui.busy {
                                "正在保存…"
                            } else {
                                "保存全部设置"
                            })
                            .disabled(
                                busy || self.preferences_ui.saved.is_none()
                                    || self.preferences_ui.recording.is_some(),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.save_preferences(cx))),
                    ),
            )
            .child(
                row()
                    .flex_wrap()
                    .children(
                        SettingsSection::ALL
                            .into_iter()
                            .map(|(section, id, label)| {
                                let button = Button::new(SharedString::from(format!(
                                    "settings-section-{id}"
                                )))
                                .small()
                                .label(label)
                                .disabled(busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.preferences_ui.section = section;
                                    this.preferences_ui.recording = None;
                                    cx.notify();
                                }));
                                if section == self.preferences_ui.section {
                                    button.primary()
                                } else {
                                    button
                                }
                            }),
                    ),
            )
            .children(
                self.preferences_ui
                    .error
                    .as_ref()
                    .map(|error| muted(error.clone()).text_color(crate::theme::color(0xBE3434))),
            )
    }

    pub fn preference_controls(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        match self.preferences_ui.section {
            SettingsSection::Learning => card()
                .child(heading("学习节奏"))
                .child(
                    row()
                        .items_start()
                        .child(
                            column()
                                .flex_1()
                                .gap_2()
                                .child(muted("询问间隔（5–240 分钟）"))
                                .child(Input::new(&self.interval).disabled(busy)),
                        )
                        .child(
                            column()
                                .flex_1()
                                .gap_2()
                                .child(muted("回答期限（1–30 分钟）"))
                                .child(Input::new(&self.deadline).disabled(busy)),
                        ),
                )
                .child(
                    column()
                        .gap_2()
                        .child(muted("复习间隔（天，以逗号分隔）"))
                        .child(Input::new(&self.review_days).disabled(busy)),
                )
                .child(muted(
                    "保存后生效；询问间隔从下一时间段生效，复习间隔只影响新建计划。",
                )),
            SettingsSection::Appearance => self.appearance_controls(cx),
            SettingsSection::Shortcuts => self.shortcut_controls(cx),
            SettingsSection::Storage => self.image_settings_controls(cx),
            _ => column(),
        }
    }

    fn appearance_controls(&self, cx: &mut Context<Self>) -> Div {
        use studyapp_core::ThemeMode;
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        let draft = self.preferences_ui.draft.clone().unwrap_or_default();
        let appearance = &draft.appearance;
        let mut panel = column().gap_3().child(
            card()
                .gap_3()
                .child(heading("主题与阅读"))
                .child(
                    row().flex_wrap().children(
                        [
                            (ThemeMode::System, "system", "跟随系统"),
                            (ThemeMode::Light, "light", "浅色"),
                            (ThemeMode::Dark, "dark", "深色"),
                        ]
                        .into_iter()
                        .map(|(mode, id, label)| {
                            let button =
                                Button::new(SharedString::from(format!("theme-mode-{id}")))
                                    .small()
                                    .label(label)
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if let Some(draft) = &mut this.preferences_ui.draft {
                                            draft.appearance.mode = mode;
                                        }
                                        this.refresh_appearance(window, cx);
                                        cx.notify();
                                    }));
                            if appearance.mode == mode {
                                button.primary()
                            } else {
                                button
                            }
                        }),
                    ),
                )
                .child(
                    row()
                        .child(div().flex_1().child("界面字号"))
                        .child(
                            Button::new("theme-font-smaller")
                                .small()
                                .label("−")
                                .disabled(busy || appearance.font_size <= 12)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(draft) = &mut this.preferences_ui.draft {
                                        draft.appearance.font_size =
                                            draft.appearance.font_size.saturating_sub(1).max(12);
                                    }
                                    this.refresh_appearance(window, cx);
                                    cx.notify();
                                })),
                        )
                        .child(muted(format!("{} px", appearance.font_size)))
                        .child(
                            Button::new("theme-font-larger")
                                .small()
                                .label("+")
                                .disabled(busy || appearance.font_size >= 22)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(draft) = &mut this.preferences_ui.draft {
                                        draft.appearance.font_size =
                                            (draft.appearance.font_size + 1).min(22);
                                    }
                                    this.refresh_appearance(window, cx);
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    row().child(div().flex_1().child("内容间距")).children(
                        [(4, "紧凑"), (8, "标准"), (12, "宽松")].into_iter().map(
                            |(spacing, label)| {
                                let button = Button::new(("theme-spacing", spacing as usize))
                                    .small()
                                    .label(label)
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if let Some(draft) = &mut this.preferences_ui.draft {
                                            draft.appearance.spacing = spacing;
                                        }
                                        this.refresh_appearance(window, cx);
                                        cx.notify();
                                    }));
                                if appearance.spacing == spacing {
                                    button.primary()
                                } else {
                                    button
                                }
                            },
                        ),
                    ),
                )
                .child(muted("修改可即时预览；保存全部设置后才会写入资料库。")),
        );
        let mut custom = card().gap_3().child(
            row()
                .child(heading("自定义主题"))
                .child(div().flex_1())
                .child(
                    Button::new("new-custom-theme")
                        .small()
                        .label("+ 新建")
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.preferences_ui.appearance_touched
                                && !this.stage_custom_appearance(cx)
                            {
                                return;
                            }
                            this.fill_appearance_fields(
                                &studyapp_core::AppearanceSettings::default(),
                                window,
                                cx,
                            );
                            cx.notify();
                        })),
                ),
        );
        for preset in &appearance.custom_presets {
            let id = preset.id.clone();
            let delete_id = id.clone();
            let selected = appearance.mode == ThemeMode::Custom
                && appearance.active_preset.as_ref() == Some(&id);
            let mut select = Button::new(SharedString::from(format!("select-theme-{id}")))
                .small()
                .label(format!(
                    "{}{}",
                    if selected { "✓ " } else { "" },
                    preset.name
                ))
                .disabled(busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    let pending = this
                        .preferences_ui
                        .appearance_touched
                        .then(|| this.custom_appearance_input(cx));
                    let settings = if let Some(draft) = &mut this.preferences_ui.draft {
                        if let Err(error) =
                            select_custom_preset(&mut draft.appearance, pending, &id)
                        {
                            this.preferences_ui.error = Some(error.to_string());
                            cx.notify();
                            return;
                        }
                        this.preferences_ui.error = None;
                        Some(draft.appearance.clone())
                    } else {
                        None
                    };
                    if let Some(settings) = settings {
                        this.fill_appearance_fields(&settings, window, cx);
                    }
                    this.refresh_appearance(window, cx);
                    cx.notify();
                }));
            if selected {
                select = select.primary();
            }
            custom = custom.child(
                row().child(select).child(div().flex_1()).child(
                    Button::new(SharedString::from(format!("delete-theme-{delete_id}")))
                        .small()
                        .label("删除")
                        .disabled(busy)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(draft) = &mut this.preferences_ui.draft {
                                draft
                                    .appearance
                                    .custom_presets
                                    .retain(|preset| preset.id != delete_id);
                                if draft.appearance.active_preset.as_ref() == Some(&delete_id) {
                                    draft.appearance.active_preset = None;
                                    draft.appearance.mode = ThemeMode::System;
                                }
                            }
                            if this.preferences_ui.editing_preset.as_ref() == Some(&delete_id) {
                                this.fill_appearance_fields(
                                    &studyapp_core::AppearanceSettings::default(),
                                    window,
                                    cx,
                                );
                            }
                            this.refresh_appearance(window, cx);
                            cx.notify();
                        })),
                ),
            );
        }
        for (key, label) in [
            ("name", "主题名称"),
            ("accent", "强调色"),
            ("background", "窗口背景"),
            ("surface", "卡片背景"),
            ("text", "文字颜色"),
        ] {
            if let Some(input) = self.preferences_ui.appearance_fields.get(key) {
                let mut field = row().child(div().w(px(100.)).text_sm().child(label));
                if key != "name" {
                    let color = studyapp_core::parse_theme_color(input.read(cx).value().trim())
                        .unwrap_or(crate::theme::palette().surface);
                    field = field.child(
                        div()
                            .size(px(22.))
                            .rounded_md()
                            .border_1()
                            .border_color(crate::theme::color(BORDER))
                            .bg(gpui_kit::rgb(color)),
                    );
                }
                custom = custom.child(
                    field.child(
                        div()
                            .flex_1()
                            .child(Input::new(input).small().disabled(busy)),
                    ),
                );
            }
        }
        custom = custom.child(
            row()
                .child(
                    Button::new("preview-custom-theme")
                        .small()
                        .primary()
                        .label(if self.preferences_ui.editing_preset.is_some() {
                            "更新并预览"
                        } else {
                            "创建并预览"
                        })
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.stage_custom_appearance(cx) {
                                this.refresh_appearance(window, cx);
                            }
                        })),
                )
                .child(muted("#RRGGBB · 自动检查文字对比度")),
        );
        panel = panel.child(custom).child(
            card()
                .gap_3()
                .child(heading("工作区"))
                .child(
                    Checkbox::new("show-file-paths")
                        .label("显示笔记文件的存储路径")
                        .checked(draft.show_file_paths)
                        .disabled(busy)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            if let Some(draft) = &mut this.preferences_ui.draft {
                                draft.show_file_paths = *checked;
                            }
                            cx.notify();
                        })),
                )
                .child(
                    row()
                        .flex_wrap()
                        .child(
                            Checkbox::new("settings-navigation-visible")
                                .label("显示主导航")
                                .checked(draft.layout.navigation_visible)
                                .disabled(busy)
                                .on_click(cx.listener(|this, checked, _, cx| {
                                    this.layout.navigation_visible = *checked;
                                    this.stage_layout(cx);
                                })),
                        )
                        .child(
                            Checkbox::new("settings-folders-visible")
                                .label("显示资料库")
                                .checked(draft.layout.folders_visible)
                                .disabled(busy)
                                .on_click(cx.listener(|this, checked, _, cx| {
                                    this.layout.folders_visible = *checked;
                                    this.stage_layout(cx);
                                })),
                        ),
                ),
        );
        panel
    }

    fn image_settings_controls(&self, cx: &mut Context<Self>) -> Div {
        use studyapp_core::ImageStorage;
        let draft = self.preferences_ui.draft.clone().unwrap_or_default();
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        let mut panel = card().gap_3().child(heading("图片存储")).child(
            row()
                .child(div().w(px(100.)).text_sm().child("保存方式"))
                .flex_wrap()
                .children(
                    [
                        (ImageStorage::Local, "local", "本地"),
                        (ImageStorage::Cloud, "cloud", "云端"),
                        (ImageStorage::Both, "both", "本地与云端"),
                    ]
                    .into_iter()
                    .map(|(storage, id, label)| {
                        let button =
                            Button::new(SharedString::from(format!("images-storage-{id}")))
                                .small()
                                .label(label)
                                .disabled(busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(draft) = &mut this.preferences_ui.draft {
                                        draft.images.storage = storage;
                                    }
                                    cx.notify();
                                }));
                        if draft.images.storage == storage {
                            button.primary()
                        } else {
                            button
                        }
                    }),
                ),
        );
        if let Some(input) = &self.preferences_ui.image_root {
            panel = panel.child(
                row()
                    .child(div().w(px(100.)).text_sm().child("本地图片目录"))
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(input).small().disabled(busy)),
                    ),
            );
        }
        panel.child(muted("目录须为绝对路径，留空使用当前资料库。云端使用「同步」分类中配置的服务；设置与其他修改一起保存。"))
    }

    fn shortcut_controls(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        let bindings = self
            .preferences_ui
            .shortcuts
            .iter()
            .map(|(id, input)| (id.clone(), input.read(cx).value().to_string()))
            .collect();
        let conflicts = crate::shortcuts::binding_conflicts(&bindings);
        let mut shortcuts = card().gap_2().child(heading("快捷键")).child(muted(
            "点击键帽录制，Esc 取消；＋追加组合，×清空。冲突须先解决。",
        ));
        if let Some(search) = &self.preferences_ui.shortcut_search {
            shortcuts = shortcuts.child(Input::new(search).small().disabled(busy));
        }
        let query = self
            .preferences_ui
            .shortcut_search
            .as_ref()
            .map(|search| search.read(cx).value().trim().to_lowercase())
            .unwrap_or_default();
        if let Some(recording) = &self.preferences_ui.recording {
            let label = crate::shortcuts::SHORTCUT_ACTIONS
                .iter()
                .find(|action| action.id == recording.action)
                .map_or("快捷键", |action| action.label);
            let mut capture = div()
                .id("shortcut-recording-target")
                .test_support()
                .p_3()
                .border_1()
                .border_color(crate::theme::color(GREEN))
                .child(format!("正在录制：{label} · 请按组合键（Esc 取消）"));
            if let Some(focus) = &self.preferences_ui.recording_focus {
                capture = capture.track_focus(focus);
            }
            shortcuts = shortcuts.child(capture).child(
                Button::new("cancel-shortcut-recording")
                    .label("取消录制")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.preferences_ui.recording = None;
                        this.preferences_ui.error = None;
                        cx.notify();
                    })),
            );
        }
        let mut group = "";
        let mut shown = 0;
        for action in crate::shortcuts::SHORTCUT_ACTIONS {
            let binding = self
                .preferences_ui
                .shortcuts
                .get(action.id)
                .map(|input| input.read(cx).value().to_string())
                .unwrap_or_default();
            if !query.is_empty()
                && !format!(
                    "{} {} {} {}",
                    action.label, action.id, action.group, binding
                )
                .to_lowercase()
                .contains(&query)
            {
                continue;
            }
            shown += 1;
            if action.group != group {
                group = action.group;
                shortcuts = shortcuts.child(heading(group));
            }
            if let Some(input) = self.preferences_ui.shortcuts.get(action.id) {
                let id = action.id;
                let error = crate::shortcuts::normalize_binding(input.read(cx).value().as_str())
                    .err()
                    .map(|error| error.to_string());
                let counterparts = conflicts.get(id).map(|others| {
                    others
                        .iter()
                        .map(|other| {
                            crate::shortcuts::SHORTCUT_ACTIONS
                                .iter()
                                .find(|action| action.id == other)
                                .map_or(other.as_str(), |action| action.label)
                        })
                        .collect::<Vec<_>>()
                        .join("、")
                });
                let invalid = error.is_some() || counterparts.is_some();
                let color = if invalid { 0xBE3434 } else { BORDER };
                let mut entry = column()
                    .id(SharedString::from(format!("shortcut-row-{id}")))
                    .test_support()
                    .gap_1()
                    .p_2()
                    .border_1()
                    .border_color(crate::theme::color(color))
                    .child(
                        row()
                            .flex_wrap()
                            .child(
                                div()
                                    .w(px(160.))
                                    .text_color(crate::theme::color(if invalid {
                                        0xBE3434
                                    } else {
                                        INK
                                    }))
                                    .child(action.label),
                            )
                            .child({
                                let recording = self
                                    .preferences_ui
                                    .recording
                                    .as_ref()
                                    .is_some_and(|recording| recording.action == id);
                                let mut keys = row()
                                    .id(SharedString::from(format!("record-shortcut-{id}")))
                                    .test_support()
                                    .flex_1()
                                    .min_w(px(150.))
                                    .min_h(px(30.))
                                    .gap_1()
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.start_shortcut_recording(id, false, window, cx)
                                    }));
                                if recording {
                                    keys = keys.child(
                                        muted("请按组合键…").text_color(crate::theme::color(GREEN)),
                                    );
                                } else if binding.trim().is_empty() {
                                    keys = keys.child(muted("点击设置"));
                                } else {
                                    for (index, alias) in binding.split(';').enumerate() {
                                        if index > 0 {
                                            keys = keys.child(muted(" / "));
                                        }
                                        for key in alias.split('+') {
                                            keys =
                                                keys.child(
                                                    div()
                                                        .px_2()
                                                        .py_1()
                                                        .rounded_md()
                                                        .border_1()
                                                        .border_color(crate::theme::color(color))
                                                        .bg(crate::theme::color(BG))
                                                        .text_xs()
                                                        .text_color(crate::theme::color(
                                                            if invalid { 0xBE3434 } else { INK },
                                                        ))
                                                        .child(key.to_owned()),
                                                );
                                        }
                                    }
                                }
                                keys
                            })
                            .child(
                                Button::new(SharedString::from(format!("append-shortcut-{id}")))
                                    .small()
                                    .label("＋")
                                    .tooltip("追加快捷键")
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.start_shortcut_recording(id, true, window, cx)
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("clear-shortcut-{id}")))
                                    .small()
                                    .label("×")
                                    .tooltip("清空快捷键")
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.preferences_ui.recording = None;
                                        if let Some(input) = this.preferences_ui.shortcuts.get(id) {
                                            input.update(cx, |input, cx| {
                                                input.set_value("", window, cx)
                                            });
                                        }
                                        cx.notify();
                                    })),
                            ),
                    );
                if let Some(counterparts) = counterparts {
                    entry = entry.child(
                        muted(format!("冲突：{counterparts}"))
                            .id(SharedString::from(format!("shortcut-conflict-{id}")))
                            .test_support()
                            .text_color(crate::theme::color(0xBE3434)),
                    );
                }
                if let Some(error) = error {
                    entry = entry.child(muted(error).text_color(crate::theme::color(0xBE3434)));
                }
                shortcuts = shortcuts.child(entry);
            }
        }
        if shown == 0 {
            shortcuts = shortcuts.child(muted("没有匹配的快捷键"));
        }
        shortcuts
    }
}

#[cfg(test)]
mod appearance_tests {
    use super::select_custom_preset;
    use studyapp_core::{AppearanceSettings, ThemePalette, ThemePreset};

    fn preset(id: &str) -> ThemePreset {
        ThemePreset {
            id: id.into(),
            name: id.into(),
            palette: ThemePalette::default(),
        }
    }

    #[test]
    fn selecting_another_or_same_preset_preserves_unsaved_fields() {
        let baseline = AppearanceSettings {
            active_preset: Some("a".into()),
            custom_presets: vec![preset("a"), preset("b")],
            ..Default::default()
        };
        let mut draft = baseline.clone();
        let mut edited = preset("a");
        edited.name = "edited a".into();
        edited.palette.accent = "#215F99".into();
        select_custom_preset(&mut draft, Some(edited.clone()), "b").unwrap();
        assert_eq!(draft.custom_presets[0], edited);
        assert_ne!(
            draft, baseline,
            "switching must retain dirty/save/discard semantics"
        );
        select_custom_preset(&mut draft, None, "a").unwrap();
        assert_eq!(draft.active_palette(), Some(&edited.palette));
        edited.name = "reselected a".into();
        select_custom_preset(&mut draft, Some(edited.clone()), "a").unwrap();
        assert_eq!(draft.custom_presets[0], edited);
        assert_eq!(
            baseline.custom_presets[0].name, "a",
            "saved baseline remains available for discard"
        );
    }

    #[test]
    fn invalid_pending_fields_block_selection_without_changing_draft() {
        let mut draft = AppearanceSettings {
            active_preset: Some("a".into()),
            custom_presets: vec![preset("a"), preset("b")],
            ..Default::default()
        };
        let before = draft.clone();
        let mut invalid = preset("a");
        invalid.palette.text = invalid.palette.surface.clone();
        assert!(select_custom_preset(&mut draft, Some(invalid), "b").is_err());
        assert_eq!(
            draft, before,
            "invalid input must stay on the current preset intact"
        );
    }
}
