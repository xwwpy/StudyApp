//! The Windows child-window host for the continuous Markdown editor.

const EDITOR_PAGE: &str = "https://studyapp.editor/index.html";
// JSON may expand each raw byte to six bytes (\u0000); allow the full 10 MiB
// document plus bounded transaction metadata. The document itself is checked
// separately by the native protocol and by the editor transaction filter.
const MAX_IPC_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorAsset {
    Html,
    Script,
    Stylesheet,
}

fn editor_asset(url: &str) -> Option<EditorAsset> {
    match url {
        "studyapp://editor/index.html" => Some(EditorAsset::Html),
        "studyapp://editor/editor.js" => Some(EditorAsset::Script),
        "studyapp://editor/editor.css" => Some(EditorAsset::Stylesheet),
        _ => None,
    }
}

fn editor_page_allowed(url: &str) -> bool {
    url == EDITOR_PAGE
        || url
            .strip_prefix(EDITOR_PAGE)
            .is_some_and(|suffix| suffix.starts_with('#'))
}

fn ipc_allowed(url: &str, bytes: usize) -> bool {
    editor_page_allowed(url) && bytes <= MAX_IPC_BYTES
}

#[cfg(windows)]
pub use windows_host::{HostedEditor, ParentWindow};

#[cfg(windows)]
mod windows_host {
    use super::{EditorAsset, editor_asset, editor_page_allowed, ipc_allowed};
    use anyhow::{Context as _, Result, ensure};
    use gpui_kit::{Bounds, Pixels, Window};
    use std::{
        borrow::Cow, cell::Cell, marker::PhantomData, path::PathBuf, rc::Rc, sync::mpsc::Sender,
    };
    use wry::{
        NewWindowResponse, PermissionResponse, WebContext, WebView, WebViewBuilder,
        WebViewBuilderExtWindows, WebViewExtWindows,
        dpi::{LogicalPosition, LogicalSize},
        http::{Method, Request, Response, StatusCode},
        raw_window_handle::{
            HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
        },
    };

    const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; \
        style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'none'; \
        frame-src 'none'; frame-ancestors 'none'; object-src 'none'; \
        media-src 'none'; font-src 'none'; form-action 'none'; base-uri 'none'";

    /// An owned copy of the HWND, without a borrow of the GPUI window or App.
    ///
    /// This deliberately remains !Send and !Sync. The caller must keep its GPUI
    /// window alive while creating/using the child, and create it on the same UI
    /// thread. It is not an owner of the actual Windows window.
    #[derive(Clone, Debug)]
    pub struct ParentWindow {
        raw: Win32WindowHandle,
        _ui_thread: PhantomData<Rc<()>>,
    }

    impl ParentWindow {
        pub fn capture(window: &Window) -> Result<Self> {
            // Window also has an inherent window_handle() returning a GPUI
            // AnyWindowHandle. Use the trait explicitly to obtain the HWND.
            let handle =
                HasWindowHandle::window_handle(window).context("无法取得编辑器所属窗口句柄")?;
            let RawWindowHandle::Win32(raw) = handle.as_raw() else {
                anyhow::bail!("Markdown 编辑器需要 Windows 原生窗口");
            };
            Ok(Self {
                raw,
                _ui_thread: PhantomData,
            })
        }
    }

