use crate::{app::StudyApp, components::*, resize_overlay::NativeResizeGuide};
use gpui_kit::{prelude::FluentBuilder, *};
use std::time::Duration;
use studyapp_core::{Command, LayoutSettings};

const HANDLE_WIDTH: f32 = 6.;
const WORKSPACE_PADDING: f32 = 32.;
const DOCUMENT_MIN: f32 = 280.;
const FOLDERS_MIN: f32 = 112.;
const NOTES_MIN: f32 = 160.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Navigation,
    Folders,
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
                Pane::Folders if f32::from(position_x) <= self.pane_left + 32. => {
                    self.pending.folders_visible = false;
                }
                Pane::Folders => {
                    self.pending.folders_visible = true;
                    self.pending.folders_width = width.round().clamp(FOLDERS_MIN, 360.) as u32;
                }
                Pane::Notes => {
                    self.pending.notes_width = width.round().clamp(NOTES_MIN, 440.) as u32
                }
            }
        }
        let fitted = fit_layout(&self.pending, viewport);
        let width = match self.pane {
            Pane::Navigation => fitted.navigation,
            Pane::Folders => fitted.folders,
            Pane::Notes => fitted.notes,
        };
        self.guide_x = px((self.pane_left + f32::from(width)).clamp(0., viewport.max(0.)));
    }
}

pub fn fit_layout(preferred: &LayoutSettings, viewport: f32) -> EffectiveLayout {
    let handles = 1.
        + u8::from(preferred.navigation_visible) as f32
        + u8::from(preferred.folders_visible) as f32;
    let budget = (viewport - WORKSPACE_PADDING - handles * HANDLE_WIDTH - DOCUMENT_MIN).max(0.);
    let folders_min = if preferred.folders_visible {
        FOLDERS_MIN
    } else {
        0.
    };
    let navigation = if preferred.navigation_visible {
        (preferred.navigation_width as f32).min((budget - folders_min - NOTES_MIN).max(0.))
    } else {
        0.
    };
    let remaining = (budget - navigation).max(0.);
    let mut folders = if preferred.folders_visible {
        preferred.folders_width as f32
    } else {
        0.
    };
    let mut notes = preferred.notes_width as f32;
    let excess = (folders + notes - remaining).max(0.);
    notes -= excess.min((notes - NOTES_MIN).max(0.));
    folders = folders.min((remaining - notes).max(folders_min).min(remaining));
    notes = notes.min((remaining - folders).max(0.));
    EffectiveLayout {
        navigation: px(navigation),
        folders: px(folders),
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

    fn persist_layout(&mut self, cx: &mut Context<Self>) {
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
            Pane::Folders => "resize-folders",
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
            .hover(|style| style.bg(rgb(LIGHT)))
            .child(div().w(px(1.)).h_full().bg(rgb(BORDER)))
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
                        Pane::Folders => (visible.folders, workspace_left),
                        Pane::Notes => (
                            visible.notes,
                            workspace_left
                                + f32::from(visible.folders)
                                + if this.layout.folders_visible {
                                    HANDLE_WIDTH
                                } else {
                                    0.
                                },
                        ),
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
    fn normal_workspace_preserves_preferred_widths() {
        let preferences = LayoutSettings::default();
        let actual = fit_layout(&preferences, 1280.);
        assert_eq!(actual.navigation, px(160.));
        assert_eq!(actual.folders, px(144.));
        assert_eq!(actual.notes, px(200.));
    }

    #[test]
    fn narrow_window_reserves_document_without_mutating_preferences() {
        let preferences = LayoutSettings {
            navigation_width: 320,
            folders_width: 360,
            notes_width: 440,
            navigation_visible: true,
            folders_visible: true,
        };
        let actual = fit_layout(&preferences, 1000.);
        let occupied = f32::from(actual.navigation + actual.folders + actual.notes);
        assert!(1000. - occupied - WORKSPACE_PADDING - 3. * HANDLE_WIDTH >= DOCUMENT_MIN);
        assert!(actual.folders >= px(112.));
        assert!(actual.notes >= px(160.));
        let restored = fit_layout(&preferences, 1600.);
        assert_eq!(restored.folders, px(360.));
        assert_eq!(restored.notes, px(440.));
    }

    #[test]
    fn hidden_navigation_uses_no_width_and_small_bounds_never_go_negative() {
        let preferences = LayoutSettings {
            navigation_visible: false,
            ..Default::default()
        };
        for width in [0., 320., 600., 1000.] {
            let actual = fit_layout(&preferences, width);
            assert_eq!(actual.navigation, px(0.));
            assert!(actual.folders >= px(0.));
            assert!(actual.notes >= px(0.));
        }
    }

    #[test]
    fn candidate_resize_never_mutates_original_layout() {
        let original = LayoutSettings::default();
        let mut state = ResizeState::new(Pane::Folders, px(310.), 144., 166., &original);
        state.update_candidate(px(370.), 1280.);
        assert_eq!(original, LayoutSettings::default());
        assert_eq!(state.original, original);
        assert_eq!(state.pending.folders_width, 204);
        state.update_candidate(px(310.), 1280.);
        assert_eq!(state.pending, original);
    }

    #[test]
    fn each_collapsible_pane_uses_its_own_left_edge_and_retains_restored_width() {
        let original = LayoutSettings::default();
        for (pane, left, width) in [(Pane::Navigation, 0., 160.), (Pane::Folders, 182., 144.)] {
            let mut state = ResizeState::new(pane, px(left + width), width, left, &original);
            state.update_candidate(px(left + 31.), 1280.);
            match pane {
                Pane::Navigation => assert!(!state.pending.navigation_visible),
                Pane::Folders => assert!(!state.pending.folders_visible),
                Pane::Notes => unreachable!(),
            }
            assert_eq!(state.pending.navigation_width, original.navigation_width);
            assert_eq!(state.pending.folders_width, original.folders_width);
            assert_eq!(state.guide_x, px(left));
            state.update_candidate(px(left + width), 1280.);
            assert_eq!(state.pending, original);
        }
    }

    #[test]
    fn hidden_panes_have_neither_width_nor_separator_budget() {
        for navigation_visible in [false, true] {
            for folders_visible in [false, true] {
                let preferred = LayoutSettings {
                    navigation_visible,
                    folders_visible,
                    ..Default::default()
                };
                for viewport in [0., 320., 600., 1000., 1600.] {
                    let actual = fit_layout(&preferred, viewport);
                    if !navigation_visible {
                        assert_eq!(actual.navigation, px(0.));
                    }
                    if !folders_visible {
                        assert_eq!(actual.folders, px(0.));
                    }
                    let used = f32::from(actual.navigation + actual.folders + actual.notes);
                    let handles =
                        1. + u8::from(navigation_visible) as f32 + u8::from(folders_visible) as f32;
                    assert!(
                        used <= (viewport
                            - WORKSPACE_PADDING
                            - handles * HANDLE_WIDTH
                            - DOCUMENT_MIN)
                            .max(0.)
                    );
                }
            }
        }
        let visible = LayoutSettings {
            navigation_visible: false,
            ..Default::default()
        };
        let hidden = LayoutSettings {
            folders_visible: false,
            ..visible.clone()
        };
        assert!(fit_layout(&hidden, 650.).notes > fit_layout(&visible, 650.).notes);
    }
}
