//! Settings are edited as a copy. Only a successful worker commit replaces the
//! baseline; background snapshots must not overwrite unfinished input.
use crate::{
    app::{Page, StudyApp},
    components::*,
    controls::{Button, Checkbox},
    preferences::{PreferenceSnapshot, SavePreferences},
    settings_cache::InputCache,
    settings_components as form,
    worker::Work,
};
use gpui_kit::{
    component::{
        Disableable, Sizable,
        button::ButtonVariants,
        input::{InputEvent, InputState},
    },
    *,
};
use std::{collections::BTreeMap, rc::Rc, time::Instant};

const MARKDOWN_FIELDS: [(&str, &str, &str, u8); 24] = [
    ("md-body", "body", "正文", 0),
    ("md-heading", "heading", "标题", 0),
    ("md-link", "link", "链接", 0),
    ("md-muted", "muted", "辅助文字", 0),
    ("md-quote", "quote", "引用文字", 1),
    ("md-quote-background", "quote_background", "引用背景", 1),
    ("md-code", "code", "代码文字", 1),
    ("md-code-background", "code_background", "代码背景", 1),
    ("md-table", "table", "表格文字", 2),
    ("md-table-header", "table_header", "表头文字", 2),
    ("md-table-background", "table_background", "表格背景", 2),
    ("md-table-stripe", "table_stripe", "交替行背景", 2),
    ("md-list", "list", "列表标记", 2),
    ("md-grid", "grid", "表格边线", 2),
    ("md-code-border", "code_border", "代码边线", 1),
    ("md-keyword", "syntax_keyword", "关键字", 3),
    ("md-string", "syntax_string", "字符串", 3),
    ("md-number", "syntax_number", "数字", 3),
    ("md-comment", "syntax_comment", "注释", 3),
    ("md-type", "syntax_type", "类型", 3),
    ("md-property", "syntax_property", "属性", 3),
    ("md-tag", "syntax_tag", "标签", 3),
    ("md-function", "syntax_function", "函数", 3),
    ("md-punctuation", "syntax_punctuation", "标点", 3),
];

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

#[derive(Default)]
struct ShortcutAnalysis {
    bindings: BTreeMap<String, String>,
    conflicts: BTreeMap<String, Vec<String>>,
    errors: BTreeMap<String, String>,
    search: BTreeMap<String, String>,
}

type AppearanceInputKey = (Vec<SharedString>, studyapp_core::MarkdownLayout);
type AppearanceViewKey = (
    AppearanceInputKey,
    u8,
    studyapp_core::Settings,
    crate::theme::ResolvedTheme,
    bool,
    bool,
    Option<String>,
    u64,
);

pub struct AppearanceView {
    owner: WeakEntity<StudyApp>,
    key: Option<AppearanceViewKey>,
}

impl Render for AppearanceView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.owner
            .update(cx, |owner, cx| owner.appearance_workspace(cx))
            .unwrap_or_else(|_| column())
    }
}

struct MarkdownPreview {
    preset: studyapp_core::ThemePreset,
    colors: [u32; 24],
}

#[derive(PartialEq)]
struct ShortcutLayout {
    analysis_revision: u64,
    recording: Option<String>,
    recording_error: Option<String>,
    font_size: u16,
    spacing: u16,
}

pub enum SettingsContinuation {
    Page(Page),
    NewNote,
    Picker(&'static str, Option<String>),
    Migrate,
    Close,
    InstallUpdate,
    ViewNote(String),
    ConfirmDeleteCopy(String),
    Record(crate::app::EditorMode),
    PickedWork(String, Box<Work>),
}

#[derive(Default)]
pub struct PreferencesUi {
    pub section: SettingsSection,
    pub section_revision: u64,
    pub section_changed_at: Option<Instant>,
    pub recording: Option<ShortcutRecording>,
    pub recording_focus: Option<FocusHandle>,
    pub recording_error: Option<String>,
    pub saved: Option<PreferenceSnapshot>,
    pub baseline: Option<PreferenceSnapshot>,
    pub draft: Option<studyapp_core::Settings>,
    pub shortcuts: BTreeMap<String, Entity<InputState>>,
    shortcut_analysis: InputCache<Vec<SharedString>, ShortcutAnalysis>,
    shortcut_rows: form::VirtualRows<ShortcutLayout>,
    #[cfg(feature = "ui-test")]
    pub shortcut_eager_baseline: bool,
    #[cfg(feature = "ui-test")]
    shortcut_row_builds: std::cell::Cell<u64>,
    #[cfg(feature = "ui-test")]
    shortcut_frame_rows: std::cell::Cell<u64>,
    pub shortcut_search: Option<Entity<InputState>>,
    pub appearance_fields: BTreeMap<&'static str, Entity<InputState>>,
    markdown_preview: InputCache<AppearanceInputKey, MarkdownPreview>,
    appearance_rows: form::VirtualRows<AppearanceViewKey>,
    appearance_focus: Vec<FocusHandle>,
    appearance_view: Option<Entity<AppearanceView>>,
    #[cfg(feature = "ui-test")]
    pub appearance_eager_baseline: bool,
    #[cfg(feature = "ui-test")]
    appearance_part_builds: std::cell::Cell<u64>,
    #[cfg(feature = "ui-test")]
    appearance_frame_parts: std::cell::Cell<u64>,
    pub appearance_touched: bool,
    pub editing_preset: Option<String>,
    pub appearance_layout: studyapp_core::MarkdownLayout,
    pub appearance_group: u8,
    pub image_root: Option<Entity<InputState>>,
    pub image_cloud: Option<crate::image_cloud::ImageCloudDraft>,
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
    fn shortcut_analysis(&self, cx: &App) -> Rc<ShortcutAnalysis> {
        // Values are cheap shared strings. Programmatic set_value suppresses
        // InputEvent::Change, so capture/clear must invalidate by actual content.
        let values: Vec<_> = crate::shortcuts::SHORTCUT_ACTIONS
            .iter()
            .map(|action| {
                self.preferences_ui
                    .shortcuts
                    .get(action.id)
                    .map(|input| input.read(cx).value())
                    .unwrap_or_default()
            })
            .collect();
        self.preferences_ui.shortcut_analysis.get(values, |values| {
            let bindings: BTreeMap<_, _> = crate::shortcuts::SHORTCUT_ACTIONS
                .iter()
                .zip(values)
                .map(|(action, value)| (action.id.to_owned(), value.to_string()))
                .collect();
            let conflicts = crate::shortcuts::binding_conflicts(&bindings);
            let errors = bindings
                .iter()
                .filter_map(|(id, binding)| {
                    crate::shortcuts::normalize_binding(binding)
                        .err()
                        .map(|error| (id.clone(), error.to_string()))
                })
                .collect();
            let search = crate::shortcuts::SHORTCUT_ACTIONS
                .iter()
                .map(|action| {
                    (
                        action.id.to_owned(),
                        format!(
                            "{} {} {} {}",
                            action.label,
                            action.id,
                            action.group,
                            bindings.get(action.id).map_or("", String::as_str)
                        )
                        .to_lowercase(),
                    )
                })
                .collect();
            ShortcutAnalysis {
                bindings,
                conflicts,
                errors,
                search,
            }
        })
    }

