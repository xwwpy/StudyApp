//! Shared neutral form surfaces, labels and inputs for the settings workspace.
use crate::components::*;
use gpui_kit::{
    component::{
        Sizable,
        input::{Input, InputState},
        scroll::ScrollableElement,
    },
    *,
};
use std::cell::RefCell;

/// Keeps variable-height form rows virtualized without resetting the user's
/// scroll position when a binding, conflict, or inline recorder changes height.
pub struct VirtualRows<K> {
    pub scroll: ListState,
    source: RefCell<Option<(Vec<usize>, K)>>,
    focused: RefCell<Vec<(usize, FocusHandle)>>,
}

impl<K> Default for VirtualRows<K> {
    fn default() -> Self {
        Self {
            scroll: ListState::new(0, ListAlignment::Top, px(48.)),
            source: RefCell::new(None),
            focused: RefCell::new(Vec::new()),
        }
    }
}

impl<K: PartialEq> VirtualRows<K> {
    pub fn sync(&self, rows: &[usize], layout: K) {
        self.sync_with_height(rows, layout, px(58.));
    }

    pub fn sync_with_height(&self, rows: &[usize], layout: K, estimate: Pixels) {
        let mut source = self.source.borrow_mut();
        match source.as_ref() {
            Some((previous_rows, previous_layout)) if previous_rows == rows => {
                if previous_layout == &layout {
                    return;
                }
                self.scroll.remeasure();
            }
            _ => {
                self.scroll.reset_with_uniform_height(rows.len(), estimate);
                self.focused.borrow_mut().clear();
            }
        }
        *source = Some((rows.to_vec(), layout));
    }

    /// Keep the one active inline recorder in the keyboard dispatch tree when
    /// the user scrolls it off screen. Other offscreen rows remain unrendered.
    pub fn track_focus(&self, next: Option<(usize, FocusHandle)>) {
        self.track_focuses(next.into_iter().collect());
    }

    pub fn track_focuses(&self, next: Vec<(usize, FocusHandle)>) {
        let mut focused = self.focused.borrow_mut();
        if *focused == next {
            return;
        }
        for (row, _) in focused
            .iter()
            .filter(|(row, _)| !next.iter().any(|(next, _)| next == row))
        {
            self.scroll.set_item_focus_handle(*row, None);
        }
        for (row, handle) in &next {
            if !focused.iter().any(|entry| entry == &(*row, handle.clone())) {
                self.scroll
                    .set_item_focus_handle(*row, Some(handle.clone()));
            }
        }
        *focused = next;
    }

    pub fn scroll_to_top(&self) {
        self.scroll.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.),
        });
    }
}

pub fn virtual_rows(
    state: &ListState,
    mut render: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
) -> impl IntoElement {
    div()
        .id("settings-section-scroll")
        .test_support()
        .relative()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .overflow_hidden()
        .rounded_xl()
        .border_1()
        .border_color(crate::theme::color(BORDER))
        .bg(crate::theme::color(0xFFFFFF))
        .vertical_scrollbar(state)
        // The list owns this complete viewport, including its left padding,
        // so wheels over the gutter do not escape to an outer scroll surface.
        .child(
            list(state.clone(), move |row, window, cx| {
                div()
                    .px_4()
                    .child(render(row, window, cx))
                    .into_any_element()
            })
            .size_full()
            .pb_4(),
        )
}

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

#[cfg(all(test, feature = "ui-test"))]
mod tests {
    use super::{VirtualRows, virtual_rows};
    use gpui_kit::{
        AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, ListOffset,
        ParentElement, Render, ScrollDelta, ScrollWheelEvent, StyleRefinement, Styled,
        TestAppContext, VisualTestContext, WeakEntity, Window, div, point, px,
    };
    use std::cell::RefCell;
    use std::{cell::Cell, collections::BTreeSet, rc::Rc};

    struct FormFixture {
        rows: VirtualRows<u64>,
        ids: Vec<usize>,
        revision: u64,
        tall: Rc<Cell<bool>>,
        rendered: Rc<RefCell<BTreeSet<usize>>>,
        width: f32,
        focus: FocusHandle,
        recording: bool,
    }

