use crate::{app::StudyApp, components::*};
use gpui_kit::{
    base::{Dialog, DialogPopup},
    *,
};

/// Retains one focus scope across renders. Focus changes only when a blocking
/// dialog opens or closes; result notifications never enter this scope.
pub struct ModalFocus {
    handle: FocusHandle,
    previous: Option<WeakFocusHandle>,
    active: bool,
    scroll: ScrollHandle,
}

impl ModalFocus {
    pub fn new(cx: &mut App) -> Self {
        Self {
            handle: cx.focus_handle(),
            previous: None,
            active: false,
            scroll: ScrollHandle::new(),
        }
    }

    pub fn handle(&self) -> &FocusHandle {
        &self.handle
    }

    #[cfg(all(test, feature = "ui-test"))]
    pub fn sync(&mut self, active: bool, window: &mut Window, cx: &mut App) {
        self.sync_with_initial(active, None, window, cx);
    }

    pub fn sync_with_initial(
        &mut self,
        active: bool,
        initial: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if active == self.active {
            // A menu or asynchronous input callback may restore an older
            // focus target after the dialog opens. Reclaim only escaped focus,
            // keeping every input and action already inside the dialog intact.
            if active && !self.handle.contains_focused(window, cx) {
                initial.as_ref().unwrap_or(&self.handle).focus(window, cx);
            }
            return;
        }
        self.active = active;
        if active {
            self.previous = window.focused(cx).map(|handle| handle.downgrade());
            initial.as_ref().unwrap_or(&self.handle).focus(window, cx);
        } else if let Some(previous) = self.previous.take().and_then(|handle| handle.upgrade()) {
            previous.focus(window, cx);
        }
    }
}

/// Uses the base dialog that is hosted by gpui-kit::open_window's Base Root.
/// Its focus trap and occluding popup provide keyboard and pointer containment.
pub fn modal_shell(
    content: impl IntoElement,
    focus: &ModalFocus,
    window: &Window,
    cx: &mut App,
) -> Dialog {
    let viewport = window.viewport_size();
    Dialog::new(cx)
        .focus_handle(focus.handle().clone())
        .close_on_backdrop_press(false)
        .backdrop(
            div()
                .size_full()
                .occlude()
                .bg(rgba(0x14251F66))
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
        )
        .popup(
            DialogPopup::new()
                .w(px(560.).min((viewport.width - px(48.)).max(px(240.))))
                .p_5()
                .rounded_xl()
                .shadow_lg()
                .border_1()
                .border_color(crate::theme::color(BORDER))
                .bg(crate::theme::color(0xFFFFFF))
                .text_color(crate::theme::color(INK))
                .child(
                    scroll_body("modal-content-scroll", &focus.scroll)
                        .test_support()
                        .max_h((viewport.height - px(112.)).max(px(120.)))
                        .overflow_y_scroll()
                        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                        .child(content),
                ),
        )
}

impl StudyApp {
    pub fn push_notice(&mut self, message: Result<String, String>, cx: &mut Context<Self>) {
        // Retain the latest outcome for diagnostic assertions; event delivery
        // goes to the queue, never through text equality or render timing.
        self.notice = Some(message.clone());
        self.notifications
            .update(cx, |center, cx| center.push(message, cx));
    }

    #[cfg(feature = "ui-test")]
    pub fn clear_notices(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        self.notifications.update(cx, |center, cx| center.clear(cx));
    }
}

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::{ModalFocus, modal_shell};
    use crate::components::row;
    use crate::controls::Button;
    use gpui_kit::{
        AppContext, Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render,
        Styled, TestAppContext, Window, WindowOptions, base::Root, div, test::TestWindowExt,
    };

    struct ModalFixture {
        focus: ModalFocus,
        open: bool,
        background_clicks: usize,
        outside: FocusHandle,
    }

    impl Render for ModalFixture {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.focus.sync(self.open, window, cx);
            let cancel = cx.weak_entity();
            div()
                .size_full()
                .child(
                    div().track_focus(&self.outside).child(
                        Button::new("background-action")
                            .label("Background")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.background_clicks += 1;
                                cx.notify();
                            })),
                    ),
                )
                .children(self.open.then(|| {
                    modal_shell(
                        row()
                            .child(Button::new("modal-first").label("First"))
                            .child(Button::new("modal-last").label("Last")),
                        &self.focus,
                        window,
                        cx,
                    )
                    .on_cancel(move |_, _, cx| {
                        let _ = cancel.update(cx, |this, cx| {
                            this.open = false;
                            cx.notify();
                        });
                        true
                    })
                }))
        }
    }

    #[gpui_kit::test]
    fn dialog_contains_tab_focus_blocks_background_and_restores_focus_on_escape(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let (handle, view) = cx.update(|cx| {
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|cx| ModalFixture {
                    focus: ModalFocus::new(cx),
                    open: false,
                    background_clicks: 0,
                    outside: cx.focus_handle(),
                })
            })
            .unwrap()
        });
        let handle = handle.downcast::<Root>().unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.click("background-action", cx);
            // Component buttons intentionally prevent focus on mouse down.
            // Establish a real prior focus target instead of assuming a click
            // focused the button, then verify that exact target is restored.
            let outside = view.read(cx).outside.clone();
            outside.focus(window, cx);
        })
        .unwrap();
        cx.run_until_parked();
        view.update(cx, |this, cx| {
            this.open = true;
            cx.notify();
        });
        cx.run_until_parked();
        for key in ["tab", "tab", "tab", "shift-tab", "shift-tab", "shift-tab"] {
            cx.update_window(handle.into(), |_, window, cx| {
                window.press(key, cx);
                window.render_frame(cx);
                assert!(
                    view.read(cx).focus.handle().contains_focused(window, cx),
                    "focus escaped after {key}"
                );
            })
            .unwrap();
            cx.run_until_parked();
        }
        cx.update_window(handle.into(), |_, window, cx| {
            let outside = view.read(cx).outside.clone();
            outside.focus(window, cx);
            window.render_frame(cx);
            assert!(
                view.read(cx).focus.handle().contains_focused(window, cx),
                "deferred menu focus restoration escaped the active dialog"
            );
        })
        .unwrap();
        cx.update_window(handle.into(), |_, window, cx| {
            window.click("background-action", cx);
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(
            cx.read(|cx| view.read(cx).background_clicks),
            1,
            "backdrop allowed background activation"
        );
        cx.update_window(handle.into(), |_, window, cx| {
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(!view.read(cx).open);
            assert!(view.read(cx).outside.is_focused(window));
        })
        .unwrap();
    }
}