    impl HasWindowHandle for ParentWindow {
        fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
            // SAFETY: capture obtains this handle from a live GPUI window.
            // The caller contract keeps that parent alive for the child lifetime.
            Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(self.raw)) })
        }
    }

    /// A single, persistent WebView for the continuous document surface.
    ///
    /// Keep this on the foreground thread. In particular, new() pumps Windows
    /// messages internally: call it from the body of a foreground async task,
    /// outside any GPUI App/entity/window update or render borrow.
    type EditorGeometry = (f64, f64, f64, f64, u32);
    pub struct HostedEditor {
        // Fields drop in declaration order: release the child before its context.
        webview: WebView,
        _context: WebContext,
        _parent: ParentWindow,
        bounds: Cell<Option<EditorGeometry>>,
        visible: Cell<bool>,
    }

    impl HostedEditor {
        pub fn new(parent: ParentWindow, sender: Sender<String>, profile: PathBuf) -> Result<Self> {
            std::fs::create_dir_all(&profile).context("无法创建本地编辑器运行目录")?;
            let mut context = WebContext::new(Some(profile));
            let builder = WebViewBuilder::new_with_web_context(&mut context)
                .with_custom_protocol("studyapp".to_owned(), |_, request| {
                    embedded_response(request)
                })
                .with_https_scheme(true)
                // Wry maps this to https://studyapp.editor/index.html on Windows.
                .with_url("studyapp://editor/index.html")
                .with_visible(false)
                .with_focused(false)
                .with_bounds(wry::Rect {
                    position: LogicalPosition::new(0.0, 0.0).into(),
                    size: LogicalSize::new(1.0, 1.0).into(),
                })
                .with_background_color((255, 255, 255, 255))
                .with_devtools(false)
                .with_browser_accelerator_keys(false)
                .with_default_context_menus(false)
                .with_general_autofill_enabled(false)
                .with_hotkeys_zoom(false)
                // Keyboard cut/copy/paste remain native editing operations.
                // Do not grant page JavaScript ambient clipboard-read permission.
                .with_clipboard(false)
                .with_permission_handler(|_| PermissionResponse::Deny)
                .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
                .with_download_started_handler(|_, _| false)
                .with_navigation_handler(|url| editor_page_allowed(&url))
                .with_ipc_handler(move |request| {
                    if ipc_allowed(&request.uri().to_string(), request.body().len()) {
                        // Callbacks never enter GPUI: the editor drains this queue
                        // on its foreground task and validates the JSON protocol.
                        let _ = sender.send(request.into_body());
                    }
                });
            #[cfg(feature = "ui-test")]
            let builder =
                builder.with_initialization_script("window.__STUDYAPP_EDITOR_TEST__ = true;");
            let webview = builder
                .build_as_child(&parent)
                .context("无法启动连续 Markdown 编辑器，请检查 WebView2 运行时")?;
            Ok(Self {
                webview,
                _context: context,
                _parent: parent,
                bounds: Cell::new(None),
                visible: Cell::new(false),
            })
        }

        /// Queues script execution. A successful return does not acknowledge its
        /// result; saves must wait for the matching frontend IPC flush response.
        pub fn evaluate(&self, script: &str) -> Result<()> {
            self.webview
                .evaluate_script(script)
                .context("无法向 Markdown 编辑器发送操作")
        }

        /// GPUI Pixels are logical units; Wry applies the window DPI conversion.
        /// The caller must provide an unobscured rectangle inside the parent.
        pub fn set_bounds(&self, bounds: Bounds<Pixels>) -> Result<()> {
            let x = f64::from(f32::from(bounds.origin.x));
            let y = f64::from(f32::from(bounds.origin.y));
            let width = f64::from(f32::from(bounds.size.width));
            let height = f64::from(f32::from(bounds.size.height));
            ensure!(
                [x, y, width, height].iter().all(|v| v.is_finite())
                    && width >= 0.0
                    && height >= 0.0,
                "Markdown 编辑器布局尺寸无效"
            );
            // A window can move to another monitor without changing logical
            // bounds. Include DPI so the child still receives new physical bounds.
            let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(self.webview.hwnd()) };
            ensure!(dpi != 0, "Markdown 编辑器窗口已失效");
            let geometry = (x, y, width, height, dpi);
            if self.bounds.get() == Some(geometry) {
                return Ok(());
            }
            self.webview
                .set_bounds(wry::Rect {
                    position: LogicalPosition::new(x, y).into(),
                    size: LogicalSize::new(width, height).into(),
                })
                .context("无法更新 Markdown 编辑器布局")?;
            self.bounds.set(Some(geometry));
            Ok(())
        }

        pub fn set_visible(&self, visible: bool) -> Result<()> {
            if self.visible.get() == visible {
                return Ok(());
            }
            self.webview
                .set_visible(visible)
                .context("无法更新 Markdown 编辑器显示状态")?;
            self.visible.set(visible);
            Ok(())
        }

        /// Captures the actual native child WebView, which GPUI render_to_image
        /// cannot include. Poll/await the receiver while the message loop runs.
        #[cfg(feature = "ui-test")]
        pub fn capture_png(
            &self,
            path: PathBuf,
        ) -> Result<std::sync::mpsc::Receiver<std::result::Result<(), String>>> {
            use base64::Engine as _;
            use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
            use windows::core::w;

            let (sender, receiver) = std::sync::mpsc::channel();
            let callback = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
                move |status, response| {
                    // Encoding and disk I/O do not hold the WebView callback.
                    std::thread::spawn(move || {
                        let result = (|| -> Result<()> {
                            status.context("WebView2 截图请求失败")?;
                            let response: serde_json::Value = serde_json::from_str(&response)
                                .context("无法读取 WebView2 截图响应")?;
                            let encoded = response
                                .get("data")
                                .and_then(serde_json::Value::as_str)
                                .context("WebView2 截图响应未包含图片")?;
                            let bytes = base64::engine::general_purpose::STANDARD
                                .decode(encoded)
                                .context("WebView2 截图编码无效")?;
                            ensure!(
                                bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                                "WebView2 未返回 PNG 图片"
                            );
                            std::fs::write(&path, bytes).context("无法保存编辑器截图")?;
                            Ok(())
                        })();
                        let _ = sender.send(result.map_err(|error| format!("{error:#}")));
                    });
                    Ok(())
                },
            ));
            // SAFETY: WebView and callback stay on their creating STA thread.
            // WebView2 retains the callback until the asynchronous operation ends.
            unsafe {
                self.webview.webview().CallDevToolsProtocolMethod(
                    w!("Page.captureScreenshot"),
                    w!(r#"{"format":"png","captureBeyondViewport":false,"fromSurface":true}"#),
                    &callback,
                )
            }
            .context("无法请求编辑器原生截图")?;
            Ok(receiver)
        }
    }

    fn embedded_response(request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
        let asset = if request.method() == Method::GET {
            editor_asset(&request.uri().to_string())
        } else {
            None
        };
        let (status, content_type, body): (_, _, &'static [u8]) = match asset {
            Some(EditorAsset::Html) => (
                StatusCode::OK,
                "text/html; charset=utf-8",
                include_bytes!("../editor/dist/index.html"),
            ),
            Some(EditorAsset::Script) => (
                StatusCode::OK,
                "text/javascript; charset=utf-8",
                include_bytes!("../editor/dist/editor.js"),
            ),
            Some(EditorAsset::Stylesheet) => (
                StatusCode::OK,
                "text/css; charset=utf-8",
                include_bytes!("../editor/dist/editor.css"),
            ),
            None => (
                StatusCode::NOT_FOUND,
                "text/plain; charset=utf-8",
                b"Editor resource not found",
            ),
        };
        Response::builder()
            .status(status)
            .header("Content-Type", content_type)
            .header("Content-Security-Policy", CONTENT_SECURITY_POLICY)
            .header("X-Content-Type-Options", "nosniff")
            .header("Referrer-Policy", "no-referrer")
            .header("Cache-Control", "no-store")
            .body(Cow::Borrowed(body))
            .expect("static editor response headers are valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_only_the_three_embedded_assets() {
        assert_eq!(
            editor_asset("studyapp://editor/index.html"),
            Some(EditorAsset::Html)
        );
        assert_eq!(
            editor_asset("studyapp://editor/editor.js"),
            Some(EditorAsset::Script)
        );
        assert_eq!(
            editor_asset("studyapp://editor/editor.css"),
            Some(EditorAsset::Stylesheet)
        );
        for url in [
            "studyapp://editor/",
            "studyapp://editor/index.html?file=secret",
            "studyapp://editor/../Cargo.toml",
            "studyapp://editor/%2e%2e/Cargo.toml",
            "studyapp://editor/editor.js.map",
            "studyapp://editor.evil/editor.js",
            "studyapp://editor:443/editor.js",
            "file:///C:/Users/PC/private.md",
            "https://example.com/editor.js",
        ] {
            assert_eq!(editor_asset(url), None, "{url}");
        }
    }

    #[test]
    fn ipc_accepts_only_the_loaded_editor_document() {
        assert!(editor_page_allowed(EDITOR_PAGE));
        assert!(editor_page_allowed(
            "https://studyapp.editor/index.html#heading"
        ));
        for url in [
            "http://studyapp.editor/index.html",
            "https://studyapp.editor.evil/index.html",
            "https://evil@studyapp.editor/index.html",
            "https://studyapp.editor:443/index.html",
            "https://studyapp.editor/editor.js",
            "https://studyapp.editor/index.html?remote=yes",
            "studyapp://editor/index.html",
            "about:blank",
            "data:text/html,hello",
        ] {
            assert!(!editor_page_allowed(url), "{url}");
        }
    }

    #[test]
    fn oversized_ipc_cannot_enter_the_host_queue() {
        assert!(ipc_allowed(EDITOR_PAGE, MAX_IPC_BYTES));
        assert!(!ipc_allowed(EDITOR_PAGE, MAX_IPC_BYTES + 1));
        assert!(!ipc_allowed("https://example.com/", 1));
    }
}
