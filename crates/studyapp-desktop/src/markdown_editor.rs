use crate::components::*;
use gpui_kit::{
    component::{
        Disableable, Selectable, Sizable,
        button::{Button, ButtonVariants},
        input::{Enter, Escape, InputEvent, Redo, Textarea, TextareaState, Undo},
        text::TextView,
    },
    prelude::FluentBuilder,
    *,
};
use std::{ops::Range, time::Instant};
use studyapp_desktop::{
    markdown::{MarkdownBlock, MarkdownBuffer, markdown_blocks},
    markdown_format::{MarkdownFormat, format_markdown},
};

const LIVE_BYTES: usize = 256 * 1024;
const LIVE_BLOCKS: usize = 500;
const PREVIEW_BYTES: usize = 64 * 1024;
const PREVIEW_LINES: usize = 400;

pub struct MarkdownChanged;

/// Native inputs own caret, selection and IME. Only raw Markdown enters the
/// buffer; rendered HTML is a disposable view, never a serialization format.
pub struct MarkdownEditor {
    buffer: MarkdownBuffer,
    pub(crate) source_input: Entity<TextareaState>,
    pub(crate) block_input: Entity<TextareaState>,
    pub(crate) source_mode: bool,
    blocks: Vec<MarkdownBlock>,
    active: Option<(usize, Range<usize>)>,
    pub(crate) live_html: String,
    source_html: String,
    readonly: bool,
    limited: bool,
    last_edit: Instant,
    focus: FocusHandle,
}

impl EventEmitter<MarkdownChanged> for MarkdownEditor {}

