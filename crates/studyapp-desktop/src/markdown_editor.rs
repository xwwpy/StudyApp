use crate::components::*;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde::Deserialize;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};
use studyapp_desktop::editor_protocol::{EditPatch, EditorDocument};

pub struct MarkdownChanged;
#[derive(Clone)]
pub enum MarkdownEvent {
    Save,
    Flushed(u64),
    Error(String),
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Message {
    Ready,
    Change {
        generation: u64,
        #[serde(rename = "baseRevision")]
        base: u64,
        revision: u64,
        changes: Vec<EditPatch>,
    },
    Flush {
        generation: u64,
        request: u64,
        revision: u64,
        #[serde(rename = "normalizedText")]
        normalized_text: String,
    },
    Save {
        generation: u64,
    },
    Error {
        generation: Option<u64>,
        message: String,
    },
    #[cfg(feature = "ui-test")]
    Test {
        request: serde_json::Value,
        result: serde_json::Value,
    },
    #[serde(other)]
    Other,
}

/// One continuous document lives in CodeMirror. Native persistence receives only
/// revisioned text transactions; no Markdown/HTML parsing runs while typing.
pub struct MarkdownEditor {
    document: EditorDocument,
    sender: mpsc::Sender<String>,
    receiver: mpsc::Receiver<String>,
    #[cfg(windows)]
    host: Option<crate::web_editor_host::HostedEditor>,
    creating: bool,
    ready: bool,
    visible: bool,
    readonly: bool,
    resume_readonly: bool,
    error: Option<String>,
    pending: Option<(u64, Instant)>,
    recovering: bool,
    request: u64,
    #[cfg(feature = "ui-test")]
    pub(crate) test_results: std::collections::HashMap<String, serde_json::Value>,
    _poll: Task<()>,
}
impl EventEmitter<MarkdownChanged> for MarkdownEditor {}
impl EventEmitter<MarkdownEvent> for MarkdownEditor {}

impl MarkdownEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let poll = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(25))
                    .await;
                if this.update_in(cx, |this, _, cx| this.poll(cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            document: EditorDocument::new(String::new()),
            sender,
            receiver,
            #[cfg(windows)]
            host: None,
            creating: false,
            ready: false,
            visible: false,
            readonly: false,
            resume_readonly: false,
            error: None,
            pending: None,
            recovering: false,
            request: 0,
            #[cfg(feature = "ui-test")]
            test_results: Default::default(),
            _poll: poll,
        }
    }

    pub fn value(&self) -> &str {
        self.document.text()
    }
    pub fn ready(&self) -> bool {
        self.ready && self.error.is_none()
    }
    pub fn creating(&self) -> bool {
        self.creating
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn set_value(&mut self, value: String, _window: &mut Window, cx: &mut Context<Self>) {
        self.document.reset(value);
        self.readonly = false;
        self.resume_readonly = false;
        self.pending = None;
        self.recovering = false;
        // A protocol failure can be recovered only by explicitly loading a
        // document again. A runtime creation failure remains visible.
        if self.ready {
            self.error = None;
            self.load(cx);
        }
        cx.notify();
    }

    fn evaluate(&self, script: &str) -> anyhow::Result<()> {
        #[cfg(windows)]
        if let Some(host) = &self.host {
            return host.evaluate(script);
        }
        anyhow::bail!("编辑器尚未准备好")
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let payload = serde_json::json!({"generation": self.document.generation(), "text": self.document.text(), "readonly": self.readonly});
        if let Err(error) = self.evaluate(&format!("window.studyappEditor.load({payload});")) {
            self.fail(format!("无法载入编辑器：{error:#}"), cx);
        }
    }

    pub fn set_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
        self.readonly = readonly;
        self.resume_readonly = readonly;
        if self.ready
            && self.error.is_none()
            && let Err(error) =
                self.evaluate(&format!("window.studyappEditor.setReadonly({readonly});"))
        {
            self.fail(format!("无法更新编辑状态：{error:#}"), cx);
        }
        cx.notify();
    }

    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        #[cfg(windows)]
        if let Some(host) = &self.host
            && let Err(error) = host.set_visible(visible && self.ready)
        {
            self.fail(format!("无法显示编辑区：{error:#}"), cx);
        }
        cx.notify();
    }

    /// The caller records its intent before this acknowledgement is dispatched.
    /// Saving, navigation and close must wait for MarkdownEvent::Flushed.
    pub fn request_flush(&mut self, cx: &mut Context<Self>) -> anyhow::Result<u64> {
        anyhow::ensure!(self.pending.is_none(), "正在确认最后的输入，请稍候");
        anyhow::ensure!(
            self.error.is_none() || !self.ready,
            "编辑状态异常，当前内容已保留，请先恢复编辑器"
        );
        self.request = self.request.wrapping_add(1).max(1);
        let request = self.request;
        if !self.ready {
            // The HWND stays hidden until ready. No browser input can exist yet.
            self.readonly = true;
            cx.emit(MarkdownEvent::Flushed(request));
        } else {
            self.pending = Some((request, Instant::now()));
            if let Err(error) = self.evaluate(&format!(
                "window.studyappEditor.flush({{request:{request}}});"
            )) {
                self.pending = None;
                return Err(error);
            }
        }
        Ok(request)
    }

    fn fail(&mut self, message: String, cx: &mut Context<Self>) {
        self.pending = None;
        self.recovering = false;
        self.error = Some(message.clone());
        self.readonly = true;
        if self.ready {
            let _ = self.evaluate("window.studyappEditor.setReadonly(true);");
        }
        cx.emit(MarkdownEvent::Error(message));
        cx.notify();
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for _ in 0..256 {
            let Ok(json) = self.receiver.try_recv() else {
                break;
            };
            let result = (|| -> anyhow::Result<()> {
                match serde_json::from_str::<Message>(&json)? {
                    Message::Ready => {
                        if self.ready {
                            return Ok(());
                        }
                        self.ready = true;
                        self.load(cx);
                        cx.notify();
                    }
                    Message::Change {
                        generation,
                        base,
                        revision,
                        changes,
                    } => {
                        if self.error.is_some() || self.recovering {
                            return Ok(());
                        }
                        changed |= self.document.apply(generation, base, revision, &changes)?;
                    }
                    Message::Flush {
                        generation,
                        request,
                        revision,
                        normalized_text,
                    } => {
                        if generation != self.document.generation()
                            || self.pending.as_ref().is_none_or(|(id, _)| *id != request)
                        {
                            return Ok(());
                        }
                        if self.recovering {
                            self.document.recover(&normalized_text)?;
                            self.pending = None;
                            self.recovering = false;
                            self.readonly = self.resume_readonly;
                            self.error = None;
                            self.load(cx);
                            changed = true;
                            cx.notify();
                            return Ok(());
                        }
                        self.document
                            .verify_flush(generation, revision, &normalized_text)?;
                        self.pending = None;
                        self.readonly = true;
                        cx.emit(MarkdownEvent::Flushed(request));
                    }
                    Message::Save { generation } => {
                        if generation == self.document.generation() && !self.readonly {
                            cx.emit(MarkdownEvent::Save);
                        }
                    }
                    Message::Error {
                        generation,
                        message,
                    } => {
                        if generation.is_none_or(|g| g == self.document.generation()) {
                            anyhow::bail!("{message}");
                        }
                    }
                    #[cfg(feature = "ui-test")]
                    Message::Test { request, result } => {
                        self.test_results.insert(
                            request
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| request.to_string()),
                            result,
                        );
                    }
                    Message::Other => {}
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.fail(
                    format!("编辑器同步失败：{error:#}。已保存的版本未被覆盖。"),
                    cx,
                );
            }
        }
        if changed {
            cx.emit(MarkdownChanged);
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(12))
        {
            self.fail(
                "等待输入法完成超时，内容仍保留；请结束组合输入后重新打开草稿。".into(),
                cx,
            );
        }
    }

    #[cfg(windows)]
    fn ensure_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.host.is_some() || self.creating || self.error.is_some() || !self.visible {
            return;
        }
        let parent = match crate::web_editor_host::ParentWindow::capture(window) {
            Ok(parent) => parent,
            Err(error) => {
                self.fail(format!("无法创建编辑窗口：{error:#}"), cx);
                return;
            }
        };
        let profile = if cfg!(feature = "ui-test") {
            std::env::temp_dir().join(format!("studyapp-editor-test-{}", std::process::id()))
        } else {
            directories::ProjectDirs::from("io", "StudyApp", "StudyApp")
                .map(|p| p.cache_dir().join("editor"))
                .unwrap_or_else(|| std::env::temp_dir().join("studyapp-editor"))
        };
        let sender = self.sender.clone();
        self.creating = true;
        cx.spawn(async move |this, cx| {
            // Wry pumps Windows messages. Absolutely no GPUI App/entity borrow
            // may be held during the constructor (including a render callback).
            cx.background_executor().timer(Duration::from_millis(1)).await;
            #[cfg(feature = "ui-test")]
            eprintln!("native editor: creating WebView");
            let result = crate::web_editor_host::HostedEditor::new(parent, sender, profile);
            #[cfg(feature = "ui-test")]
            eprintln!("native editor: constructor finished");
            let _ = this.update(cx, |this, cx| {
                this.creating = false;
                match result {
                    Ok(host) => { this.host = Some(host); cx.notify(); }
                    Err(error) => this.fail(format!("连续编辑器启动失败：{error:#}。请确认 Microsoft Edge WebView2 Runtime 已安装。"), cx),
                }
            });
        }).detach();
    }

    #[cfg(feature = "ui-test")]
    pub(crate) fn test(&self, action: serde_json::Value) -> anyhow::Result<()> {
        self.evaluate(&format!("window.studyappEditor.test({action});"))
    }
    #[cfg(feature = "ui-test")]
    pub(crate) fn inject_test_error(&self, stale: bool) {
        let generation = self.document.generation() + u64::from(stale);
        self.sender
            .send(
                serde_json::json!({
                    "type":"error", "generation":generation, "message":"模拟可恢复的输入确认失败"
                })
                .to_string(),
            )
            .unwrap();
    }

    #[cfg(all(windows, feature = "ui-test"))]
    pub(crate) fn capture_png(
        &self,
        path: std::path::PathBuf,
    ) -> anyhow::Result<mpsc::Receiver<Result<(), String>>> {
        self.host
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("WebView 尚未创建"))?
            .capture_png(path)
    }
}

