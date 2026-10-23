//! Native image jobs are scoped to a library, document generation and request.
//! Resolution is debounced with two bounded slots; imports run in a bounded queue.
use crate::{
    app::{Page, StudyApp},
    drafts::NoteDraft,
    media::{MediaInput, MediaOutcome, MediaRequest, MediaResponse, ResolveControl},
    worker::Work,
};
use gpui_kit::*;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
use studyapp_core::ImageSettings;

const MAX_FILES: usize = 20;

#[derive(Clone, PartialEq, Eq)]
struct DocumentKey {
    library: String,
    generation: u64,
    revision: u64,
    source: Option<PathBuf>,
    settings: ImageSettings,
}

struct Insertion {
    library: String,
    generation: u64,
    group: String,
    title: String,
    settings: ImageSettings,
    inputs: VecDeque<MediaInput>,
    request: Option<u64>,
}

struct Adoption {
    library: String,
    generation: u64,
    request: u64,
    selected_note: Option<String>,
    selected_version: Option<String>,
    draft: NoteDraft,
    commit_after_adoption: bool,
}

struct PickerResult {
    token: u64,
    library: String,
    generation: u64,
    paths: Vec<PathBuf>,
}

pub struct MediaUi {
    sequence: u64,
    observed: Option<DocumentKey>,
    dirty_since: Option<Instant>,
    resolving: Vec<(u64, DocumentKey, ResolveControl)>,
    current_resolve: Option<u64>,
    insertion: Option<Insertion>,
    adoption: Option<Adoption>,
    picker: Option<u64>,
    picker_sender: mpsc::Sender<PickerResult>,
    picker_results: mpsc::Receiver<PickerResult>,
    last_missing: Vec<String>,
}

impl MediaUi {
    pub fn new() -> Self {
        let (picker_sender, picker_results) = mpsc::channel();
        Self {
            sequence: 0,
            observed: None,
            dirty_since: None,
            resolving: Vec::new(),
            current_resolve: None,
            insertion: None,
            adoption: None,
            picker: None,
            picker_sender,
            picker_results,
            last_missing: Vec::new(),
        }
    }
    fn next(&mut self) -> u64 {
        self.sequence = self.sequence.wrapping_add(1).max(1);
        self.sequence
    }
    pub fn busy(&self) -> bool {
        self.insertion.is_some() || self.adoption.is_some() || self.picker.is_some()
    }
    fn observe(&mut self, key: Option<DocumentKey>) {
        if self.observed != key {
            for (_, _, control) in &self.resolving {
                control.cancel();
            }
            self.current_resolve = None;
            self.observed = key;
            self.dirty_since = Some(Instant::now());
        }
    }
    fn resolution_available(&self) -> bool {
        self.resolving.len() < 2 && self.current_resolve.is_none()
    }
}

pub(crate) fn asset_group(note_id: Option<&str>, _content: &str) -> String {
    if let Some(id) =
        note_id.filter(|id| uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == *id))
    {
        return id.to_owned();
    }
    crate::media::new_group()
}

fn clipboard_input(encoded: String) -> anyhow::Result<MediaInput> {
    anyhow::ensure!(
        encoded.len() <= crate::media::MAX_IMAGE_BYTES.div_ceil(3) * 4,
        "图片超过 10 MiB"
    );
    anyhow::ensure!(!encoded.is_empty(), "剪贴板图片为空");
    // Decoding and format/dimension validation run on the media worker.
    Ok(MediaInput::Encoded(encoded))
}

