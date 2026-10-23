//! Shared native/component/editor appearance resolved only from safe persisted tokens.
use gpui_kit::{
    component::{Theme, ThemeMode as ComponentMode},
    *,
};
use std::cell::Cell;
use studyapp_core::{AppearanceSettings, ThemeMode, parse_theme_color, theme_contrast};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedTheme {
    pub dark: bool,
    pub background: u32,
    pub surface: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub accent_text: u32,
    pub accent_soft: u32,
    pub border: u32,
    pub danger: u32,
    pub warning: u32,
    pub font_size: u16,
    pub spacing: u16,
}

fn blend(first: u32, second: u32, weight: f32) -> u32 {
    let channel = |shift: u32| {
        (((first >> shift) & 255u32) as f32 * (1. - weight)
            + ((second >> shift) & 255u32) as f32 * weight)
            .round() as u32
    };
    channel(16) << 16 | channel(8) << 8 | channel(0)
}

pub fn resolve(settings: &AppearanceSettings, system_dark: bool) -> ResolvedTheme {
    let mut dark = match settings.mode {
        ThemeMode::System => system_dark,
        ThemeMode::Dark => true,
        _ => false,
    };
    let (mut background, mut surface, mut text, mut accent) = if dark {
        (0x1D1F23, 0x272A30, 0xEFF0F3, 0x70ACF7)
    } else {
        (0xF4F5F7, 0xFFFFFF, 0x24272D, 0x2478D4)
    };
    if settings.mode == ThemeMode::Custom
        && let Some(custom) = settings
            .active_palette()
            .filter(|custom| custom.validate().is_ok())
    {
        background = parse_theme_color(&custom.background).unwrap_or(background);
        surface = parse_theme_color(&custom.surface).unwrap_or(surface);
        text = parse_theme_color(&custom.text).unwrap_or(text);
        accent = parse_theme_color(&custom.accent).unwrap_or(accent);
        dark = theme_contrast(background, 0xFFFFFF) > theme_contrast(background, 0x000000);
    }
    ResolvedTheme {
        dark,
        background,
        surface,
        text,
        accent,
        muted: blend(text, surface, 0.38),
        accent_text: if theme_contrast(accent, 0xFFFFFF) >= theme_contrast(accent, 0x000000) {
            0xFFFFFF
        } else {
            0x000000
        },
        accent_soft: blend(surface, accent, if dark { 0.22 } else { 0.10 }),
        border: blend(surface, text, 0.14),
        danger: if dark { 0xFF9393 } else { 0xBE3434 },
        warning: if dark { 0xF3BE79 } else { 0x9B5719 },
        font_size: settings.font_size.clamp(12, 22),
        spacing: settings.spacing.clamp(4, 16),
    }
}

thread_local! { static PALETTE: Cell<ResolvedTheme> = Cell::new(resolve(&AppearanceSettings::default(), false)); }
pub fn palette() -> ResolvedTheme {
    PALETTE.with(Cell::get)
}

/// Explicit semantic conversion for legacy native palette tokens. Raw accent
/// foreground is available as palette().accent_text, preserving custom contrast.
pub fn color(token: u32) -> Rgba {
    let p = palette();
    rgb(match token {
        0x233631 | 0x24272D => p.text,
        0x74827C | 0x6B7280 => p.muted,
        0x187B5B | 0x2478D4 => p.accent,
        0xEAF4ED | 0xEDF6EF | 0xF0F7F2 => p.accent_soft,
        0xE3E9E3 | 0xDDE5DF => p.border,
        0xF5F7F3 | 0xF4F5F7 => p.background,
        0xFFFFFF | 0xFAFBF9 => p.surface,
        0xBE3434 | 0xB83A3A | 0xB42318 => p.danger,
        0xA34829 | 0xA56618 | 0x9B5719 => p.warning,
        other => other,
    })
}

impl ResolvedTheme {
    pub fn editor_tokens(self) -> serde_json::Value {
        let hex = |color| format!("#{color:06X}");
        serde_json::json!({
            "mode": if self.dark { "dark" } else { "light" },
            "accent": hex(self.accent), "background": hex(self.background),
            "surface": hex(self.surface), "text": hex(self.text),
            "codeBackground": hex(self.surface), "codeText": hex(self.text),
            "muted": hex(self.muted), "border": hex(self.border),
            "selection": hex(self.accent_soft), "fontSize": self.font_size,
            "spacing": 1.3 + (self.spacing as f64 - 4.) / 12. * 1.1,
        })
    }
}

