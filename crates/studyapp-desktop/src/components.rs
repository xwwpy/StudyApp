use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;

/// Shared bounded scroll surface. The original ID remains the actual viewport,
/// so wheel containment, reveal and native clipping use the same rectangle.
pub fn scroll_body(id: impl Into<ElementId>, handle: &ScrollHandle) -> Stateful<Div> {
    column()
        .id(id)
        .relative()
        .min_h_0()
        .min_w_0()
        .overflow_y_scroll()
        .track_scroll(handle)
        .vertical_scrollbar(handle)
}

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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.weak_entity();
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
                    .border_color(crate::theme::color(BORDER))
                    .bg(crate::theme::color(0xFFFFFF))
                    .text_color(crate::theme::color(INK))
                    .text_sm()
                    .shadow_md()
                    .child(self.text.clone())
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |_, _, window, _| {
                                let owner = owner.clone();
                                window.on_mouse_event::<MouseMoveEvent>(
                                    move |event, phase, _, cx| {
                                        if phase == DispatchPhase::Capture {
                                            let _ = owner.update(cx, |this, cx| {
                                                let next = event.position + point(px(-8.), px(16.));
                                                if this.position != next {
                                                    this.position = next;
                                                    cx.notify();
                                                }
                                            });
                                        }
                                    },
                                );
                            },
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
    }
}

pub const LIBRARY_ROW_HEIGHT: f32 = 32.;

pub fn library_entry() -> Div {
    row()
        .h(px(LIBRARY_ROW_HEIGHT))
        .flex_shrink_0()
        .min_w_0()
        .gap_2()
        .px_2()
        .rounded_md()
        .text_sm()
}

/// Secondary library metadata stays visually separate from study-change badges.
pub fn library_tag(label: &'static str, warning: bool) -> Div {
    div()
        .flex_shrink_0()
        .px_1()
        .rounded_sm()
        .text_xs()
        .bg(crate::theme::color(BG))
        .text_color(crate::theme::color(if warning { 0x9B5719 } else { MUTED }))
        .child(label)
}

/// A compact drag representation sharing the selectable row's height.
pub fn library_drag_card(
    id: &'static str,
    title: String,
    icon: impl IntoElement,
) -> impl IntoElement {
    row()
        .id(id)
        .test_support()
        .w(px(220.))
        .h(px(LIBRARY_ROW_HEIGHT))
        .min_w_0()
        .px_2()
        .gap_2()
        .rounded_md()
        .border_1()
        .border_color(crate::theme::color(BORDER))
        .bg(crate::theme::color(0xFFFFFF))
        .text_color(crate::theme::color(INK))
        .text_sm()
        .shadow_sm()
        .child(icon)
        .child(div().flex_1().min_w_0().truncate().child(title))
}

pub fn column() -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(crate::theme::palette().spacing as f32))
}
pub fn row() -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(crate::theme::palette().spacing as f32))
}
pub fn card() -> Div {
    column()
        .flex_shrink_0()
        .p_4()
        .rounded_xl()
        .bg(crate::theme::color(0xFFFFFF))
        .border_1()
        .border_color(crate::theme::color(BORDER))
}
pub fn muted(text: impl Into<SharedString>) -> Div {
    div()
        .text_sm()
        .text_color(crate::theme::color(MUTED))
        .child(text.into())
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
        .bg(crate::theme::color(color))
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

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::{SidebarTooltip, card, column, scroll_body};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Context, InteractiveElement, IntoElement, MouseMoveEvent, ParentElement,
        Render, ScrollHandle, Styled, TestAppContext, TestSupportExt, Window, div, point, px,
    };

    struct ScrollCards {
        scroll: ScrollHandle,
    }

    impl Render for ScrollCards {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            column()
                .w(px(400.))
                .h(px(280.))
                .gap_0()
                .child(
                    div()
                        .id("fixed-header")
                        .test_support()
                        .h(px(32.))
                        .flex_shrink_0(),
                )
                .child(
                    scroll_body("cards-viewport", &self.scroll)
                        .test_support()
                        .flex_1()
                        .gap_2()
                        .children((0_usize..5).map(|i| {
                            card()
                                .id(("whole-card", i))
                                .test_support()
                                .child(div().h(px(48.)).flex_shrink_0())
                                .child(div().h(px(48.)).flex_shrink_0())
                        })),
                )
                .child(
                    div()
                        .id("fixed-footer")
                        .test_support()
                        .h(px(32.))
                        .flex_shrink_0(),
                )
        }
    }

    #[gpui_kit::test]
    fn shared_scroll_keeps_cards_whole_and_footer_outside_viewport(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let scroll = ScrollHandle::new();
        let (_, cx) = cx.add_window_view(|_, _| ScrollCards {
            scroll: scroll.clone(),
        });
        cx.update(|window, cx| window.render_frame(cx));
        let (header, footer, card_height) = cx.update(|window, _| {
            let header = window.find("fixed-header").bounds();
            let footer = window.find("fixed-footer").bounds();
            let viewport = window.find("cards-viewport").bounds();
            let first = window.find(("whole-card", 0_usize)).bounds();
            assert!(
                first.size.height >= px(128.),
                "card contents were flex-compressed"
            );
            assert_eq!(viewport.top(), header.bottom());
            assert_eq!(viewport.bottom(), footer.top());
            (header, footer, first.size.height)
        });
        assert!(scroll.max_offset().y > px(400.));
        scroll.set_offset(point(px(0.), -scroll.max_offset().y));
        cx.update(|window, cx| window.render_frame(cx));
        cx.update(|window, _| {
            assert_eq!(window.find("fixed-header").bounds(), header);
            assert_eq!(window.find("fixed-footer").bounds(), footer);
            let last = window.find(("whole-card", 4_usize)).bounds();
            assert_eq!(last.size.height, card_height);
            assert!(
                last.bottom() <= footer.top() + px(1.),
                "last card cannot be fully revealed"
            );
        });
    }

    #[gpui_kit::test]
    fn pointer_tooltip_follows_pointer_after_it_is_open(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        struct TooltipSurface(gpui_kit::Entity<SidebarTooltip>);
        impl Render for TooltipSurface {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().size_full().relative().child(self.0.clone())
            }
        }
        let (surface, cx) = cx.add_window_view(|_, cx| {
            TooltipSurface(cx.new(|_| SidebarTooltip {
                text: "最近学习 / 上次复习 / 下次复习".into(),
                position: point(px(320.), px(120.)),
            }))
        });
        let tooltip = cx.update(|_, cx| surface.read(cx).0.clone());
        cx.update(|window, cx| window.render_frame(cx));
        for position in [point(px(420.), px(160.)), point(px(450.), px(190.))] {
            cx.simulate_event(MouseMoveEvent {
                position,
                ..Default::default()
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.render_frame(cx);
                assert_eq!(
                    tooltip.read(cx).position,
                    position + point(px(-8.), px(16.))
                );
                let bounds = window.find("sidebar-tooltip-content").bounds();
                assert_eq!(bounds.right(), position.x - px(8.));
                assert_eq!(bounds.top(), position.y + px(16.));
            });
        }
    }
}
