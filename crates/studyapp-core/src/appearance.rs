//! Declarative appearance preferences. No CSS or executable content is accepted.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemePalette {
    pub accent: String,
    pub background: String,
    pub surface: String,
    pub text: String,
}

impl Default for ThemePalette {
    fn default() -> Self {
        Self {
            accent: "#2478D4".into(),
            background: "#F4F5F7".into(),
            surface: "#FFFFFF".into(),
            text: "#24272D".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemePreset {
    pub id: String,
    pub name: String,
    pub palette: ThemePalette,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppearanceSettings {
    pub mode: ThemeMode,
    pub active_preset: Option<String>,
    pub custom_presets: Vec<ThemePreset>,
    pub font_size: u16,
    pub spacing: u16,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            mode: ThemeMode::System,
            active_preset: None,
            custom_presets: vec![],
            font_size: 14,
            spacing: 8,
        }
    }
}

pub fn parse_theme_color(value: &str) -> Option<u32> {
    let hex = value.strip_prefix('#')?;
    (hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

fn luminance(color: u32) -> f64 {
    let channel = |shift: u32| {
        let value = ((color >> shift) & 255u32) as f64 / 255.;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

pub fn theme_contrast(first: u32, second: u32) -> f64 {
    let a = luminance(first);
    let b = luminance(second);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

impl ThemePalette {
    pub fn validate(&self) -> Result<()> {
        let text = parse_theme_color(&self.text);
        let background = parse_theme_color(&self.background);
        let surface = parse_theme_color(&self.surface);
        let accent = parse_theme_color(&self.accent);
        ensure!(
            [text, background, surface, accent]
                .iter()
                .all(Option::is_some),
            "颜色须使用 #RRGGBB 六位十六进制格式"
        );
        if let (Some(text), Some(background), Some(surface), Some(accent)) =
            (text, background, surface, accent)
        {
            ensure!(
                theme_contrast(text, background) >= 4.5 && theme_contrast(text, surface) >= 4.5,
                "文字与背景、卡片的对比度须达到 4.5:1"
            );
            ensure!(
                theme_contrast(accent, surface) >= 3.,
                "强调色与卡片的对比度须达到 3:1"
            );
        }
        Ok(())
    }
}

impl AppearanceSettings {
    pub fn active_palette(&self) -> Option<&ThemePalette> {
        let id = self.active_preset.as_ref()?;
        self.custom_presets
            .iter()
            .find(|preset| &preset.id == id)
            .map(|preset| &preset.palette)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!((12..=22).contains(&self.font_size), "字号须为 12–22 像素");
        ensure!((4..=16).contains(&self.spacing), "间距须为 4–16 像素");
        ensure!(self.custom_presets.len() <= 20, "自定义主题最多 20 个");
        let mut ids = std::collections::BTreeSet::new();
        let mut names = std::collections::BTreeSet::new();
        for preset in &self.custom_presets {
            ensure!(
                !preset.id.is_empty()
                    && preset.id.len() <= 80
                    && preset
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
                "主题标识无效"
            );
            ensure!(ids.insert(&preset.id), "主题标识不能重复");
            let name = preset.name.trim();
            ensure!(
                !name.is_empty()
                    && name.chars().count() <= 40
                    && !name.chars().any(char::is_control),
                "主题名称须为 1–40 个可显示字符"
            );
            ensure!(names.insert(name.to_lowercase()), "主题名称不能重复");
            preset.palette.validate()?;
        }
        if let Some(id) = &self.active_preset {
            ensure!(ids.contains(id), "所选自定义主题不存在");
        }
        ensure!(
            self.mode != ThemeMode::Custom || self.active_palette().is_some(),
            "请选择自定义主题"
        );
        Ok(())
    }
}