pub fn system_is_dark(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

pub fn apply(settings: &AppearanceSettings, system_dark: bool, cx: &mut App) -> ResolvedTheme {
    let p = resolve(settings, system_dark);
    PALETTE.with(|palette| palette.set(p));
    Theme::change(
        if p.dark {
            ComponentMode::Dark
        } else {
            ComponentMode::Light
        },
        None,
        cx,
    );
    Theme::update(cx, |theme| {
        theme.font_family = "Microsoft YaHei UI".into();
        theme.font_size = px(p.font_size as f32);
        let c = &mut theme.colors;
        c.background = rgb(p.background).into();
        c.foreground = rgb(p.text).into();
        c.border = rgb(p.border).into();
        c.input = rgb(p.border).into();
        c.primary = rgb(p.accent).into();
        c.primary_hover = rgb(blend(p.accent, p.text, 0.08)).into();
        c.primary_active = rgb(blend(p.accent, p.text, 0.16)).into();
        c.primary_foreground = rgb(p.accent_text).into();
        c.button_primary = c.primary;
        c.button_primary_hover = c.primary_hover;
        c.button_primary_active = c.primary_active;
        c.button_primary_foreground = c.primary_foreground;
        c.button = rgb(p.surface).into();
        c.button_foreground = rgb(p.text).into();
        c.button_hover = rgb(p.accent_soft).into();
        c.button_active = c.button_hover;
        c.secondary = rgb(p.surface).into();
        c.secondary_foreground = rgb(p.text).into();
        c.secondary_hover = rgb(p.accent_soft).into();
        c.secondary_active = c.secondary_hover;
        c.accent = rgb(p.accent_soft).into();
        c.accent_foreground = rgb(p.text).into();
        c.muted = rgb(p.background).into();
        c.muted_foreground = rgb(p.muted).into();
        c.ring = rgb(p.accent).into();
        c.caret = rgb(p.accent).into();
        c.selection = rgb(p.accent_soft).into();
        c.popover = rgb(p.surface).into();
        c.popover_foreground = rgb(p.text).into();
        c.list = rgb(p.surface).into();
        c.list_hover = rgb(p.accent_soft).into();
        c.list_active = c.list_hover;
        c.list_active_border = rgb(p.border).into();
        c.sidebar = rgb(p.background).into();
        c.sidebar_foreground = rgb(p.text).into();
        c.sidebar_border = rgb(p.border).into();
        c.sidebar_accent = rgb(p.accent_soft).into();
        c.sidebar_accent_foreground = rgb(p.text).into();
        c.tab = rgb(p.background).into();
        c.tab_active = rgb(p.surface).into();
        c.danger = rgb(p.danger).into();
        c.link = rgb(p.accent).into();
        c.link_hover = c.link;
        c.group_box = rgb(p.surface).into();
        c.group_box_foreground = rgb(p.text).into();
    });
    p
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use studyapp_core::{AppearanceSettings, ThemeMode};
    #[test]
    fn system_resolution_and_editor_tokens_follow_same_safe_palette() {
        let settings = AppearanceSettings::default();
        assert!(!resolve(&settings, false).dark);
        let dark = resolve(&settings, true);
        assert!(dark.dark);
        let tokens = dark.editor_tokens();
        assert_eq!(tokens["mode"], "dark");
        assert_eq!(tokens["surface"], "#272A30");
        assert_eq!(tokens["codeBackground"], "#272A30");
        assert_eq!(tokens["codeText"], "#EFF0F3");
        assert!(tokens["spacing"].as_f64().unwrap() >= 1.3);
    }
    #[test]
    fn invalid_custom_palette_resolves_without_panic_or_executable_tokens() {
        let mut settings = AppearanceSettings {
            mode: ThemeMode::Custom,
            active_preset: Some("bad".into()),
            ..Default::default()
        };
        settings.custom_presets.push(studyapp_core::ThemePreset {
            id: "bad".into(),
            name: "bad".into(),
            palette: studyapp_core::ThemePalette {
                text: "url(x)".into(),
                ..Default::default()
            },
        });
        assert_eq!(resolve(&settings, false).text, 0x24272D);
        assert!(
            !resolve(&settings, false)
                .editor_tokens()
                .to_string()
                .contains("url(")
        );
    }
}
