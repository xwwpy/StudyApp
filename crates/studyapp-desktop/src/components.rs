use gpui_kit::*;

pub const INK: u32 = 0x233631;
pub const MUTED: u32 = 0x74827C;
pub const GREEN: u32 = 0x187B5B;
pub const LIGHT: u32 = 0xEAF4ED;
pub const BORDER: u32 = 0xE3E9E3;
pub const BG: u32 = 0xF5F7F3;

/// Native children paint above GPUI. Place sidebar tooltips to the pointer's
/// left in window coordinates, keeping the editor's pixels unobscured.
pub fn sidebar_tooltip(text: String, window: &mut Window, cx: &mut App) -> AnyView {
    let position = window.mouse_position() + point(px(-8.), px(16.));
    cx.new(|_| SidebarTooltip { text, position }).into()
}

struct SidebarTooltip {
    text: String,
    position: Point<Pixels>,
}

impl Render for SidebarTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        anchored()
            .position(self.position)
            .anchor(Anchor::TopRight)
            .snap_to_window()
            .child(
                div()
                    .id("sidebar-tooltip-content")
                    .test_support()
                    .w(px(220.))
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(0xFFFFFF))
                    .text_color(rgb(INK))
                    .text_sm()
                    .shadow_md()
                    .child(self.text.clone()),
            )
    }
}

pub fn column() -> Div {
    div().flex().flex_col().gap_4()
}
pub fn row() -> Div {
    div().flex().items_center().gap_3()
}
pub fn card() -> Div {
    column()
        .p_5()
        .rounded_xl()
        .bg(rgb(0xFFFFFF))
        .border_1()
        .border_color(rgb(BORDER))
}
pub fn muted(text: impl Into<SharedString>) -> Div {
    div().text_sm().text_color(rgb(MUTED)).child(text.into())
}
pub fn heading(text: impl Into<SharedString>) -> Div {
    div()
        .text_lg()
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}
pub fn pill(text: impl Into<SharedString>, color: u32) -> Div {
    div()
        .px_3()
        .py_1()
        .rounded_full()
        .text_xs()
        .bg(rgb(color))
        .child(text.into())
}
pub fn empty(title: &str, detail: &str) -> Div {
    card()
        .items_center()
        .justify_center()
        .py_12()
        .child(heading(title.to_owned()))
        .child(muted(detail.to_owned()))
}
pub fn metric(label: &str, value: String, detail: &str) -> Div {
    card()
        .flex_1()
        .child(muted(label.to_owned()))
        .child(
            div()
                .text_3xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child(value),
        )
        .child(muted(detail.to_owned()))
}
