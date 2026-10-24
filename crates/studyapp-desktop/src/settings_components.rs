//! Shared neutral form surfaces, labels and inputs for the settings workspace.
use crate::components::*;
use gpui_kit::{
    component::{
        Sizable,
        input::{Input, InputState},
    },
    *,
};

pub fn section(title: impl Into<SharedString>) -> Div {
    column()
        .min_w_0()
        .flex_shrink_0()
        .gap_3()
        .p_4()
        .rounded_xl()
        .border_1()
        .border_color(crate::theme::color(BORDER))
        .bg(crate::theme::color(0xFFFFFF))
        .child(heading(title).text_base())
}

pub fn field(label: impl Into<SharedString>, control: impl IntoElement) -> Div {
    column()
        .min_w_0()
        .gap_1()
        .child(
            div()
                .text_sm()
                .text_color(crate::theme::color(INK))
                .child(label.into()),
        )
        .child(control)
}

pub fn input(state: &Entity<InputState>, disabled: bool) -> Input {
    Input::new(state)
        .small()
        .h(px(32.))
        .disabled(disabled)
        .rounded_md()
        .border_color(crate::theme::color(BORDER))
        .bg(crate::theme::color(BG))
}

pub fn binding_cell() -> Div {
    row()
        .min_w_0()
        .min_h(px(34.))
        .px_2()
        .py_1()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(crate::theme::color(BORDER))
        .bg(crate::theme::color(BG))
}

/// Display familiar keycap glyphs without changing stored/recorded key tokens.
pub fn keycap_label(key: &str) -> &str {
    match key {
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "Backslash" => "\\",
        "Semicolon" => ";",
        "Quote" => "'",
        "Backquote" => "`",
        "Minus" => "−",
        "Equal" => "=",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Escape" => "Esc",
        "Enter" => "↵",
        "Backspace" => "⌫",
        "PageUp" => "PgUp",
        "PageDown" => "PgDn",
        "Delete" => "Del",
        "Insert" => "Ins",
        other => other,
    }
}