    #[cfg(any(test, feature = "ui-test"))]
    pub fn shortcut_analysis_rebuilds(&self) -> u64 {
        self.preferences_ui.shortcut_analysis.rebuilds()
    }

    #[cfg(feature = "ui-test")]
    pub fn shortcut_render_work(&self) -> (u64, u32) {
        (
            self.preferences_ui.shortcut_row_builds.get(),
            self.preferences_ui.shortcut_frame_rows.get().count_ones(),
        )
    }

    #[cfg(feature = "ui-test")]
    pub fn reset_shortcut_scroll(&self) {
        self.preferences_ui.shortcut_rows.scroll_to_top();
        self.preferences_ui.appearance_rows.scroll_to_top();
        self.settings_scroll.set_offset(point(px(0.), px(0.)));
    }

    fn markdown_preview(&self, cx: &Context<Self>) -> Rc<MarkdownPreview> {
        let values = self
            .preferences_ui
            .appearance_fields
            .values()
            .map(|input| input.read(cx).value())
            .collect();
        self.preferences_ui.markdown_preview.get(
            (values, self.preferences_ui.appearance_layout),
            |_| {
                let preset = self.custom_appearance_input(cx);
                let colors = preset.markdown.resolved_colors(&preset.palette);
                MarkdownPreview { preset, colors }
            },
        )
    }

    #[cfg(any(test, feature = "ui-test"))]
    pub fn appearance_preview_rebuilds(&self) -> u64 {
        self.preferences_ui.markdown_preview.rebuilds()
    }

