use crate::{app::StudyApp, components::*, resize_overlay::NativeResizeGuide};
use gpui_kit::{prelude::FluentBuilder, *};
use std::time::Duration;
use studyapp_core::{Command, LayoutSettings};

const HANDLE_WIDTH: f32 = 6.;
const WORKSPACE_PADDING: f32 = 32.;
const DOCUMENT_MIN: f32 = 280.;
const NOTES_MIN: f32 = 160.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Navigation,
    Notes,
}

pub struct EffectiveLayout {
    pub navigation: Pixels,
    pub folders: Pixels,
    pub notes: Pixels,
}

/// A drag is a transaction. The rendered layout remains unchanged until release.
pub struct ResizeState {
    pub pane: Pane,
    start_x: Pixels,
    initial_width: f32,
    pane_left: f32,
    original: LayoutSettings,
    pending: LayoutSettings,
    guide_x: Pixels,
    guide: Option<NativeResizeGuide>,
    watchdog: Option<Task<()>>,
}

impl ResizeState {
    fn new(
        pane: Pane,
        start_x: Pixels,
        initial_width: f32,
        pane_left: f32,
        original: &LayoutSettings,
    ) -> Self {
        Self {
            pane,
            start_x,
            initial_width,
            pane_left,
            original: original.clone(),
            pending: original.clone(),
            guide_x: px(pane_left + initial_width),
            guide: None,
            watchdog: None,
        }
    }

    fn update_candidate(&mut self, position_x: Pixels, viewport: f32) {
        self.pending = self.original.clone();
        let delta = f32::from(position_x - self.start_x);
        if delta.abs() >= 1. {
            let width = self.initial_width + delta;
            match self.pane {
                Pane::Navigation if f32::from(position_x) <= self.pane_left + 32. => {
                    self.pending.navigation_visible = false;
                }
                Pane::Navigation => {
                    self.pending.navigation_visible = true;
                    self.pending.navigation_width = width.round().clamp(128., 320.) as u32;
                }
                Pane::Notes if f32::from(position_x) <= self.pane_left + 32. => {
                    self.pending.folders_visible = false;
                }
                Pane::Notes => {
                    self.pending.folders_visible = true;
                    self.pending.notes_width = width.round().clamp(NOTES_MIN, 440.) as u32
                }
            }
        }
        let fitted = fit_layout(&self.pending, viewport);
        let width = match self.pane {
            Pane::Navigation => fitted.navigation,
            Pane::Notes => fitted.notes,
        };
        self.guide_x = px((self.pane_left + f32::from(width)).clamp(0., viewport.max(0.)));
    }
}

pub fn fit_layout(preferred: &LayoutSettings, viewport: f32) -> EffectiveLayout {
    let handles =
        u8::from(preferred.navigation_visible) as f32 + u8::from(preferred.folders_visible) as f32;
    let budget = (viewport - WORKSPACE_PADDING - handles * HANDLE_WIDTH - DOCUMENT_MIN).max(0.);
    let library_min = if preferred.folders_visible {
        NOTES_MIN.min(budget)
    } else {
        0.
    };
    let navigation = if preferred.navigation_visible {
        (preferred.navigation_width as f32).min((budget - library_min).max(0.))
    } else {
        0.
    };
    let notes = if preferred.folders_visible {
        (preferred.notes_width as f32).min((budget - navigation).max(0.))
    } else {
        0.
    };
    EffectiveLayout {
        navigation: px(navigation),
        folders: px(0.),
        notes: px(notes),
    }
}

impl StudyApp {
    pub fn effective_layout(&self, window: &Window) -> EffectiveLayout {
        fit_layout(&self.layout, f32::from(window.viewport_size().width))
    }

    pub fn is_layout_resizing(&self) -> bool {
        self.resizing.is_some()
    }

    pub fn toggle_navigation(&mut self, cx: &mut Context<Self>) {
        if self.storage_busy
            || self.preferences_ui.busy
            || self.preferences_ui.replacing
            || self.has_modal(cx)
        {
            return;
        }
        self.cancel_resize(cx);
        self.layout.navigation_visible = !self.layout.navigation_visible;
        self.persist_layout(cx);
    }