impl MarkdownEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let source_input =
            cx.new(|cx| TextareaState::new(window, cx).placeholder("在这里输入完整 Markdown…"));
        let block_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 16)
                .placeholder("开始写作，支持 Markdown…")
        });
        for (input, source) in [(&source_input, true), (&block_input, false)] {
            cx.subscribe_in(input, window, move |this, _, event, _, cx| {
                if matches!(event, InputEvent::Change | InputEvent::Blur) {
                    this.accept_input(source, cx);
                }
            })
            .detach();
        }
        Self {
            buffer: MarkdownBuffer::new(String::new()),
            source_input,
            block_input,
            source_mode: false,
            blocks: Vec::new(),
            active: None,
            live_html: String::new(),
            source_html: String::new(),
            readonly: false,
            limited: false,
            last_edit: Instant::now(),
            focus: cx.focus_handle(),
        }
    }

    pub fn value(&self) -> &str {
        self.buffer.text()
    }

    pub fn set_value(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        self.buffer = MarkdownBuffer::new(value);
        self.active = None;
        self.source_mode = false;
        self.readonly = false;
        self.set_readonly(false, cx);
        self.rebuild();
        self.source_input.update(cx, |s, cx| {
            s.set_value(self.buffer.text().to_owned(), window, cx)
        });
        if self.limited {
            self.source_mode = true;
        } else if self.buffer.text().is_empty() {
            self.activate(0, 0..0, window, cx);
        }
        cx.notify();
    }

    pub fn set_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
        self.readonly = readonly;
        // Apply immediately as well as in render, so pending key events cannot
        // change the document after a version has been submitted.
        for input in [&self.source_input, &self.block_input] {
            input.update(cx, |s, cx| s.set_readonly(readonly, cx));
        }
        cx.notify();
    }

    /// An explicit save/mode change commits the visible native composition
    /// before reading the canonical buffer. GPUI's unmark-only IME path does
    /// not emit Change, so event subscriptions alone are insufficient.
    pub fn flush(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.readonly {
            return;
        }
        let input = if self.source_mode {
            &self.source_input
        } else {
            &self.block_input
        };
        input.update(cx, |s, cx| s.unmark_text(window, cx));
        self.accept_input(self.source_mode, cx);
    }

    fn accept_input(&mut self, source: bool, cx: &mut Context<Self>) {
        if self.readonly || source != self.source_mode {
            return;
        }
        let (range, text) = if source {
            (
                0..self.buffer.text().len(),
                self.source_input.read(cx).value().to_string(),
            )
        } else if let Some((_, range)) = &self.active {
            (range.clone(), self.block_input.read(cx).value().to_string())
        } else {
            return;
        };
        if self.last_edit.elapsed().as_millis() > 750 {
            self.buffer.checkpoint();
        }
        self.last_edit = Instant::now();
        if !self.buffer.replace(range.clone(), &text) {
            return;
        }
        if source {
            // Re-evaluate the limit after edits so shortening a large document
            // enables live mode again without reopening the note.
            self.rebuild();
        } else {
            if let Some((_, active)) = &mut self.active {
                active.end = active.start + text.len();
            }
            // Keep the active input entity and range stable until the user
            // leaves it; an unfinished fence must not steal the IME/caret.
            self.live_html =
                active_preview(self.buffer.text(), &(range.start..range.start + text.len()));
        }
        cx.emit(MarkdownChanged);
        cx.notify();
    }

    fn rebuild(&mut self) {
        self.limited = self.buffer.text().len() > LIVE_BYTES;
        self.blocks = if self.limited {
            Vec::new()
        } else {
            markdown_blocks(self.buffer.text())
        };
        if self.blocks.len() > LIVE_BLOCKS
            || self
                .blocks
                .iter()
                .any(|b| preview_end(&self.buffer.text()[b.range.clone()]) < b.range.len())
        {
            self.limited = true;
            self.blocks.clear();
        }
        self.source_html =
            if self.limited || preview_end(self.buffer.text()) < self.buffer.text().len() {
                preview(self.buffer.text())
            } else {
                self.blocks.iter().map(|b| b.html.as_str()).collect()
            };
    }

    fn activate(
        &mut self,
        index: usize,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        self.buffer.checkpoint();
        let text = self.buffer.text()[range.clone()].to_owned();
        self.live_html = active_preview(self.buffer.text(), &range);
        self.active = Some((index, range));
        self.block_input.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush(window, cx);
        self.active = None;
        self.buffer.checkpoint();
        self.rebuild();
        if self.limited {
            self.source_mode = true;
            self.source_input.update(cx, |s, cx| {
                s.set_value(self.buffer.text().to_owned(), window, cx);
                s.focus(window, cx);
            });
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    fn switch_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish(window, cx);
        if self.limited {
            return;
        }
        self.source_mode = !self.source_mode;
        if self.source_mode {
            self.source_input.update(cx, |s, cx| {
                s.set_value(self.buffer.text().to_owned(), window, cx);
                s.focus(window, cx);
            });
        }
        cx.notify();
    }

    fn edit_block(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.active.as_ref().is_some_and(|(i, _)| *i == index) {
            return;
        }
        // Composition can change the active length without a Change event.
        // Commit it while the old block partition is still available.
        self.flush(window, cx);
        // Map the clicked, still-visible block through the active edit before
        // rebuilding. Its old byte offset may have moved or merged.
        let Some(block) = self.blocks.get(index) else {
            return;
        };
        let mut offset = block.range.start;
        if let Some((active_index, active_range)) = &self.active
            && index > *active_index
        {
            let old_length = self.blocks[*active_index].range.len();
            offset = offset - old_length + active_range.len();
        }
        self.finish(window, cx);
        if !self.source_mode
            && let Some((index, block)) = self
                .blocks
                .iter()
                .enumerate()
                .find(|(_, b)| b.range.contains(&offset))
        {
            self.activate(index, block.range.clone(), window, cx);
        }
    }

    fn append(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.readonly {
            return;
        }
        self.finish(window, cx);
        if self.source_mode {
            return;
        }
        let end = self.buffer.text().len();
        // The separator is editable and undoable with the new paragraph.
        let separator = if end == 0 || self.buffer.text().ends_with("\n\n") {
            ""
        } else if self.buffer.text().ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        self.activate(self.blocks.len(), end..end, window, cx);
        self.block_input.update(cx, |s, cx| {
            s.replace_all(separator, window, cx);
            let end = s.value().len();
            s.set_selected_range(end..end, cx);
        });
        self.accept_input(false, cx);
    }

    fn history(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.readonly {
            return;
        }
        self.accept_input(self.source_mode, cx);
        let changed = if redo {
            self.buffer.redo()
        } else {
            self.buffer.undo()
        };
        if changed {
            self.active = None;
            self.rebuild();
            self.source_mode |= self.limited;
            self.source_input.update(cx, |s, cx| {
                s.set_value(self.buffer.text().to_owned(), window, cx);
                if self.source_mode {
                    s.focus(window, cx);
                }
            });
            if !self.source_mode {
                window.focus(&self.focus, cx);
            }
            cx.emit(MarkdownChanged);
            cx.notify();
        }
    }

    fn format(&mut self, kind: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.readonly {
            return;
        }
        if !self.source_mode && self.active.is_none() {
            self.append(window, cx);
        }
        self.flush(window, cx);
        self.buffer.checkpoint();
        let input = if self.source_mode {
            self.source_input.clone()
        } else {
            self.block_input.clone()
        };
        input.update(cx, |s, cx| {
            let kind = match kind {
                "bold" => MarkdownFormat::Bold,
                "italic" => MarkdownFormat::Italic,
                "heading" => MarkdownFormat::Heading,
                "list" => MarkdownFormat::List,
                "quote" => MarkdownFormat::Quote,
                "code" => MarkdownFormat::Code,
                "table" => MarkdownFormat::Table,
                _ => return,
            };
            if let Some(edit) = format_markdown(&s.value(), s.selected_range(), kind) {
                s.set_selected_range(edit.range, cx);
                s.replace(edit.replacement, window, cx);
                s.set_selected_range(edit.selection, cx);
                s.focus(window, cx);
            }
        });
        self.accept_input(self.source_mode, cx);
        self.buffer.checkpoint();
    }

    fn composing(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let input = if self.source_mode {
            &self.source_input
        } else {
            &self.block_input
        };
        input.update(cx, |s, cx| s.marked_text_range(window, cx).is_some())
    }
}