    pub fn select_settings_section(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        if self.preferences_ui.section != section {
            self.preferences_ui.section = section;
            self.preferences_ui.section_revision =
                self.preferences_ui.section_revision.wrapping_add(1);
            self.preferences_ui.section_changed_at = Some(Instant::now());
            self.settings_scroll.set_offset(point(px(0.), px(0.)));
            self.preferences_ui.shortcut_rows.scroll_to_top();
            self.preferences_ui.appearance_rows.scroll_to_top();
        }
        self.preferences_ui.recording = None;
        self.preferences_ui.recording_error = None;
        cx.notify();
    }

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
        if self.preferences_ui.appearance_view.is_none() {
            let owner = cx.weak_entity();
            self.preferences_ui.appearance_view =
                Some(cx.new(|_| AppearanceView { owner, key: None }));
            self.preferences_ui.appearance_focus = (0..5).map(|_| cx.focus_handle()).collect();
        }
        let preset = settings
            .custom_presets
            .iter()
            .find(|preset| Some(&preset.id) == settings.active_preset.as_ref());
        let palette = preset
            .map(|preset| preset.palette.clone())
            .unwrap_or_default();
        self.preferences_ui.editing_preset = preset.map(|preset| preset.id.clone());
        let markdown = preset
            .map(|preset| preset.markdown.clone())
            .unwrap_or_default();
        self.preferences_ui.appearance_layout = markdown.layout;
        let mut fields = vec![
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
        ];
        fields.extend(MARKDOWN_FIELDS.iter().map(|(key, token, _, _)| {
            (
                *key,
                markdown.colors.get(*token).cloned().unwrap_or_default(),
            )
        }));
        fields.extend([
            ("md-body-size", markdown.layout.body_size.to_string()),
            ("md-code-size", markdown.layout.code_size.to_string()),
            ("md-line-height", markdown.layout.line_height.to_string()),
            (
                "md-reading-width",
                markdown.layout.reading_width.to_string(),
            ),
            ("md-code-radius", markdown.layout.code_radius.to_string()),
        ]);
        for (key, value) in fields {
            if let Some(input) = self.preferences_ui.appearance_fields.get(key) {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
            } else {
                let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
                cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.preferences_ui.appearance_touched = true;
                        cx.notify();
                    }
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
        let mut layout = self.preferences_ui.appearance_layout;
        layout.body_size = value("md-body-size").parse().unwrap_or(u16::MAX);
        layout.code_size = value("md-code-size").parse().unwrap_or(u16::MAX);
        layout.line_height = value("md-line-height").parse().unwrap_or(u16::MAX);
        layout.reading_width = value("md-reading-width").parse().unwrap_or(u16::MAX);
        layout.code_radius = value("md-code-radius").parse().unwrap_or(u16::MAX);
        studyapp_core::ThemePreset {
            id,
            name: value("name"),
            markdown: studyapp_core::MarkdownStyle {
                colors: MARKDOWN_FIELDS
                    .iter()
                    .filter_map(|(key, token, _, _)| {
                        let color = value(*key);
                        (!color.is_empty()).then(|| ((*token).to_owned(), color))
                    })
                    .collect(),
                layout,
            },
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
        self.preferences_ui.recording_error = None;
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
                self.preferences_ui.recording_error = None;
            }
            crate::shortcuts::CaptureKey::Invalid(error) => {
                self.preferences_ui.recording_error = Some(error)
            }
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
                            self.preferences_ui.recording_error = None;
                        }
                        Err(error) => self.preferences_ui.recording_error = Some(error.to_string()),
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
                .image_cloud
                .as_ref()
                .is_some_and(|draft| draft.dirty(&baseline.settings.images, cx))
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
        self.preferences_ui.recording_error = None;
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
        self.preferences_ui.image_cloud = Some(crate::image_cloud::ImageCloudDraft::new(
            &value.settings.images,
            value.image_secret_saved,
            window,
            cx,
        ));
        let root = value.settings.images.root.clone().unwrap_or_default();
        if let Some(input) = &self.preferences_ui.image_root {
            input.update(cx, |input, cx| input.set_value(root, window, cx));
        } else {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("留空使用资料库 images 目录")
                    .default_value(root)
            });
            cx.subscribe(&input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            })
            .detach();
            self.preferences_ui.image_root = Some(input);
        }
        if self.preferences_ui.shortcut_search.is_none() {
            let input =
                cx.new(|cx| InputState::new(window, cx).placeholder("搜索操作、分组或快捷键"));
            cx.subscribe(&input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            })
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
                cx.subscribe(&input, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
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
        if self.note_editor.is_some() {
            self.request_note_action(
                crate::editor_lifecycle::NoteIntent::Navigate(page),
                window,
                cx,
            );
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
            SettingsContinuation::InstallUpdate => self.request_install_update(window, cx),
            SettingsContinuation::ViewNote(id) => {
                let version = self.snapshot.as_ref().and_then(|s| {
                    s.versions
                        .iter()
                        .rev()
                        .find(|v| v.note_id == id)
                        .map(|v| v.id.clone())
                });
                self.select_note_document(id, version, true, window, cx);
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
        } else if !self.draft_work_ready
            && matches!(
                &work,
                Work::SwitchLibrary(_)
                    | Work::MigrateLibrary(_)
                    | Work::Apply(studyapp_core::Command::Restore(_))
            )
        {
            self.draft_deferred_work = Some((library, work));
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
            let image_secret = if let Some(image) = &self.preferences_ui.image_cloud {
                settings.images.cloud = image.target(cx);
                image.secret_change(cx)
            } else {
                Default::default()
            };
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
                image_secret,
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
        column()
            .min_w_0()
            .gap_2()
            .px_2()
            .py_2()
            .child(
                row()
                    .flex_wrap()
                    .child(heading("偏好设置").text_base())
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
            .children(
                self.preferences_ui
                    .error
                    .as_ref()
                    .map(|error| muted(error.clone()).text_color(crate::theme::color(0xBE3434))),
            )
    }

    pub fn settings_navigation(&self, compact: bool, cx: &mut Context<Self>) -> Div {
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        let navigation = if compact {
            row().flex_wrap().gap_1().pb_2()
        } else {
            column().w(px(136.)).flex_shrink_0().gap_1().pt_1()
        };
        navigation.children(
            SettingsSection::ALL
                .into_iter()
                .map(|(section, id, label)| {
                    let selected = section == self.preferences_ui.section;
                    let mut button =
                        Button::new(SharedString::from(format!("settings-section-{id}")))
                            .small()
                            .ghost()
                            .label(label)
                            .disabled(busy)
                            .rounded_md()
                            .text_color(crate::theme::color(if selected { INK } else { MUTED }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_settings_section(section, cx)
                            }));
                    if !compact {
                        button = button.w_full().justify_start().h(px(36.));
                    }
                    if selected {
                        button = button
                            .bg(crate::theme::selection_color())
                            .font_weight(FontWeight::SEMIBOLD);
                    }
                    button
                }),
        )
    }

    pub fn preference_controls(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        match self.preferences_ui.section {
            SettingsSection::Learning => form::section("学习节奏")
                .child(
                    row()
                        .flex_wrap()
                        .items_start()
                        .child(
                            form::field(
                                "询问间隔（5–240 分钟）",
                                form::input(&self.interval, busy),
                            )
                            .flex_1()
                            .min_w(px(190.)),
                        )
                        .child(
                            form::field("回答期限（1–30 分钟）", form::input(&self.deadline, busy))
                                .flex_1()
                                .min_w(px(190.)),
                        ),
                )
                .child(form::field(
                    "复习间隔（天，以逗号分隔）",
                    form::input(&self.review_days, busy),
                ))
                .child(muted(
                    "保存后生效；询问间隔从下一时间段生效，复习间隔只影响新建计划。",
                )),
            SettingsSection::Appearance => column(),
            // The shortcut list owns the bounded viewport, outside this
            // auto-height form used by the other categories.
            SettingsSection::Shortcuts => column(),
            SettingsSection::Storage => self.image_settings_controls(cx),
            _ => column(),
        }
    }

    fn markdown_style_preview(&self, cx: &mut Context<Self>) -> Div {
        use studyapp_core::ReadingFont;
        let preview = self.markdown_preview(cx);
        let preset = &preview.preset;
        let colors = &preview.colors;
        let p = &preset.markdown.layout;
        let color = |index| gpui_kit::rgb(colors[index]);
        let size = if p.body_size == 0 {
            self.effective_appearance().font_size
        } else {
            p.body_size.clamp(12, 28)
        } as f32;
        let code_size = if p.code_size == 0 {
            (size - 1.).max(12.)
        } else {
            p.code_size.clamp(12, 26) as f32
        };
        let font = |value| match value {
            ReadingFont::Sans => "Microsoft YaHei UI",
            ReadingFont::Serif => "SimSun",
            ReadingFont::Mono => "Consolas",
        };
        let sample = column()
            .id("theme-markdown-preview")
            .test_support()
            .gap_2()
            .p_3()
            .bg(gpui_kit::rgb(
                studyapp_core::parse_theme_color(&preset.palette.background).unwrap_or(0xFFFFFF),
            ))
            .text_color(color(0))
            .text_size(px(size))
            .font_family(font(p.body_font))
            .line_height(px(size
                * if p.line_height == 0 {
                    1.8
                } else {
                    p.line_height.clamp(130, 240) as f32 / 100.
                }))
            .child(
                div()
                    .text_size(px(size * 1.5))
                    .font_weight(FontWeight::BOLD)
                    .text_color(color(1))
                    .child("阅读样例 · Markdown"),
            )
            .child(
                row()
                    .flex_wrap()
                    .child("正文清晰易读，")
                    .child(div().text_color(color(2)).child("链接指向延伸阅读"))
                    .child(div().text_color(color(3)).child(" · 辅助文字")),
            )
            .child(
                div()
                    .p_2()
                    .border_l_2()
                    .border_color(color(13))
                    .bg(color(5))
                    .text_color(color(4))
                    .child("引用：知识来自观察、验证与思考。"),
            )
            .child(
                column()
                    .child(div().text_color(color(12)).child("• 一级要点"))
                    .child(div().pl_5().text_color(color(12)).child("1. 二级编号"))
                    .child(div().pl_10().child("☑ 三级任务")),
            )
            .child(
                column()
                    .p_2()
                    .rounded(px(p.code_radius.min(24) as f32))
                    .border(if p.code_border { px(1.) } else { px(0.) })
                    .border_color(color(14))
                    .bg(color(7))
                    .text_color(color(6))
                    .font_family(font(p.mono_font))
                    .text_size(px(code_size))
                    .child(
                        row()
                            .child(div().text_color(color(15)).child("const "))
                            .child("answer = ")
                            .child(div().text_color(color(17)).child("42"))
                            .child(";"),
                    )
                    .child(
                        div()
                            .text_color(color(18))
                            .child("// 保持代码与注释的可读性"),
                    ),
            )
            .child(
                column()
                    .border(if p.table_grid { px(1.) } else { px(0.) })
                    .border_color(color(13))
                    .child(
                        row()
                            .p_2()
                            .bg(gpui_kit::rgb(
                                studyapp_core::parse_theme_color(&preset.palette.surface)
                                    .unwrap_or(0xFFFFFF),
                            ))
                            .text_color(color(9))
                            .child(div().flex_1().child("概念"))
                            .child(div().flex_1().child("进度")),
                    )
                    .child(
                        row()
                            .p_2()
                            .bg(color(10))
                            .text_color(color(8))
                            .child(div().flex_1().child("阅读"))
                            .child(div().flex_1().child("完成")),
                    )
                    .child(
                        row()
                            .p_2()
                            .bg(if p.table_stripes {
                                color(11)
                            } else {
                                color(10)
                            })
                            .text_color(color(8))
                            .child(div().flex_1().child("实践"))
                            .child(div().flex_1().child("进行中")),
                    ),
            );
        column()
            .gap_3()
            .child(heading("Markdown 样式"))
            .child(muted(
                "颜色留空会按主题自动适配；样例随输入更新。保存主题后可在笔记中使用。",
            ))
            .child(sample)
    }

    fn markdown_style_controls(&self, busy: bool, cx: &mut Context<Self>) -> Div {
        use studyapp_core::ReadingFont;
        let preview = self.markdown_preview(cx);
        let preset = &preview.preset;
        let colors = &preview.colors;
        let p = &preset.markdown.layout;
        let color = |index| gpui_kit::rgb(colors[index]);
        let mut panel = column().gap_3().child(
            row().flex_wrap().children(
                ["文字与版式", "引用与代码", "表格与列表", "语法高亮"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, label)| {
                        Button::new(("markdown-style-group", index))
                            .small()
                            .label(format!(
                                "{}{}",
                                if self.preferences_ui.appearance_group as usize == index {
                                    "✓ "
                                } else {
                                    ""
                                },
                                label
                            ))
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.preferences_ui.appearance_group = index as u8;
                                cx.notify();
                            }))
                    }),
            ),
        );
        for (index, (key, _, label, group)) in MARKDOWN_FIELDS.iter().enumerate() {
            if *group != self.preferences_ui.appearance_group {
                continue;
            }
            if let Some(input) = self.preferences_ui.appearance_fields.get(key) {
                let value = input.read(cx).value().trim().to_owned();
                let parsed = studyapp_core::parse_theme_color(&value);
                let invalid = !value.is_empty() && parsed != Some(colors[index]);
                let mut field = column().gap_1().child(
                    row()
                        .child(div().w(px(100.)).text_sm().child(*label))
                        .child(
                            div()
                                .size(px(22.))
                                .rounded_md()
                                .border_1()
                                .border_color(crate::theme::color(BORDER))
                                .bg(color(index)),
                        )
                        .child(div().flex_1().child(form::input(input, busy))),
                );
                if invalid {
                    field = field.child(
                        muted(format!(
                            "{label}：{}",
                            if parsed.is_none() {
                                "请输入 #RRGGBB，或留空自动适配"
                            } else {
                                "与实际背景对比度须达到 4.5:1"
                            }
                        ))
                        .text_color(crate::theme::color(0xBE3434)),
                    );
                }
                panel = panel.child(field);
            }
        }
        let group = self.preferences_ui.appearance_group;
        for (key, label, min, max, inherit, field_group) in [
            ("md-body-size", "正文字号 px", 12, 28, true, 0),
            ("md-line-height", "行高 %", 130, 240, true, 0),
            ("md-reading-width", "阅读宽度 px", 480, 1600, false, 0),
            ("md-code-size", "代码字号 px", 12, 26, true, 1),
            ("md-code-radius", "代码圆角 px", 0, 24, false, 1),
        ] {
            if group != field_group {
                continue;
            }
            if let Some(input) = self.preferences_ui.appearance_fields.get(key) {
                let number = input.read(cx).value().trim().parse::<u16>().ok();
                let valid = number.is_some_and(|n| (min..=max).contains(&n) || (inherit && n == 0));
                panel = panel.child(
                    row()
                        .child(div().w(px(100.)).text_sm().child(label))
                        .child(div().flex_1().child(form::input(input, busy))),
                );
                panel = panel.child(
                    muted(format!(
                        "{min}–{max}{}",
                        if inherit {
                            "；0 表示继承界面设置"
                        } else {
                            ""
                        }
                    ))
                    .text_color(crate::theme::color(if valid {
                        MUTED
                    } else {
                        0xBE3434
                    })),
                );
            }
        }
        if group <= 1 {
            panel = panel.child(
                row()
                    .child(div().w(px(100.)).child(if group == 0 {
                        "正文字体"
                    } else {
                        "代码字体"
                    }))
                    .children(
                        [
                            (ReadingFont::Sans, "无衬线"),
                            (ReadingFont::Serif, "衬线"),
                            (ReadingFont::Mono, "等宽"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (font, label))| {
                            let selected = if group == 0 { p.body_font } else { p.mono_font };
                            Button::new(("markdown-font", index))
                                .small()
                                .label(format!(
                                    "{}{}",
                                    if selected == font { "✓ " } else { "" },
                                    label
                                ))
                                .disabled(busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if group == 0 {
                                        this.preferences_ui.appearance_layout.body_font = font;
                                    } else {
                                        this.preferences_ui.appearance_layout.mono_font = font;
                                    }
                                    this.preferences_ui.appearance_touched = true;
                                    cx.notify();
                                }))
                        }),
                    ),
            );
        }
        for (key, label, checked, field_group) in [
            ("grid", "显示表格网格", p.table_grid, 2),
            ("stripes", "表格交替行背景", p.table_stripes, 2),
            ("border", "显示代码边框", p.code_border, 1),
        ] {
            if group == field_group {
                panel = panel.child(
                    Checkbox::new(SharedString::from(format!("markdown-{key}")))
                        .label(label)
                        .checked(checked)
                        .disabled(busy)
                        .on_click(cx.listener(move |this, value, _, cx| {
                            let style = &mut this.preferences_ui.appearance_layout;
                            match key {
                                "grid" => style.table_grid = *value,
                                "stripes" => style.table_stripes = *value,
                                _ => style.code_border = *value,
                            }
                            this.preferences_ui.appearance_touched = true;
                            cx.notify();
                        })),
                );
            }
        }
        panel.child(
            row()
                .flex_wrap()
                .child(
                    Button::new("reset-markdown-style")
                        .small()
                        .label("恢复 Markdown 默认样式")
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            let mut preset = this.custom_appearance_input(cx);
                            preset.markdown = Default::default();
                            let settings = studyapp_core::AppearanceSettings {
                                active_preset: Some(preset.id.clone()),
                                custom_presets: vec![preset],
                                ..Default::default()
                            };
                            this.fill_appearance_fields(&settings, window, cx);
                            this.preferences_ui.appearance_touched = true;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("discard-theme-fields")
                        .small()
                        .label("放弃本次编辑")
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            let settings = this
                                .preferences_ui
                                .draft
                                .as_ref()
                                .map(|draft| draft.appearance.clone())
                                .unwrap_or_default();
                            this.fill_appearance_fields(&settings, window, cx);
                            cx.notify();
                        })),
                ),
        )
    }

    fn appearance_view_key(&self, cx: &App) -> AppearanceViewKey {
        #[cfg(not(feature = "ui-test"))]
        let eager = false;
        #[cfg(feature = "ui-test")]
        let eager = self.preferences_ui.appearance_eager_baseline;
        (
            (
                self.preferences_ui
                    .appearance_fields
                    .values()
                    .map(|input| input.read(cx).value())
                    .collect(),
                self.preferences_ui.appearance_layout,
            ),
            self.preferences_ui.appearance_group,
            self.preferences_ui.draft.clone().unwrap_or_default(),
            crate::theme::palette(),
            self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy,
            eager,
            self.preferences_ui.editing_preset.clone(),
            self.preferences_ui.section_revision,
        )
    }

    /// A definite-size view boundary lets GPUI reuse unchanged appearance
    /// layout/paint when only the parent shell (timer, notices) is notified.
    pub fn appearance_surface(&self, cx: &mut Context<Self>) -> Div {
        let Some(view) = self.preferences_ui.appearance_view.as_ref() else {
            return column();
        };
        let key = self.appearance_view_key(cx);
        view.update(cx, |view, cx| {
            if view.key.as_ref() != Some(&key) {
                view.key = Some(key);
                cx.notify();
            }
        });
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .max_w(px(960.))
            .child(view.clone().cached(StyleRefinement::default().size_full()))
    }

    fn appearance_workspace(&self, cx: &mut Context<Self>) -> Div {
        // Keep programmatic field changes reflected even when the sample is
        // outside the viewport; cached resolution is independent of row layout.
        let _ = self.markdown_preview(cx);
        let key = self.appearance_view_key(cx);
        self.preferences_ui
            .appearance_rows
            .sync_with_height(&[0, 1, 2, 3, 4], key, px(320.));
        self.preferences_ui.appearance_rows.track_focuses(
            self.preferences_ui
                .appearance_focus
                .iter()
                .cloned()
                .enumerate()
                .collect(),
        );
        #[cfg(feature = "ui-test")]
        self.preferences_ui.appearance_frame_parts.set(0);
        #[cfg(feature = "ui-test")]
        if self.preferences_ui.appearance_eager_baseline {
            return column().size_full().min_h_0().child(
                scroll_body("settings-section-scroll", &self.settings_scroll)
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .gap_0()
                    .px_4()
                    .children(
                        (0..5).map(|part| div().py_2().child(self.appearance_part(part, cx))),
                    ),
            );
        }
        let owner = cx.entity();
        column().size_full().min_h_0().child(form::virtual_rows(
            &self.preferences_ui.appearance_rows.scroll,
            move |part, _, cx| {
                owner
                    .update(cx, |owner, cx| {
                        div()
                            .py_2()
                            .track_focus(&owner.preferences_ui.appearance_focus[part])
                            .child(owner.appearance_part(part, cx))
                    })
                    .into_any_element()
            },
        ))
    }

    #[cfg(feature = "ui-test")]
    pub fn appearance_render_work(&self) -> (u64, u32) {
        (
            self.preferences_ui.appearance_part_builds.get(),
            self.preferences_ui
                .appearance_frame_parts
                .get()
                .count_ones(),
        )
    }

    fn appearance_part(&self, part: usize, cx: &mut Context<Self>) -> Div {
        #[cfg(feature = "ui-test")]
        {
            self.preferences_ui
                .appearance_part_builds
                .set(self.preferences_ui.appearance_part_builds.get() + 1);
            self.preferences_ui
                .appearance_frame_parts
                .set(self.preferences_ui.appearance_frame_parts.get() | (1 << part));
        }
        use studyapp_core::ThemeMode;
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        let fallback;
        let draft = if let Some(draft) = self.preferences_ui.draft.as_ref() {
            draft
        } else {
            fallback = studyapp_core::Settings::default();
            &fallback
        };
        let appearance = &draft.appearance;
        if part == 0 {
            return column().gap_3().child(
                form::section("主题与阅读")
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
                                            draft.appearance.font_size = draft
                                                .appearance
                                                .font_size
                                                .saturating_sub(1)
                                                .max(12);
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
                    .child(row().child(div().flex_1().child("内容间距")).children(
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
                    ))
                    .child(muted("修改可即时预览；保存全部设置后才会写入资料库。")),
            );
        }
        if part == 1 {
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
                    custom =
                        custom.child(field.child(div().flex_1().child(form::input(input, busy))));
                }
            }
            return custom;
        }
        if part == 2 {
            return form::section("Markdown 预览").child(self.markdown_style_preview(cx));
        }
        if part == 3 {
            return form::section("Markdown 参数")
                .child(self.markdown_style_controls(busy, cx))
                .child(
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
        }
        column().child(
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
        )
    }

    pub fn poll_image_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(cloud) = &mut self.preferences_ui.image_cloud else {
            return;
        };
        if self.page != Page::Settings
            || self.preferences_ui.busy
            || self.preferences_ui.replacing
            || self.storage_busy
        {
            cloud.picker.invalidate();
            return;
        }
        if let Some(path) = cloud.picker.poll(&self.last_library)
            && let Some(input) = &self.preferences_ui.image_root
        {
            input.update(cx, |input, cx| {
                input.set_value(path.to_string_lossy().into_owned(), window, cx)
            });
            cx.notify();
        }
    }

    fn image_settings_controls(&self, cx: &mut Context<Self>) -> Div {
        use studyapp_core::ImageStorage;
        let draft = self.preferences_ui.draft.clone().unwrap_or_default();
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        let mut panel = form::section("图片存储").child(
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
                                        if draft.images.storage == ImageStorage::Local
                                            && storage != ImageStorage::Local
                                            && matches!(
                                        draft.images.cloud,
                                        studyapp_core::media_settings::ImageCloudTarget::NoteSync
                                    ) && let Some(cloud) =
                                            &mut this.preferences_ui.image_cloud
                                        {
                                            // A new cloud choice starts with the generic form.
                                            // Legacy reuse remains an explicit visible option.
                                            cloud.kind = 0;
                                        }
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
                    .child(div().flex_1().child(form::input(input, busy)))
                    .child(
                        Button::new("pick-image-root")
                            .small()
                            .label("选择文件夹")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let raw = this
                                    .preferences_ui
                                    .image_root
                                    .as_ref()
                                    .map(|input| input.read(cx).value().trim().to_owned())
                                    .unwrap_or_default();
                                let current = if raw.is_empty() {
                                    std::path::Path::new(&this.last_library)
                                        .parent()
                                        .map(|path| path.join("images"))
                                } else {
                                    Some(std::path::PathBuf::from(raw))
                                };
                                if let Some(cloud) = &mut this.preferences_ui.image_cloud {
                                    cloud.picker.choose(
                                        &this.last_library,
                                        current,
                                        "选择本地图片目录",
                                    );
                                }
                            })),
                    ),
            );
        }
        panel = panel.child(muted(
            "留空使用当前资料库 images 目录。选择目录只修改草稿，不迁移或删除已有图片。",
        ));
        if draft.images.storage != ImageStorage::Local
            && let Some(cloud) = &self.preferences_ui.image_cloud
        {
            panel = panel.child(heading("图片云端服务")).child(
                row().flex_wrap().children(
                    [
                        (0u8, "s3", "通用对象存储（S3 兼容）"),
                        (1u8, "note-sync", "复用笔记同步（WebDAV / NAS）"),
                    ]
                    .into_iter()
                    .map(|(kind, id, label)| {
                        let button =
                            Button::new(SharedString::from(format!("image-provider-{id}")))
                                .small()
                                .label(label)
                                .disabled(busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(cloud) = &mut this.preferences_ui.image_cloud {
                                        cloud.kind = kind;
                                    }
                                    cx.notify();
                                }));
                        if cloud.kind == kind {
                            button.primary()
                        } else {
                            button
                        }
                    }),
                ),
            );
            if cloud.kind == 0 {
                panel = panel.child(muted("兼容协议：S3 · AWS Signature V4。请填写服务商的 S3 兼容入口，不是控制台登录地址。"));
                for (key, label) in [
                    ("endpoint", "Endpoint"),
                    ("bucket", "Bucket"),
                    ("region", "Region"),
                    ("access_key_id", "Access Key ID"),
                    ("secret", "Secret"),
                    ("prefix", "对象前缀"),
                ] {
                    panel = panel.child(
                        row().child(div().w(px(112.)).text_sm().child(label)).child(
                            div()
                                .id(SharedString::from(format!("image-cloud-field-{key}")))
                                .test_support()
                                .flex_1()
                                .child(form::input(&cloud.fields[key], busy)),
                        ),
                    );
                }
                panel = panel.child(row().child(div().w(px(112.)).text_sm().child("寻址方式"))
                    .children([(studyapp_core::media_settings::ObjectAddressing::VirtualHosted, "virtual", "虚拟主机"), (studyapp_core::media_settings::ObjectAddressing::Path, "path", "路径")].into_iter().map(|(style, id, label)| {
                        let button = Button::new(SharedString::from(format!("image-addressing-{id}"))).small().label(label).disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(cloud) = &mut this.preferences_ui.image_cloud { cloud.addressing = style; }
                                cx.notify();
                            }));
                        if cloud.addressing == style { button.primary() } else { button }
                    })))
                    .child(muted("虚拟主机：bucket.endpoint/key；路径：endpoint/bucket/key。不同服务的兼容范围与要求不同，请按服务商文档选择。"))
                    .child(row().child(muted(if cloud.clear_secret { "保存后将清除当前密钥" } else if cloud.secret_saved { "当前目标的密钥已加密保存；留空保留。更换目标需重新填写。" } else { "请填写 Secret；它只会加密保存，不进入笔记或编辑器。" }))
                        .child(Button::new("clear-image-secret").small().ghost().label("清除已保存密钥").disabled(busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(cloud) = &mut this.preferences_ui.image_cloud {
                                    cloud.clear_secret = true;
                                    cloud.fields["secret"].update(cx, |input, cx| input.set_value("", window, cx));
                                }
                                cx.notify();
                            }))));
            } else {
                let target = match &self.sync_panel.read(cx).status.settings.target {
                    crate::sync_runtime::SyncTarget::Disabled => {
                        "笔记同步尚未配置；请先在同步分类配置 WebDAV 或文件夹 / NAS".to_owned()
                    }
                    crate::sync_runtime::SyncTarget::Folder { .. } => {
                        "已明确复用笔记同步的文件夹 / NAS 目标".into()
                    }
                    crate::sync_runtime::SyncTarget::WebDav { .. } => {
                        "已明确复用笔记同步的 WebDAV 目标及受保护账户".into()
                    }
                };
                panel = panel.child(muted(target)).child(
                    Button::new("image-open-sync-settings")
                        .small()
                        .label("查看同步配置")
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.select_settings_section(SettingsSection::Sync, cx);
                        })),
                );
            }
            let dirty = self.settings_dirty(cx);
            let running = busy || self.image_cloud_busy;
            panel = panel
                .child(
                    row()
                        .child(
                            Button::new("test-image-cloud")
                                .small()
                                .label("测试图片云端连接")
                                .disabled(running || dirty)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this
                                        .worker
                                        .sender
                                        .send(Work::Scoped {
                                            library: this.last_library.clone(),
                                            work: Box::new(Work::TestImageCloud),
                                        })
                                        .is_err()
                                    {
                                        this.push_notice(Err("后台服务已停止".into()), cx);
                                    }
                                })),
                        )
                        .child(
                            Button::new("retry-image-cloud")
                                .small()
                                .label("重试图片上传")
                                .disabled(running || dirty)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this
                                        .worker
                                        .sender
                                        .send(Work::Scoped {
                                            library: this.last_library.clone(),
                                            work: Box::new(Work::RetryImageCloud),
                                        })
                                        .is_err()
                                    {
                                        this.push_notice(Err("后台服务已停止".into()), cx);
                                    }
                                })),
                        ),
                )
                .child(muted(if dirty {
                    "请先保存全部设置，再测试连接或上传已有图片。"
                } else if self.image_cloud_busy {
                    "正在处理图片云端任务…"
                } else {
                    "连接和上传使用已保存的配置，不会在输入时联网。"
                }));
        }
        panel.child(muted("云端模式保留本地离线缓存；上传失败时本地图片仍保留，可重试。更改存储方式不会自动补传旧图片，请保存后点击重试图片上传。"))
    }

    pub fn shortcut_controls(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        let analysis = self.shortcut_analysis(cx);
        let mut header = form::section("快捷键").child(muted(
            "点击当前绑定录制，Esc 取消；＋追加组合，×清空。冲突会显示在对应操作下。",
        ));
        if let Some(search) = &self.preferences_ui.shortcut_search {
            header = header.child(form::input(search, busy));
        }
        let query = self
            .preferences_ui
            .shortcut_search
            .as_ref()
            .map(|search| search.read(cx).value().trim().to_lowercase())
            .unwrap_or_default();
        let rows: Vec<usize> = crate::shortcuts::SHORTCUT_ACTIONS
            .iter()
            .enumerate()
            .filter(|(_, action)| {
                self.preferences_ui
                    .recording
                    .as_ref()
                    .is_some_and(|recording| recording.action == action.id)
                    || query.is_empty()
                    || analysis
                        .search
                        .get(action.id)
                        .is_some_and(|search| search.contains(&query))
            })
            .map(|(ix, _)| ix)
            .collect();
        let theme = crate::theme::palette();
        self.preferences_ui.shortcut_rows.sync(
            &rows,
            ShortcutLayout {
                analysis_revision: self.preferences_ui.shortcut_analysis.rebuilds(),
                recording: self
                    .preferences_ui
                    .recording
                    .as_ref()
                    .map(|recording| recording.action.clone()),
                recording_error: self.preferences_ui.recording_error.clone(),
                font_size: theme.font_size,
                spacing: theme.spacing,
            },
        );
        self.preferences_ui.shortcut_rows.track_focus(
            self.preferences_ui
                .recording
                .as_ref()
                .and_then(|recording| {
                    let row = rows.iter().position(|&ix| {
                        crate::shortcuts::SHORTCUT_ACTIONS[ix].id == recording.action
                    })?;
                    self.preferences_ui
                        .recording_focus
                        .clone()
                        .map(|focus| (row, focus))
                }),
        );
        #[cfg(feature = "ui-test")]
        self.preferences_ui.shortcut_frame_rows.set(0);
        let workspace = column()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .max_w(px(960.))
            .gap_3()
            .child(header);
        if rows.is_empty() {
            return workspace.child(
                div()
                    .id("settings-section-scroll")
                    .test_support()
                    .flex_1()
                    .child(muted("没有匹配的快捷键")),
            );
        }
        // Keep an exact eager control only in the CI binary. Both paths use the
        // same row builder, typography and variable-height contents for A/B timing.
        #[cfg(feature = "ui-test")]
        if self.preferences_ui.shortcut_eager_baseline {
            return workspace.child(
                scroll_body("settings-section-scroll", &self.settings_scroll)
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .gap_0()
                    .px_4()
                    .pb_4()
                    .children(rows.iter().enumerate().map(|(row, &ix)| {
                        let group = row == 0
                            || crate::shortcuts::SHORTCUT_ACTIONS[rows[row - 1]].group
                                != crate::shortcuts::SHORTCUT_ACTIONS[ix].group;
                        self.shortcut_row(ix, group, &analysis, cx)
                    })),
            );
        }
        let owner = cx.entity();
        workspace.child(form::virtual_rows(
            &self.preferences_ui.shortcut_rows.scroll,
            move |row, _, cx| {
                let ix = rows[row];
                let group = row == 0
                    || crate::shortcuts::SHORTCUT_ACTIONS[rows[row - 1]].group
                        != crate::shortcuts::SHORTCUT_ACTIONS[ix].group;
                owner
                    .update(cx, |this, cx| this.shortcut_row(ix, group, &analysis, cx))
                    .into_any_element()
            },
        ))
    }

    fn shortcut_row(
        &self,
        index: usize,
        show_group: bool,
        analysis: &ShortcutAnalysis,
        cx: &mut Context<Self>,
    ) -> Div {
        let busy = self.preferences_ui.busy || self.preferences_ui.replacing || self.storage_busy;
        let action = &crate::shortcuts::SHORTCUT_ACTIONS[index];
        let id = action.id;
        #[cfg(feature = "ui-test")]
        {
            self.preferences_ui
                .shortcut_row_builds
                .set(self.preferences_ui.shortcut_row_builds.get() + 1);
            self.preferences_ui
                .shortcut_frame_rows
                .set(self.preferences_ui.shortcut_frame_rows.get() | (1 << index));
        }
        let recording = self
            .preferences_ui
            .recording
            .as_ref()
            .is_some_and(|recording| recording.action == id);
        let binding = analysis.bindings.get(id).map_or("", String::as_str);
        let error = analysis.errors.get(id);
        let counterparts = analysis.conflicts.get(id).map(|others| {
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
        let mut keys = form::binding_cell()
            .id(SharedString::from(format!("record-shortcut-{id}")))
            .test_support()
            .flex_1()
            .min_w(px(170.))
            .flex_wrap()
            .border_color(crate::theme::color(if recording {
                GREEN
            } else if invalid {
                0xBE3434
            } else {
                BORDER
            }));
        if !busy && !recording {
            keys = keys
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_shortcut_recording(id, false, window, cx)
                }));
        }
        if recording {
            let mut capture = row()
                .id("shortcut-recording-target")
                .test_support()
                .flex_1()
                .min_w_0()
                .gap_2()
                .flex_wrap()
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .text_color(crate::theme::color(GREEN))
                        .child("请按组合键…"),
                )
                .child(
                    Button::new("cancel-shortcut-recording")
                        .small()
                        .ghost()
                        .label("取消 · Esc")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.preferences_ui.recording = None;
                            this.preferences_ui.recording_error = None;
                            if let Some(search) = &this.preferences_ui.shortcut_search {
                                search.read(cx).focus_handle(cx).focus(window, cx);
                            }
                            cx.notify();
                        })),
                );
            if let Some(focus) = &self.preferences_ui.recording_focus {
                capture = capture.track_focus(focus);
            }
            keys = keys.child(capture);
        } else if binding.trim().is_empty() {
            keys = keys.child(muted("点击设置"));
        } else {
            for (index, alias) in binding.split(';').enumerate() {
                if index > 0 {
                    keys = keys.child(muted(" / "));
                }
                for key in alias.split('+') {
                    keys = keys.child(
                        div()
                            .px_1()
                            .py_0p5()
                            .rounded_sm()
                            .border_1()
                            .border_color(crate::theme::color(BORDER))
                            .bg(crate::theme::color(0xFFFFFF))
                            .text_xs()
                            .text_color(crate::theme::color(if invalid { 0xBE3434 } else { INK }))
                            .child(form::keycap_label(key).to_owned()),
                    );
                }
            }
        }
        let mut entry = column()
            .id(SharedString::from(format!("shortcut-row-{id}")))
            .test_support()
            .gap_1()
            .py_2()
            .border_b_1()
            .border_color(crate::theme::color(BORDER))
            .child(
                row()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        div()
                            .w(px(136.))
                            .text_sm()
                            .text_color(crate::theme::color(INK))
                            .child(action.label),
                    )
                    .child(keys)
                    .child(
                        Button::new(SharedString::from(format!("append-shortcut-{id}")))
                            .small()
                            .ghost()
                            .label("＋")
                            .tooltip("追加快捷键")
                            .disabled(busy || recording)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.start_shortcut_recording(id, true, window, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("clear-shortcut-{id}")))
                            .small()
                            .ghost()
                            .label("×")
                            .tooltip("清空快捷键")
                            .disabled(busy || recording)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.preferences_ui.recording = None;
                                this.preferences_ui.recording_error = None;
                                if let Some(input) = this.preferences_ui.shortcuts.get(id) {
                                    input.update(cx, |input, cx| input.set_value("", window, cx));
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
            entry = entry.child(muted(error.clone()).text_color(crate::theme::color(0xBE3434)));
        }
        if recording && let Some(error) = &self.preferences_ui.recording_error {
            entry = entry.child(
                muted(error.clone())
                    .id("shortcut-recording-error")
                    .test_support()
                    .text_color(crate::theme::color(0xBE3434)),
            );
        }

        column()
            .gap_0()
            .children(show_group.then(|| {
                div()
                    .pt_3()
                    .pb_1()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(action.group)
            }))
            .child(entry)
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
            markdown: Default::default(),
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
        edited.markdown.layout.code_size = 18;
        edited.markdown.layout.table_stripes = false;
        edited
            .markdown
            .colors
            .insert("heading".into(), "#234567".into());
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
