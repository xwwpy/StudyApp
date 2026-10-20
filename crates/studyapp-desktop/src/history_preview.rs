use crate::components::*;
use gpui_kit::*;
use std::sync::Arc;
use studyapp_core::NoteVersion;
use studyapp_desktop::presentation::diff_rows;

#[derive(Default)]
pub struct HistoryPreview {
    key: (String, usize),
    pub rows: Arc<Vec<(String, char)>>,
    pub scroll: UniformListScrollHandle,
}

impl HistoryPreview {
    pub fn prepare(&mut self, version: Option<&NoteVersion>, width: f32) {
        // Reserve room for navigation, notes, version timeline, padding and CJK glyphs.
        let columns = ((width - 850.) / 7.).max(16.) as usize;
        let key = (version.map(|v| v.id.clone()).unwrap_or_default(), columns);
        if self.key == key {
            return;
        }
        let changed_version = self.key.0 != key.0;
        self.key = key;
        self.rows = Arc::new(
            version
                .map(|v| diff_rows(&v.diff, columns))
                .unwrap_or_default(),
        );
        if changed_version {
            self.scroll = UniformListScrollHandle::default();
        }
    }

    pub fn render(&self) -> Div {
        let rows = self.rows.clone();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .bg(rgb(BG))
            .rounded_lg()
            .child(
                uniform_list("history-diff-scroll", rows.len(), move |range, _, _| {
                    range
                        .map(|i| {
                            let (text, marker) = &rows[i];
                            let color = match marker {
                                '+' => GREEN,
                                '-' => 0xB45142,
                                _ => MUTED,
                            };
                            div()
                                .id(("diff-line", i))
                                .test_support()
                                .h(px(22.))
                                .px_2()
                                .font_family("Consolas")
                                .text_size(px(12.))
                                .line_height(px(22.))
                                .text_color(rgb(color))
                                .whitespace_nowrap()
                                .child(text.clone())
                        })
                        .collect::<Vec<_>>()
                })
                .with_sizing_behavior(ListSizingBehavior::Auto)
                .track_scroll(&self.scroll)
                .absolute()
                .inset_0()
                .size_full()
                .test_support(),
            )
    }
}