fn preview(text: &str) -> String {
    let end = preview_end(text);
    markdown_blocks(&text[..end])
        .into_iter()
        .take(LIVE_BLOCKS)
        .map(|b| b.html)
        .collect()
}

fn preview_end(text: &str) -> usize {
    let mut end = text.len().min(PREVIEW_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if let Some((offset, _)) = text[..end].match_indices('\n').nth(PREVIEW_LINES) {
        end = offset;
    }
    end
}

fn active_preview(text: &str, range: &Range<usize>) -> String {
    if text.len() > LIVE_BYTES || preview_end(&text[range.clone()]) < range.len() {
        return preview(&text[range.clone()]);
    }
    markdown_blocks(text)
        .into_iter()
        .filter(|b| b.range.start < range.end && b.range.end > range.start)
        .take(LIVE_BLOCKS)
        .map(|b| b.html)
        .collect()
}

impl Render for MarkdownEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toolbar = row()
            .gap_1()
            .flex_wrap()
            .pb_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .children(
                [
                    ("heading", "标题"),
                    ("bold", "粗体"),
                    ("italic", "斜体"),
                    ("list", "列表"),
                    ("quote", "引用"),
                    ("code", "代码"),
                    ("table", "表格"),
                ]
                .into_iter()
                .map(|(kind, label)| {
                    Button::new(SharedString::from(format!("md-{kind}")))
                        .small()
                        .ghost()
                        .disabled(self.readonly)
                        .label(label)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.format(kind, window, cx);
                        }))
                }),
            )
            .child(div().flex_1())
            .child(
                Button::new("md-undo")
                    .small()
                    .ghost()
                    .label("撤销")
                    .disabled(self.readonly || !self.buffer.can_undo())
                    .on_click(cx.listener(|this, _, window, cx| this.history(false, window, cx))),
            )
            .child(
                Button::new("md-redo")
                    .small()
                    .ghost()
                    .label("重做")
                    .disabled(self.readonly || !self.buffer.can_redo())
                    .on_click(cx.listener(|this, _, window, cx| this.history(true, window, cx))),
            )
            .child(
                Button::new("md-source-toggle")
                    .small()
                    .ghost()
                    .selected(self.source_mode)
                    .disabled(self.readonly || self.limited)
                    .label(if self.source_mode {
                        "即时编辑"
                    } else {
                        "</> 源码"
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.switch_mode(window, cx))),
            );
        let mut body = column()
            .id("md-scroll")
            .flex_1()
            .min_h_0()
            .gap_2()
            .overflow_y_scroll()
            .px_3()
            .py_2();
        if self.source_mode {
            body = body
                .when(
                    self.limited || preview_end(self.buffer.text()) < self.buffer.text().len(),
                    |v| {
                        v.child(muted(
                            "长文档预览仅显示开头；源码可完整编辑，保存包含全部内容。",
                        ))
                    },
                )
                .child(
                    div()
                        .id("md-source-input")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.source_input.update(cx, |s, cx| s.focus(window, cx));
                        }))
                        .child(
                            Textarea::new(&self.source_input)
                                .aria_label("Markdown 全文源码")
                                .readonly(self.readonly)
                                .h(px(260.)),
                        )
                        .test_support(),
                )
                .child(muted("实时预览"))
                .child(
                    TextView::html("md-source-preview", self.source_html.clone()).scrollable(false),
                );
        } else {
            for (index, block) in self.blocks.iter().enumerate() {
                if self.active.as_ref().is_some_and(|(i, _)| *i == index) {
                    body = body.child(self.active_view(cx));
                } else {
                    body = body.child(
                        div()
                            .id(("md-block", index))
                            .rounded_md()
                            .p_2()
                            .min_h(px(30.))
                            .cursor_text()
                            .hover(|s| s.bg(rgb(0xF4F8F4)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.edit_block(index, window, cx);
                            }))
                            .when(block.html.trim().is_empty(), |v| {
                                v.child(muted("空行 / Markdown 定义 · 点击编辑"))
                            })
                            .when(!block.html.trim().is_empty(), |v| {
                                v.child(
                                    TextView::html(("md-rendered", index), block.html.clone())
                                        .selectable(false)
                                        .scrollable(false),
                                )
                            })
                            .test_support(),
                    );
                }
            }
            if self
                .active
                .as_ref()
                .is_some_and(|(i, _)| *i >= self.blocks.len())
            {
                body = body.child(self.active_view(cx));
            }
            body = body.child(
                Button::new("md-append")
                    .ghost()
                    .disabled(self.readonly)
                    .label("+ 继续写作")
                    .on_click(cx.listener(|this, _, window, cx| this.append(window, cx))),
            );
        }
        column()
            .id("live-markdown-editor")
            .track_focus(&self.focus)
            .tab_index(0)
            .gap_2()
            .h(px(480.))
            .min_w_0()
            // GPUI resolves native input actions before raw key listeners.
            // Capture them here so undo spans blocks and source mode alike.
            .capture_action(cx.listener(|this, _: &Undo, window, cx| {
                if !this.composing(window, cx) {
                    this.history(false, window, cx);
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &Redo, window, cx| {
                if !this.composing(window, cx) {
                    this.history(true, window, cx);
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                if !this.source_mode && !this.composing(window, cx) {
                    this.finish(window, cx);
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, action: &Enter, window, cx| {
                if action.secondary && !this.source_mode && !this.composing(window, cx) {
                    this.finish(window, cx);
                    cx.stop_propagation();
                }
            }))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                let control =
                    event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
                if this.composing(window, cx) {
                    return;
                }
                if control && key.eq_ignore_ascii_case("z") {
                    this.history(event.keystroke.modifiers.shift, window, cx);
                } else if control && key.eq_ignore_ascii_case("y") {
                    this.history(true, window, cx);
                } else if control && key.eq_ignore_ascii_case("b") {
                    this.format("bold", window, cx);
                } else if control && key.eq_ignore_ascii_case("i") {
                    this.format("italic", window, cx);
                } else if !this.source_mode && (key == "escape" || (control && key == "enter")) {
                    this.finish(window, cx);
                } else {
                    return;
                }
                cx.stop_propagation();
            }))
            .child(toolbar)
            .child(body)
            .child(
                row()
                    .justify_between()
                    .child(muted(if self.source_mode {
                        "全文源码 · 预览随输入更新"
                    } else {
                        "点击段落编辑 · Esc / Ctrl+Enter 收起 · Ctrl+B 粗体"
                    }))
                    .child(muted(format!(
                        "{} 字符",
                        self.buffer.text().chars().count()
                    ))),
            )
    }
}

