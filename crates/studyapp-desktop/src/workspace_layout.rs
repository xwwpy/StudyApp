use crate::{app::StudyApp, components::*};
use gpui_kit::{prelude::FluentBuilder, *};
use studyapp_core::{Command, LayoutSettings};

const HANDLE_WIDTH: f32 = 6.;
const WORKSPACE_PADDING: f32 = 32.;
const DOCUMENT_MIN: f32 = 280.;

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

pub struct ResizeState {
    pub pane: Pane,
    start_x: Pixels,
    initial_width: f32,
    original: LayoutSettings,
}

pub fn fit_layout(preferred: &LayoutSettings, viewport: f32) -> EffectiveLayout {
    let navigation = if preferred.navigation_visible {
        (preferred.navigation_width as f32).min(
            (viewport - WORKSPACE_PADDING - 3. * HANDLE_WIDTH - DOCUMENT_MIN - 144. - 180.).max(0.),
        )
    } else {
        0.
    };
    let budget = (viewport
        - navigation
        - if preferred.navigation_visible {
            HANDLE_WIDTH
        } else {
            0.
        }
        - WORKSPACE_PADDING
        - 2. * HANDLE_WIDTH
        - DOCUMENT_MIN)
        .max(0.);
    let mut folders = preferred.folders_width as f32;
    let mut notes = preferred.notes_width as f32;
    let excess = (folders + notes - budget).max(0.);
    notes -= excess.min((notes - 180.).max(0.));
    folders = folders.min((budget - notes).max(144.).min(budget));
    notes = notes.min((budget - folders).max(0.));
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
        if self.storage_busy || self.has_modal(cx) {
            return;
        }
        self.layout.navigation_visible = !self.layout.navigation_visible;
        self.persist_layout(cx);
    }

    fn persist_layout(&mut self, cx: &mut Context<Self>) {
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
            .when(!self.storage_busy, |view| view.cursor_col_resize())
            .when(self.storage_busy, |view| view.cursor_default())
            .bg(rgb(BORDER))
            .hover(|style| style.bg(rgb(0xA9CBBB)))
            .when(
                self.resizing
                    .as_ref()
                    .is_some_and(|state| state.pane == pane),
                |view| view.bg(rgb(GREEN)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if this.storage_busy
                        || this.confirmation.is_some()
                        || this.storage_confirm.is_some()
                        || this.folders.read(cx).modal_open()
                    {
                        return;
                    }
                    let visible = this.effective_layout(window);
                    let initial_width = match pane {
                        Pane::Navigation => visible.navigation,
                        Pane::Folders => visible.folders,
                        Pane::Notes => visible.notes,
                    };
                    this.resizing = Some(ResizeState {
                        pane,
                        start_x: event.position.x,
                        initial_width: f32::from(initial_width),
                        original: this.layout.clone(),
                    });
                    // Child HWNDs otherwise receive pointer movement over the document.
                    this.note_content
                        .update(cx, |editor, cx| editor.set_visible(false, cx));
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
    }

    fn move_divider(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(state) = &self.resizing else { return };
        let delta = f32::from(position.x - state.start_x);
        if delta.abs() < 1. {
            self.layout = state.original.clone();
            cx.notify();
            return;
        }
        let width = state.initial_width + delta;
        match state.pane {
            Pane::Navigation if position.x <= px(36.) => {
                self.layout.navigation_visible = false;
                self.layout.navigation_width = state.original.navigation_width;
            }
            Pane::Navigation => {
                self.layout.navigation_visible = true;
                self.layout.navigation_width = width.round().clamp(144., 320.) as u32;
            }
            Pane::Folders => self.layout.folders_width = width.round().clamp(144., 360.) as u32,
            Pane::Notes => self.layout.notes_width = width.round().clamp(180., 440.) as u32,
        }
        cx.notify();
    }

    fn finish_resize(&mut self, cancelled: bool, cx: &mut Context<Self>) {
        let Some(state) = self.resizing.take() else {
            return;
        };
        if cancelled {
            self.layout = state.original;
        } else if self.layout != state.original {
            self.persist_layout(cx);
        }
        cx.notify();
    }

    pub fn resize_event_layer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.weak_entity();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let move_owner = owner.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
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
                            this.move_divider(event.position, cx);
                        }
                        cx.stop_propagation();
                    });
                });
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if !phase.capture() || event.button != MouseButton::Left {
                        return;
                    }
                    let _ = owner.update(cx, |this, cx| {
                        if this.resizing.is_some() {
                            this.move_divider(event.position, cx);
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
    use super::{DOCUMENT_MIN, HANDLE_WIDTH, LayoutSettings, WORKSPACE_PADDING, fit_layout};
    use gpui_kit::px;

    #[test]
    fn normal_workspace_preserves_preferred_widths() {
        let preferences = LayoutSettings::default();
        let actual = fit_layout(&preferences, 1280.);
        assert_eq!(actual.navigation, px(184.));
        assert_eq!(actual.folders, px(176.));
        assert_eq!(actual.notes, px(240.));
    }

    #[test]
    fn narrow_window_reserves_document_without_mutating_preferences() {
        let preferences = LayoutSettings {
            navigation_width: 320,
            folders_width: 360,
            notes_width: 440,
            navigation_visible: true,
        };
        let actual = fit_layout(&preferences, 1000.);
        let occupied = f32::from(actual.navigation + actual.folders + actual.notes);
        assert!(1000. - occupied - WORKSPACE_PADDING - 3. * HANDLE_WIDTH >= DOCUMENT_MIN);
        assert!(actual.folders >= px(144.));
        assert!(actual.notes >= px(180.));
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
}