fn image_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|extension| {
            ["png", "jpg", "jpeg", "gif", "webp", "bmp"]
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

impl StudyApp {
    pub fn media_busy(&self) -> bool {
        self.media_ui.busy()
    }

    /// Storage migration must wait for all jobs that can read/write media.
    /// Preview resolution remains nonblocking for ordinary editor controls.
    pub fn media_jobs_active(&self) -> bool {
        self.media_ui.busy() || !self.media_ui.resolving.is_empty()
    }

    fn media_key(&self, cx: &App) -> Option<DocumentKey> {
        if self.page != Page::Notes || self.last_library.is_empty() {
            return None;
        }
        let snapshot = self.snapshot.as_ref()?;
        if self.note_editor.is_none() && self.reader_key.is_empty() {
            return None;
        }
        let source_id = self
            .note_editor
            .as_ref()
            .and_then(|draft| draft.note_id.as_ref())
            .or(self.selected_note.as_ref());
        // An unsaved new draft has no source directory. Retained references use
        // studyapp-media, so resolving relative paths against another note is wrong.
        let source = if self
            .note_editor
            .as_ref()
            .is_some_and(|draft| draft.note_id.is_none())
        {
            None
        } else {
            snapshot
                .notes
                .iter()
                .find(|note| Some(&note.id) == source_id)
                .map(|note| PathBuf::from(&note.path))
        };
        let editor = self.note_content.read(cx);
        Some(DocumentKey {
            library: self.last_library.clone(),
            generation: editor.generation(),
            revision: editor.revision(),
            source,
            settings: snapshot.settings.images.clone(),
        })
    }

    fn send_media(
        &mut self,
        library: String,
        request: MediaRequest,
        cx: &mut Context<Self>,
    ) -> bool {
        if self
            .worker
            .sender
            .send(Work::Scoped {
                library,
                work: Box::new(Work::Media(request)),
            })
            .is_err()
        {
            self.push_notice(Err("图片处理服务已停止，当前笔记仍保留。".into()), cx);
            return false;
        }
        true
    }

    pub fn poll_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Ok(result) = self.media_ui.picker_results.try_recv() {
            if self.media_ui.picker != Some(result.token) {
                continue;
            }
            self.media_ui.picker = None;
            if result.library == self.last_library
                && result.generation == self.note_content.read(cx).generation()
                && !result.paths.is_empty()
            {
                self.import_dropped_images(result.paths, window, cx);
            }
            cx.notify();
        }
        let key = self.media_key(cx);
        self.media_ui.observe(key);
        if self.media_ui.resolution_available()
            && !self.storage_busy
            && !self.preferences_ui.replacing
            && self
                .media_ui
                .dirty_since
                .is_some_and(|at| at.elapsed() >= Duration::from_millis(200))
            && let Some(key) = self.media_ui.observed.clone()
        {
            let request = self.media_ui.next();
            let control = ResolveControl::default();
            let job = MediaRequest::Resolve {
                control: control.clone(),
                generation: key.generation,
                request,
                content: self.note_content.read(cx).value().to_owned(),
                note_path: key.source.clone(),
                settings: key.settings.clone(),
            };
            self.media_ui.dirty_since = None;
            if self.send_media(key.library.clone(), job, cx) {
                self.media_ui.resolving.push((request, key, control));
                self.media_ui.current_resolve = Some(request);
            }
        }
        self.send_next_image(cx);
    }

    fn can_insert_image(&self, cx: &App) -> bool {
        self.page == Page::Notes
            && self.note_editor.is_some()
            && !self.note_saving
            && !self.storage_busy
            && self.confirmation.is_none()
            && !self.has_modal(cx)
            && self.note_content.read(cx).editable()
    }

    pub fn request_insert_image(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_insert_image(cx) || self.media_busy() {
            self.push_notice(
                Err("请先打开可编辑的草稿，并等待当前图片操作完成。".into()),
                cx,
            );
            return;
        }
        let token = self.media_ui.next();
        self.media_ui.picker = Some(token);
        let sender = self.media_ui.picker_sender.clone();
        let library = self.last_library.clone();
        let generation = self.note_content.read(cx).generation();
        std::thread::spawn(move || {
            let paths = rfd::FileDialog::new()
                .set_title("插入图片")
                .add_filter("图片", &["png", "jpg", "jpeg", "gif", "webp", "bmp"])
                .pick_files()
                .unwrap_or_default();
            let _ = sender.send(PickerResult {
                token,
                library,
                generation,
                paths,
            });
        });
        cx.notify();
    }

    pub fn paste_editor_image(
        &mut self,
        _name: String,
        _mime: String,
        encoded: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_insert_image(cx) || self.media_busy() {
            self.push_notice(Err("请等待当前图片操作完成后再粘贴。".into()), cx);
            return;
        }
        match clipboard_input(encoded) {
            Ok(input) => self.queue_image_inputs(VecDeque::from([input]), cx),
            Err(error) => self.push_notice(Err(format!("无法粘贴图片：{error}")), cx),
        }
    }

    /// Returning false leaves non-image payloads to normal Markdown import.
    pub fn import_dropped_images(
        &mut self,
        paths: Vec<PathBuf>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if paths.is_empty() || !paths.iter().all(|path| image_path(path)) {
            return false;
        }
        if !self.can_insert_image(cx) || self.media_busy() {
            self.push_notice(
                Err("请先打开可编辑的草稿，并等待当前图片操作完成后插入图片。".into()),
                cx,
            );
            return true;
        }
        if paths.len() > MAX_FILES {
            self.push_notice(Err(format!("每次最多插入 {MAX_FILES} 张图片。")), cx);
            return true;
        }
        self.queue_image_inputs(paths.into_iter().map(MediaInput::Path).collect(), cx);
        true
    }

    fn queue_image_inputs(&mut self, inputs: VecDeque<MediaInput>, cx: &mut Context<Self>) {
        let Some(draft) = self.note_editor.as_mut() else {
            return;
        };
        if draft.asset_group.is_empty() {
            draft.asset_group = asset_group(draft.note_id.as_deref(), &draft.content);
        }
        let group = draft.asset_group.clone();
        let settings = self
            .snapshot
            .as_ref()
            .map(|s| s.settings.images.clone())
            .unwrap_or_default();
        self.media_ui.insertion = Some(Insertion {
            library: self.last_library.clone(),
            generation: self.note_content.read(cx).generation(),
            group,
            title: self.note_title.read(cx).value().to_string(),
            settings,
            inputs,
            request: None,
        });
        self.capture_note_draft(cx);
        self.send_next_image(cx);
        cx.notify();
    }

    fn send_next_image(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.media_ui.insertion.as_ref() else {
            return;
        };
        if pending.library != self.last_library
            || pending.generation != self.note_content.read(cx).generation()
            || self.note_editor.is_none()
        {
            self.media_ui.insertion = None;
            return;
        }
        if pending.request.is_some() {
            return;
        }
        let request = self.media_ui.next();
        let pending = self.media_ui.insertion.as_mut().unwrap();
        let Some(input) = pending.inputs.pop_front() else {
            self.media_ui.insertion = None;
            cx.notify();
            return;
        };
        pending.request = Some(request);
        let library = pending.library.clone();
        let job = MediaRequest::Import {
            generation: pending.generation,
            request,
            group: pending.group.clone(),
            title: pending.title.clone(),
            input,
            settings: pending.settings.clone(),
        };
        if !self.send_media(library, job, cx) {
            self.media_ui.insertion = None;
        }
    }

    pub(crate) fn begin_media_adoption(
        &mut self,
        draft: NoteDraft,
        source_path: Option<PathBuf>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_media_adoption(draft, source_path, false, cx);
    }

    pub(crate) fn begin_save_images(
        &mut self,
        copy: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut draft) = self.note_editor.clone() else {
            return;
        };
        let source_path = self.snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .notes
                .iter()
                .find(|note| Some(&note.id) == draft.note_id.as_ref())
                .map(|note| PathBuf::from(&note.path))
        });
        if copy {
            draft.note_id = None;
            draft.expected_hash = None;
            draft.asset_group = crate::media::new_group();
        } else if draft.note_id.is_some() || draft.asset_group.is_empty() {
            draft.asset_group = asset_group(draft.note_id.as_deref(), &draft.content);
        }
        self.start_media_adoption(draft, source_path, true, cx);
    }

    fn start_media_adoption(
        &mut self,
        draft: NoteDraft,
        source_path: Option<PathBuf>,
        commit_after_adoption: bool,
        cx: &mut Context<Self>,
    ) {
        if self.media_busy() {
            return;
        }
        let request = self.media_ui.next();
        let generation = self.note_content.read(cx).generation();
        let settings = self
            .snapshot
            .as_ref()
            .map(|s| s.settings.images.clone())
            .unwrap_or_default();
        let job = MediaRequest::Adopt {
            generation,
            request,
            group: draft.asset_group.clone(),
            title: draft.title.clone(),
            content: draft.content.clone(),
            source_path,
            settings,
        };
        let library = self.last_library.clone();
        if self.send_media(library.clone(), job, cx) {
            self.media_ui.adoption = Some(Adoption {
                library,
                generation,
                request,
                selected_note: self.selected_note.clone(),
                selected_version: self.selected_version.clone(),
                draft,
                commit_after_adoption,
            });
            self.push_notice(
                Ok(if commit_after_adoption {
                    "正在保存笔记图片，请稍候…"
                } else {
                    "正在为副本保存图片，请稍候…"
                }
                .into()),
                cx,
            );
            cx.notify();
        } else if commit_after_adoption {
            self.note_saving = false;
            self.note_content
                .update(cx, |editor, cx| editor.set_readonly(false, cx));
        }
    }

    fn warn_missing_images(&mut self, missing: Vec<String>, copying: bool, cx: &mut Context<Self>) {
        if !missing.is_empty() && (copying || missing != self.media_ui.last_missing) {
            let prefix = if copying {
                "部分图片未能保存到资料库，原引用已保留"
            } else {
                "部分图片无法显示"
            };
            self.push_notice(
                Err(format!(
                    "{prefix}（{} 项）：{}",
                    missing.len(),
                    missing
                        .iter()
                        .take(3)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("；")
                )),
                cx,
            );
        }
        self.media_ui.last_missing = missing;
    }

    pub fn accept_media_result(
        &mut self,
        response: MediaResponse,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self
            .media_ui
            .resolving
            .iter()
            .position(|(request, key, _)| {
                *request == response.request
                    && key.library == response.library
                    && key.generation == response.generation
            })
        {
            let (_, key, control) = self.media_ui.resolving.remove(index);
            if self.media_ui.current_resolve != Some(response.request) {
                return;
            }
            self.media_ui.current_resolve = None;
            if control.is_cancelled() || self.media_key(cx).as_ref() != Some(&key) {
                return;
            }
            match response.result {
                Ok(MediaOutcome::Resolved { images, missing }) => {
                    self.note_content.update(cx, |editor, cx| {
                        editor.set_images(response.generation, images, cx)
                    });
                    self.warn_missing_images(missing, false, cx);
                }
                Err(error) => self.push_notice(Err(format!("图片预览失败：{error}")), cx),
                _ => {}
            }
            return;
        }
        if self.media_ui.adoption.as_ref().is_some_and(|pending| {
            pending.request == response.request
                && pending.library == response.library
                && pending.generation == response.generation
        }) {
            let mut pending = self.media_ui.adoption.take().unwrap();
            if pending.library != self.last_library
                || pending.generation != self.note_content.read(cx).generation()
                || self.note_editor.is_some() != pending.commit_after_adoption
                || pending.selected_note != self.selected_note
                || pending.selected_version != self.selected_version
            {
                cx.notify();
                return;
            }
            match response.result {
                Ok(MediaOutcome::Adopted { content, missing }) => {
                    pending.draft.content = content;
                    self.start_note_draft(pending.draft, window, cx);
                    if missing.is_empty() && !pending.commit_after_adoption {
                        self.push_notice(
                            Ok("已创建可编辑副本，图片已保存到当前资料库。".into()),
                            cx,
                        );
                    }
                    self.warn_missing_images(missing, true, cx);
                    if pending.commit_after_adoption {
                        self.commit_note(cx);
                    }
                }
                Err(error) => {
                    if pending.commit_after_adoption {
                        self.note_saving = false;
                        self.note_content
                            .update(cx, |editor, cx| editor.set_readonly(false, cx));
                    }
                    self.push_notice(
                        Err(format!("无法保存笔记图片：{error}；原笔记和草稿已保留。")),
                        cx,
                    );
                }
                _ => {}
            }
            cx.notify();
            return;
        }
        if self.media_ui.insertion.as_ref().is_some_and(|pending| {
            pending.request == Some(response.request)
                && pending.library == response.library
                && pending.generation == response.generation
        }) {
            let pending = self.media_ui.insertion.as_mut().unwrap();
            pending.request = None;
            if response.library != self.last_library
                || response.generation != self.note_content.read(cx).generation()
                || self.note_editor.is_none()
            {
                self.media_ui.insertion = None;
                cx.notify();
                return;
            }
            match response.result {
                Ok(MediaOutcome::Inserted { markdown }) => {
                    let result = self.note_content.update(cx, |editor, cx| {
                        editor.insert_markdown(response.generation, &markdown, cx)
                    });
                    if let Err(error) = result {
                        self.push_notice(Err(error.to_string()), cx);
                    }
                }
                Err(error) => self.push_notice(Err(format!("图片插入失败：{error}")), cx),
                _ => {}
            }
            self.send_next_image(cx);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{asset_group, clipboard_input};
    #[test]
    fn document_switch_cancels_old_preview_but_keeps_two_tracked_slots() {
        use super::{DocumentKey, MediaUi, ResolveControl};
        let key = |generation| DocumentKey {
            library: "library".into(),
            generation,
            revision: 0,
            source: None,
            settings: Default::default(),
        };
        let mut ui = MediaUi::new();
        ui.observe(Some(key(1)));
        let first = ResolveControl::default();
        ui.resolving.push((1, key(1), first.clone()));
        ui.current_resolve = Some(1);
        ui.observe(Some(key(2)));
        assert!(first.is_cancelled());
        assert!(
            ui.resolution_available(),
            "new local preview can start while old network drains"
        );
        let second = ResolveControl::default();
        ui.resolving.push((2, key(2), second.clone()));
        ui.current_resolve = Some(2);
        ui.observe(Some(key(3)));
        ui.observe(Some(key(4)));
        assert!(second.is_cancelled());
        assert!(
            !ui.resolution_available(),
            "rapid switches must not create unbounded workers"
        );
        assert_eq!(
            ui.resolving.len(),
            2,
            "cancelled storage jobs remain tracked until completion"
        );
        ui.resolving.remove(0);
        assert!(ui.resolution_available());
        assert_eq!(
            ui.observed.unwrap().generation,
            4,
            "only newest pending document is retained"
        );
    }
    #[test]
    fn clipboard_rejects_oversized_payload_before_queueing_for_decode() {
        assert!(
            clipboard_input("A".repeat(crate::media::MAX_IMAGE_BYTES.div_ceil(3) * 4 + 1)).is_err()
        );
        assert!(clipboard_input(String::new()).is_err());
        assert!(
            matches!(clipboard_input("AQID".into()).unwrap(), crate::media::MediaInput::Encoded(value) if value == "AQID")
        );
    }
    #[test]
    fn note_identity_owns_assets_regardless_of_markdown_content() {
        let id = "36b79f08-5128-43ab-a1cb-13f318c455f7";
        assert_eq!(asset_group(Some(id), ""), id);
        assert_ne!(
            asset_group(None, &format!("![图](studyapp-media:{id}/file.png)")),
            id
        );
        let own = "8529f643-e153-4a0b-983b-c32bb6d28da7";
        assert_eq!(
            asset_group(Some(own), &format!("![图](studyapp-media:{id}/file.png)")),
            own
        );
        assert_eq!(
            asset_group(
                Some(own),
                &format!("```\nstudyapp-media:{id}/file.png\n```")
            ),
            own
        );
        assert_ne!(asset_group(None, ""), asset_group(None, ""));
    }
}
