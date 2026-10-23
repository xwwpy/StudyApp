//! Independently invalidated notification surface; animation never notifies StudyApp.
use crate::{components::*, controls::Button, notice_queue::NoticeQueue};
use gpui_kit::{
    component::{IconName, Sizable, button::ButtonVariants},
    *,
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

/// Actual card rectangles plus shadow, in the parent window's logical pixels.
#[derive(Clone, Debug)]
pub struct NotificationOcclusions(pub Vec<Bounds<Pixels>>);

pub struct NotificationCenter {
    queue: NoticeQueue,
    epoch: Instant,
    timer: Option<Task<()>>,
    occlusions: Vec<Bounds<Pixels>>,
}

impl EventEmitter<NotificationOcclusions> for NotificationCenter {}

impl NotificationCenter {
    pub fn new() -> Self {
        Self {
            queue: NoticeQueue::default(),
            epoch: Instant::now(),
            timer: None,
            occlusions: Vec::new(),
        }
    }

    pub fn push(&mut self, message: Result<String, String>, cx: &mut Context<Self>) {
        self.queue.push(message, self.epoch.elapsed());
        if self.timer.is_none() {
            self.timer = Some(cx.spawn(async move |owner, cx| {
                loop {
                    let delay = owner
                        .update(cx, |this, _| {
                            if this.queue.animating(this.epoch.elapsed()) {
                                16
                            } else {
                                100
                            }
                        })
                        .unwrap_or(100);
                    cx.background_executor()
                        .timer(Duration::from_millis(delay))
                        .await;
                    let keep = owner
                        .update(cx, |this, cx| {
                            if this.queue.tick(this.epoch.elapsed()) {
                                cx.notify();
                            }
                            if this.queue.is_empty() {
                                this.timer = None;
                                false
                            } else {
                                true
                            }
                        })
                        .unwrap_or(false);
                    if !keep {
                        break;
                    }
                }
            }));
        }
        cx.notify();
    }

    #[cfg(feature = "ui-test")]
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.queue.clear();
        cx.notify();
    }

    #[cfg(feature = "ui-test")]
    pub fn visible_count(&self) -> usize {
        self.queue.visible().count()
    }

    #[cfg(feature = "ui-test")]
    pub fn test_state(&self) -> Vec<(u64, f32)> {
        self.queue
            .visible()
            .map(|entry| (entry.id, entry.opacity(self.epoch.elapsed())))
            .collect()
    }
}

impl Render for NotificationCenter {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = self.epoch.elapsed();
        let palette = crate::theme::palette();
        let available = (window.viewport_size().width - px(32.)).max(px(0.));
        let font_size = px((palette.font_size as f32 - 1.).max(12.));
        let font = Font {
            family: "Microsoft YaHei UI".into(),
            ..Default::default()
        };
        let frame_bounds = Rc::new(RefCell::new(Vec::<Bounds<Pixels>>::new()));
        let weak = cx.weak_entity();
        column()
            .id("floating-notice")
            .test_support()
            .absolute()
            // Reserve the application and library/editor action rows. Long
            // notices still grow with their content without covering menus.
            .top(px(144.))
            .left_0()
            .w_full()
            .items_center()
            .gap(px(8.))
            .children(self.queue.visible().enumerate().map(|(index, notice)| {
                let id = notice.id;
                let (message, accent, tint, icon) = match &notice.message {
                    Ok(message) => (message, palette.accent, palette.accent_soft, "✓"),
                    Err(message) => (message, palette.danger, palette.background, "!"),
                };
                let opacity = notice.opacity(now);
                let text_width = message
                    .lines()
                    .map(|line| {
                        window
                            .text_system()
                            .shape_line(
                                line.to_owned().into(),
                                font_size,
                                &[TextRun {
                                    len: line.len(),
                                    font: font.clone(),
                                    color: rgb(palette.text).into(),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                }],
                                None,
                            )
                            .width
                    })
                    .fold(px(0.), Pixels::max);
                let width = (text_width + px(86.))
                    .max(px(180.))
                    .min(px(560.))
                    .min(available);
                let card_bounds = frame_bounds.clone();
                row()
                    .id(SharedString::from(format!("notice-{id}")))
                    .test_support()
                    .occlude()
                    .w(width)
                    .min_h(px(40.))
                    .px_2()
                    .py(px(8.))
                    .gap_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.surface))
                    .shadow_md()
                    .min_w_0()
                    .opacity(opacity)
                    .relative()
                    .top(px(-6. * (1. - opacity)))
                    .child(
                        div()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(22.))
                            .rounded_full()
                            .bg(rgb(tint))
                            .text_color(rgb(accent))
                            .child(icon),
                    )
                    .child(
                        div()
                            .id(if index == 0 {
                                SharedString::from("notice-message-scroll")
                            } else {
                                SharedString::from(format!("notice-message-{id}"))
                            })
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .max_h(px(164.))
                            .overflow_y_scroll()
                            .text_size(font_size)
                            .line_height(px(20.))
                            .text_color(rgb(palette.text))
                            .child(message.clone())
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
                    )
                    .child(
                        Button::new(if index == 0 {
                            SharedString::from("dismiss-notice")
                        } else {
                            SharedString::from(format!("dismiss-notice-{id}"))
                        })
                        .small()
                        .ghost()
                        .icon(IconName::Close)
                        .accessibility_label("关闭通知")
                        .tooltip("关闭通知")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.queue.dismiss(id, this.epoch.elapsed());
                            cx.notify();
                        })),
                    )
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        this.queue.hover(id, *hovered, this.epoch.elapsed());
                        cx.notify();
                    }))
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                // Reserve the real visual footprint, not the entire
                                // centered overlay row, so the rest of the editor remains
                                // visible and receives native pointer events.
                                card_bounds.borrow_mut().push(Bounds::new(
                                    point(bounds.origin.x - px(10.), bounds.origin.y - px(10.)),
                                    size(bounds.size.width + px(20.), bounds.size.height + px(20.)),
                                ));
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
            }))
            .child(
                canvas(
                    move |_, _, cx| {
                        let bounds = frame_bounds.borrow().clone();
                        let _ = weak.update(cx, |this, cx| {
                            if this.occlusions != bounds {
                                this.occlusions = bounds.clone();
                                cx.emit(NotificationOcclusions(bounds));
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size(px(0.)),
            )
    }
}