impl Render for MarkdownEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(windows)]
        self.ensure_host(window, cx);
        let weak = cx.weak_entity();
        column()
            .id("continuous-markdown-editor")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .when(self.error.is_some(), |c| {
                c.child(muted(self.error.clone().unwrap_or_default()))
                    .when(self.ready, |c| {
                        c.child(
                            Button::new("recover-editor-bridge")
                                .primary()
                                .label("重试同步并恢复输入")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.error = None;
                                    this.recovering = true;
                                    if let Err(error) = this.request_flush(cx) {
                                        this.fail(error.to_string(), cx);
                                    }
                                    cx.notify();
                                })),
                        )
                    })
            })
            .when(!self.ready && self.error.is_none(), |c| {
                c.child(muted("正在准备连续编辑器…"))
            })
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let bounds = bounds.intersect(&window.content_mask().bounds);
                        let _ = weak.update(cx, |this, cx| {
                            #[cfg(windows)]
                            if let Some(host) = &this.host {
                                let result = host.set_bounds(bounds).and_then(|()| {
                                    host.set_visible(
                                        this.visible
                                            && this.ready
                                            && bounds.size.width > px(0.)
                                            && bounds.size.height > px(0.),
                                    )
                                });
                                if let Err(error) = result {
                                    this.fail(format!("编辑器布局失败：{error:#}"), cx);
                                }
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .size_full()
                .min_h(px(180.)),
            )
    }
}