impl MarkdownEditor {
    fn active_view(&self, cx: &mut Context<Self>) -> Div {
        let truncated = self.active.as_ref().is_some_and(|(_, range)| {
            preview_end(&self.buffer.text()[range.clone()]) < range.len()
        });
        column()
            .gap_2()
            .border_l_2()
            .border_color(rgb(GREEN))
            .pl_3()
            .py_2()
            .child(
                row().child(muted("正在编辑")).child(div().flex_1()).child(
                    Button::new("md-finish")
                        .small()
                        .ghost()
                        .label("完成本段")
                        .disabled(self.readonly)
                        .on_click(cx.listener(|this, _, window, cx| this.finish(window, cx))),
                ),
            )
            .when(self.buffer.text().len() > LIVE_BYTES, |v| {
                v.child(muted(
                    "预览已限制长度；完成本段后进入完整源码模式，原文不会截断。",
                ))
            })
            .child(
                div()
                    .id("md-block-input")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.block_input.update(cx, |s, cx| s.focus(window, cx));
                    }))
                    .child(
                        Textarea::new(&self.block_input)
                            .aria_label("当前段落 Markdown")
                            .readonly(self.readonly)
                            .appearance(false)
                            .bordered(false),
                    )
                    .test_support(),
            )
            .child(
                div()
                    .id("md-live-preview")
                    .bg(rgb(0xF7FAF7))
                    .rounded_md()
                    .p_3()
                    .child(muted(if truncated {
                        "实时预览 · 仅显示开头，原文完整保留"
                    } else {
                        "实时预览"
                    }))
                    .child(
                        TextView::html("md-active-rendered", self.live_html.clone())
                            .scrollable(false),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn active_preview_resolves_definitions_outside_edited_block() {
        let source = "[visible][ref]\n\nnext\n\n[ref]: https://example.org\n";
        let blocks = markdown_blocks(source);
        assert_eq!(active_preview(source, &blocks[0].range), "<p>visible</p>\n");
    }

    #[::core::prelude::v1::test]
    fn preview_bounds_multibyte_text_and_many_lines_without_touching_source() {
        let source = format!("{}最后", "中文\n".repeat(40_000));
        let result = preview(&source);
        assert!(!result.contains("最后"));
        assert!(result.len() < 16_000);
        assert!(source.ends_with("最后"));
        let source = "中".repeat(PREVIEW_BYTES);
        assert!(source.is_char_boundary(preview_end(&source)));
    }
}