    impl Render for FormFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.rows.sync(&self.ids, self.revision);
            self.rows.track_focus(
                self.recording
                    .then(|| {
                        self.ids
                            .iter()
                            .position(|&id| id == 30)
                            .map(|row| (row, self.focus.clone()))
                    })
                    .flatten(),
            );
            let ids = self.ids.clone();
            let rendered = self.rendered.clone();
            let tall = self.tall.clone();
            let focus = self.focus.clone();
            div()
                .flex()
                .flex_col()
                .w(px(self.width))
                .h(px(300.))
                .child(virtual_rows(&self.rows.scroll, move |ix, _, _| {
                    let id = ids[ix];
                    rendered.borrow_mut().insert(id);
                    let mut row = div()
                        .h(px(if id == 30 && tall.get() { 140. } else { 58. }))
                        .child(format!("Shortcut {id}"));
                    if id == 30 {
                        row = row.track_focus(&focus);
                    }
                    row.into_any_element()
                }))
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            _ = window.draw(cx);
        });
    }

    #[gpui_kit::test]
    fn settings_rows_bound_render_work_and_preserve_variable_height_scroll(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let rendered = Rc::new(RefCell::new(BTreeSet::new()));
        let tall = Rc::new(Cell::new(false));
        let (view, cx) = cx.add_window_view(|_, cx| FormFixture {
            rows: VirtualRows::default(),
            ids: (0..50).collect(),
            revision: 0,
            tall: tall.clone(),
            rendered: rendered.clone(),
            width: 500.,
            focus: cx.focus_handle(),
            recording: false,
        });
        draw(cx);
        assert!(
            rendered.borrow().len() < 12,
            "offscreen rows were constructed"
        );
        assert!(!rendered.borrow().contains(&49));
        view.read_with(cx, |view, _| {
            assert!(
                view.rows.scroll.max_offset_for_scrollbar().y > px(2000.),
                "initial layout discarded offscreen row height estimates"
            )
        });

        rendered.borrow_mut().clear();
        view.update(cx, |view, cx| {
            view.width = 420.;
            cx.notify();
        });
        draw(cx);
        assert!(rendered.borrow().len() < 12);
        view.read_with(cx, |view, _| {
            assert!(
                view.rows.scroll.max_offset_for_scrollbar().y > px(2000.),
                "resize discarded offscreen row height estimates"
            )
        });

        view.update(cx, |view, cx| {
            view.rows.scroll.scroll_to(ListOffset {
                item_ix: 30,
                offset_in_item: px(0.),
            });
            cx.notify();
        });
        rendered.borrow_mut().clear();
        draw(cx);
        assert!(rendered.borrow().contains(&30));
        assert!(!rendered.borrow().contains(&0));

        // Inline recording/conflict feedback changes a row's height. Invalidate
        // measurements while retaining the same logical row under the viewport.
        tall.set(true);
        view.update(cx, |view, cx| {
            view.revision += 1;
            cx.notify();
        });
        draw(cx);
        view.read_with(cx, |view, _| {
            assert_eq!(view.rows.scroll.logical_scroll_top().item_ix, 30);
            let row = view.rows.scroll.bounds_for_item(30).unwrap();
            let next = view.rows.scroll.bounds_for_item(31).unwrap();
            assert_eq!(row.size.height, px(140.));
            assert!(
                next.top() >= row.bottom(),
                "expanded row overlaps its successor"
            );
            assert!(view.rows.scroll.max_offset_for_scrollbar().y > px(2000.));
        });

        view.update(cx, |view, cx| {
            view.recording = true;
            cx.notify();
        });
        cx.update(|window, cx| {
            let focus = view.read(cx).focus.clone();
            focus.focus(window, cx);
        });
        draw(cx);
        rendered.borrow_mut().clear();
        // Dispatch a real GPUI wheel over the list padding, as native smoke
        // does, and retain only the active offscreen recorder in addition to
        // the newly visible rows.
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(5.), px(150.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(-700.))),
            ..Default::default()
        });
        draw(cx);
        assert!(
            rendered.borrow().contains(&30),
            "offscreen recorder lost keyboard dispatch"
        );
        assert!(rendered.borrow().iter().any(|&id| id > 35));
        assert!(rendered.borrow().len() < 14);

        let extent = view.read_with(cx, |view, _| view.rows.scroll.max_offset_for_scrollbar());
        view.update(cx, |view, cx| {
            view.recording = false;
            view.revision += 1;
            cx.notify();
        });
        draw(cx);
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.rows.scroll.max_offset_for_scrollbar(),
                extent,
                "canceling an offscreen recorder discarded its unchanged height"
            )
        });

        // Filtering replaces row identities and starts at the first result.
        rendered.borrow_mut().clear();
        view.update(cx, |view, cx| {
            view.ids = vec![49];
            cx.notify();
        });
        draw(cx);
        assert_eq!(*rendered.borrow(), BTreeSet::from([49]));
        view.read_with(cx, |view, _| {
            assert_eq!(view.rows.scroll.logical_scroll_top().item_ix, 0)
        });
    }

    struct CachedShell {
        content: Entity<CachedContent>,
        source: usize,
        width: f32,
    }

    struct CachedContent {
        owner: WeakEntity<CachedShell>,
        source: Option<usize>,
        renders: Rc<Cell<u64>>,
    }

    impl Render for CachedShell {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.content.update(cx, |content, cx| {
                if content.source != Some(self.source) {
                    content.source = Some(self.source);
                    cx.notify();
                }
            });
            div().w(px(self.width)).h(px(300.)).child(
                self.content
                    .clone()
                    .cached(StyleRefinement::default().size_full()),
            )
        }
    }

    impl Render for CachedContent {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            // Match AppearanceView: constructing the content through its owner
            // must not make every future owner notification dirty this cache.
            self.owner
                .update(cx, |owner, _| {
                    div().size_full().child(format!("{}", owner.source))
                })
                .unwrap()
        }
    }

    fn draw_cached(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }

    #[gpui_kit::test]
    fn settings_cached_view_reuses_parent_notifications_but_updates_source_and_bounds(
        cx: &mut TestAppContext,
    ) {
        cx.update(gpui_kit::init);
        let renders = Rc::new(Cell::new(0));
        let (view, cx) = cx.add_window_view(|_, cx| {
            let owner = cx.weak_entity();
            let content = cx.new(|_| CachedContent {
                owner,
                source: None,
                renders: renders.clone(),
            });
            CachedShell {
                content,
                source: 0,
                width: 500.,
            }
        });
        draw_cached(cx);
        let baseline = renders.get();
        assert!(baseline > 0);
        for _ in 0..20 {
            view.update(cx, |_, cx| cx.notify());
            draw_cached(cx);
        }
        assert_eq!(
            renders.get(),
            baseline,
            "unchanged shell notification rebuilt cached content"
        );
        view.update(cx, |view, cx| {
            view.source = 1;
            cx.notify();
        });
        draw_cached(cx);
        assert!(
            renders.get() > baseline,
            "changed source did not invalidate content"
        );
        let before_resize = renders.get();
        view.update(cx, |view, cx| {
            view.width = 420.;
            cx.notify();
        });
        draw_cached(cx);
        assert!(
            renders.get() > before_resize,
            "new viewport bounds reused obsolete layout"
        );
    }
}