    pub fn toggle_folders(&mut self, cx: &mut Context<Self>) {
        if self.storage_busy
            || self.preferences_ui.busy
            || self.preferences_ui.replacing
            || self.has_modal(cx)
        {
            return;
        }
        self.cancel_resize(cx);
        self.layout.folders_visible = !self.layout.folders_visible;
        self.persist_layout(cx);
    }

    pub(crate) fn persist_layout(&mut self, cx: &mut Context<Self>) {
        if self.page == crate::app::Page::Settings {
            self.stage_layout(cx);
            return;
        }
        self.layout_dirty = true;
        self.send(Command::SaveLayout(self.layout.clone()), cx);
        cx.notify();
    }

    pub fn resize_handle(&self, pane: Pane, cx: &mut Context<Self>) -> impl IntoElement {
        let id = match pane {
            Pane::Navigation => "resize-navigation",
            Pane::Notes => "resize-notes",
        };
        div()
            .id(id)
            .test_support()
            .w(px(HANDLE_WIDTH))
            .h_full()
            .flex_shrink_0()
            .flex()
            .justify_center()
            .when(
                !self.storage_busy && !self.preferences_ui.busy && !self.preferences_ui.replacing,
                |view| view.cursor_col_resize(),
            )
            .when(
                self.storage_busy || self.preferences_ui.busy || self.preferences_ui.replacing,
                |view| view.cursor_default(),
            )
            .hover(|style| style.bg(crate::theme::color(LIGHT)))
            .child(div().w(px(1.)).h_full().bg(crate::theme::color(BORDER)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if this.storage_busy
                        || this.preferences_ui.busy
                        || this.preferences_ui.replacing
                        || this.has_modal(cx)
                        || this.folders.read(cx).overlay_open()
                    {
                        return;
                    }
                    let visible = this.effective_layout(window);
                    let workspace_left = f32::from(visible.navigation)
                        + if this.layout.navigation_visible {
                            HANDLE_WIDTH
                        } else {
                            0.
                        }
                        + WORKSPACE_PADDING / 2.;
                    let (initial_width, pane_left) = match pane {
                        Pane::Navigation => (visible.navigation, 0.),
                        Pane::Notes => (visible.notes, workspace_left),
                    };
                    let mut state = ResizeState::new(
                        pane,
                        event.position.x,
                        f32::from(initial_width),
                        pane_left,
                        &this.layout,
                    );
                    match NativeResizeGuide::new(window) {
                        Ok(mut guide) => {
                            guide.show(state.guide_x);
                            state.guide = Some(guide);
                        }
                        Err(error) => eprintln!("Could not create resize guide: {error}"),
                    }
                    this.resizing = Some(state);
                    let watchdog = cx.spawn_in(window, async move |owner, cx| {
                        loop {
                            cx.background_executor()
                                .timer(Duration::from_millis(16))
                                .await;
                            let keep = owner
                                .update_in(cx, |this, window, cx| {
                                    let Some(state) = this.resizing.as_mut() else {
                                        return false;
                                    };
                                    // Backends release capture before dispatching MouseUp. Poll only
                                    // after message dispatch, so ordinary release can commit first.
                                    if this.storage_busy
                                        || this.preferences_ui.busy
                                        || this.preferences_ui.replacing
                                        || !window.is_window_active()
                                        || state
                                            .guide
                                            .as_ref()
                                            .is_some_and(|guide| !guide.owns_capture())
                                    {
                                        this.finish_resize(true, cx);
                                        return false;
                                    }
                                    if let Some(guide) = state.guide.as_mut() {
                                        guide.show(state.guide_x);
                                    }
                                    true
                                })
                                .unwrap_or(false);
                            if !keep {
                                break;
                            }
                        }
                    });
                    if let Some(state) = &mut this.resizing {
                        state.watchdog = Some(watchdog);
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }

    fn move_divider(&mut self, position: Point<Pixels>, window: &Window) {
        let Some(state) = &mut self.resizing else {
            return;
        };
        state.update_candidate(position.x, f32::from(window.viewport_size().width));
        if let Some(guide) = state.guide.as_mut() {
            guide.show(state.guide_x);
        }
    }

    pub fn cancel_resize(&mut self, cx: &mut Context<Self>) {
        self.finish_resize(true, cx);
    }

    fn finish_resize(&mut self, cancelled: bool, cx: &mut Context<Self>) {
        let Some(state) = self.resizing.take() else {
            return;
        };
        if !cancelled
            && !self.storage_busy
            && !self.preferences_ui.busy
            && !self.preferences_ui.replacing
            && state.pending != state.original
        {
            self.layout = state.pending.clone();
            self.persist_layout(cx);
        }
        // Dropping the state destroys only its owned guide and cancels its poll.
        drop(state);
        cx.notify();
    }

    pub fn resize_event_layer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.weak_entity();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let move_owner = owner.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if !phase.capture() {
                        return;
                    }
                    let _ = move_owner.update(cx, |this, cx| {
                        if this.resizing.is_none() {
                            return;
                        }
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.finish_resize(true, cx);
                        } else {
                            this.move_divider(event.position, window);
                        }
                        cx.stop_propagation();
                    });
                });
                let key_owner = owner.clone();
                window.on_key_event(move |event: &KeyDownEvent, phase, _, cx| {
                    if phase.capture() && event.keystroke.key.eq_ignore_ascii_case("escape") {
                        let _ = key_owner.update(cx, |this, cx| {
                            if this.resizing.is_some() {
                                this.cancel_resize(cx);
                                cx.stop_propagation();
                            }
                        });
                    }
                });
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if !phase.capture() || event.button != MouseButton::Left {
                        return;
                    }
                    let _ = owner.update(cx, |this, cx| {
                        if this.resizing.is_some() {
                            this.move_divider(event.position, window);
                            this.finish_resize(false, cx);
                            cx.stop_propagation();
                        }
                    });
                });
            },
        )
        .absolute()
        .size_full()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DOCUMENT_MIN, HANDLE_WIDTH, LayoutSettings, Pane, ResizeState, WORKSPACE_PADDING,
        fit_layout,
    };
    use gpui_kit::px;
    #[test]
    fn unified_library_has_one_width_and_reserves_document_space() {
        let preferred = LayoutSettings::default();
        let actual = fit_layout(&preferred, 1280.);
        assert_eq!(actual.navigation, px(160.));
        assert_eq!(actual.folders, px(0.));
        assert_eq!(actual.notes, px(280.));
        for navigation_visible in [false, true] {
            for folders_visible in [false, true] {
                let preferred = LayoutSettings {
                    navigation_width: 320,
                    notes_width: 440,
                    navigation_visible,
                    folders_visible,
                    ..Default::default()
                };
                for width in [0., 320., 600., 1000., 1600.] {
                    let layout = fit_layout(&preferred, width);
                    let handles =
                        u8::from(navigation_visible) as f32 + u8::from(folders_visible) as f32;
                    assert!(
                        f32::from(layout.navigation + layout.notes)
                            <= (width - WORKSPACE_PADDING - handles * HANDLE_WIDTH - DOCUMENT_MIN)
                                .max(0.)
                    );
                    assert_eq!(layout.folders, px(0.));
                    if !folders_visible {
                        assert_eq!(layout.notes, px(0.));
                    }
                }
            }
        }
    }
    #[test]
    fn resize_is_transactional_and_library_hides_at_its_own_left_edge() {
        let original = LayoutSettings::default();
        let mut state = ResizeState::new(Pane::Notes, px(462.), 280., 182., &original);
        state.update_candidate(px(500.), 1280.);
        assert_eq!(state.original, original);
        assert_eq!(state.pending.notes_width, 318);
        state.update_candidate(px(213.), 1280.);
        assert!(!state.pending.folders_visible);
        assert_eq!(state.pending.notes_width, original.notes_width);
        assert_eq!(state.guide_x, px(182.));
        state.update_candidate(px(462.), 1280.);
        assert_eq!(state.pending, original);
    }
}
