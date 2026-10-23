//! A UI-thread-only owned popup: a GPUI line cannot paint above the editor HWND.
use gpui_kit::{Pixels, Window};

fn physical_guide_bounds(x: f32, scale: f32, width: i32, height: i32) -> (i32, i32, i32) {
    let line_width = (2. * scale).round().max(1.) as i32;
    let line_width = line_width.min(width.max(0));
    let left = ((x * scale).round() as i32).clamp(0, (width - line_width).max(0));
    (left, line_width, height.max(0))
}

#[cfg(windows)]
mod platform {
    use super::*;
    use anyhow::{Context as _, Result};
    use std::{marker::PhantomData, rc::Rc, sync::OnceLock};
    use windows::{
        Win32::{
            Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
            Graphics::Gdi::{
                BeginPaint, ClientToScreen, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
                InvalidateRect, PAINTSTRUCT,
            },
            UI::{
                HiDpi::GetDpiForWindow, Input::KeyboardAndMouse::GetCapture, WindowsAndMessaging::*,
            },
        },
        core::w,
    };
    use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const TRANSPARENT_KEY: COLORREF = COLORREF(0x00FF00FF);
    const GUIDE_COLOR: COLORREF = COLORREF(0x00557B37);
    static GUIDE_CLASS: OnceLock<u16> = OnceLock::new();

    pub struct NativeResizeGuide {
        hwnd: HWND,
        owner: HWND,
        geometry: Option<(i32, i32, i32, i32, u32)>,
        // Creation, paint and destruction all belong to the parent's UI thread.
        _ui_thread: PhantomData<Rc<()>>,
    }

    impl NativeResizeGuide {
        pub fn new(window: &Window) -> Result<Self> {
            let RawWindowHandle::Win32(raw) = HasWindowHandle::window_handle(window)
                .context("Resize guide requires the parent window handle")?
                .as_raw()
            else {
                anyhow::bail!("Resize guide requires a Windows owner");
            };
            let owner = HWND(raw.hwnd.get() as *mut _);
            // SAFETY: owner is a live GPUI HWND and this runs on its UI thread.
            unsafe {
                let instance = HINSTANCE(GetWindowLongPtrW(owner, GWLP_HINSTANCE) as *mut _);
                let atom = *GUIDE_CLASS.get_or_init(|| {
                    RegisterClassW(&WNDCLASSW {
                        lpfnWndProc: Some(guide_proc),
                        hInstance: instance,
                        lpszClassName: w!("WWStudyResizeGuide"),
                        ..Default::default()
                    })
                });
                anyhow::ensure!(atom != 0, "Could not register resize guide class");
                let hwnd = CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                    w!("WWStudyResizeGuide"),
                    w!("WW-StudyApp resize guide"),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    Some(owner),
                    None,
                    Some(instance),
                    None,
                )?;
                let guide = Self {
                    hwnd,
                    owner,
                    geometry: None,
                    _ui_thread: PhantomData,
                };
                // If setup fails, Drop also releases the just-created HWND.
                SetLayeredWindowAttributes(hwnd, TRANSPARENT_KEY, 255, LWA_COLORKEY)?;
                Ok(guide)
            }
        }

        pub fn owns_capture(&self) -> bool {
            // No SetCapture/SetFocus: the GPUI parent already owns mouse capture.
            unsafe { GetCapture() == self.owner && GetForegroundWindow() == self.owner }
        }

        pub fn show(&mut self, x: Pixels) {
            // Owned popups stay above their owner but are never globally topmost.
            // Recheck foreground before every move so no line covers other apps.
            unsafe {
                if !self.owns_capture() || IsIconic(self.owner).as_bool() {
                    self.hide();
                    return;
                }
                let mut rect = RECT::default();
                if GetClientRect(self.owner, &mut rect).is_err() {
                    self.hide();
                    return;
                }
                let dpi = GetDpiForWindow(self.owner).max(96);
                let scale = dpi as f32 / 96.;
                let (left, width, height) = physical_guide_bounds(
                    f32::from(x),
                    scale,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                );
                if width == 0 || height == 0 {
                    self.hide();
                    return;
                }
                let mut origin = POINT { x: left, y: 0 };
                if !ClientToScreen(self.owner, &mut origin).as_bool() {
                    self.hide();
                    return;
                }
                let geometry = (origin.x, origin.y, width, height, dpi);
                if self.geometry != Some(geometry)
                    && SetWindowPos(
                        self.hwnd,
                        None,
                        origin.x,
                        origin.y,
                        width,
                        height,
                        SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
                    )
                    .is_ok()
                {
                    self.geometry = Some(geometry);
                    let _ = InvalidateRect(Some(self.hwnd), None, false);
                }
            }
        }

        fn hide(&mut self) {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.geometry = None;
        }
    }

    impl Drop for NativeResizeGuide {
        fn drop(&mut self) {
            // This destroys exactly the HWND owned by this value, never its owner.
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }

    unsafe extern "system" fn guide_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        // The procedure never enters GPUI/entity code, including during creation.
        unsafe {
            match message {
                WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
                WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
                WM_ERASEBKGND => LRESULT(1),
                WM_PAINT => {
                    let mut paint = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut paint);
                    let mut rect = RECT::default();
                    if GetClientRect(hwnd, &mut rect).is_ok() {
                        let key = CreateSolidBrush(TRANSPARENT_KEY);
                        FillRect(dc, &rect, key);
                        let _ = DeleteObject(key.into());
                        let brush = CreateSolidBrush(GUIDE_COLOR);
                        let scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.;
                        let dash = (6. * scale).round().max(1.) as i32;
                        let gap = (4. * scale).round().max(1.) as i32;
                        let mut top = 0;
                        while top < rect.bottom {
                            let stripe = RECT {
                                left: 0,
                                top,
                                right: rect.right,
                                bottom: (top + dash).min(rect.bottom),
                            };
                            FillRect(dc, &stripe, brush);
                            top += dash + gap;
                        }
                        let _ = DeleteObject(brush.into());
                    }
                    let _ = EndPaint(hwnd, &paint);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, message, wparam, lparam),
            }
        }
    }
}

#[cfg(windows)]
pub use platform::NativeResizeGuide;

#[cfg(not(windows))]
pub struct NativeResizeGuide;

#[cfg(not(windows))]
impl NativeResizeGuide {
    pub fn new(_: &Window) -> anyhow::Result<Self> {
        Ok(Self)
    }
    pub fn show(&mut self, _: Pixels) {}
    pub fn owns_capture(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::physical_guide_bounds;

    #[test]
    fn guide_scales_with_dpi_and_stays_inside_its_owner() {
        assert_eq!(physical_guide_bounds(160., 1., 1280, 800), (160, 2, 800));
        assert_eq!(
            physical_guide_bounds(160., 1.25, 1600, 1000),
            (200, 3, 1000)
        );
        assert_eq!(physical_guide_bounds(160., 1.5, 1920, 1200), (240, 3, 1200));
        assert_eq!(physical_guide_bounds(-20., 1.5, 100, 200), (0, 3, 200));
        assert_eq!(physical_guide_bounds(500., 1.5, 100, 200), (97, 3, 200));
        assert_eq!(physical_guide_bounds(500., 1.5, 0, 0), (0, 0, 0));
    }
}
