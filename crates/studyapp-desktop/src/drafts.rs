use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteDraft {
    pub note_id: Option<String>,
    pub expected_hash: Option<String>,
    #[serde(default)]
    pub folder_id: Option<String>,
    pub title: String,
    pub content: String,
}

fn read_draft(path: &Path) -> anyhow::Result<Option<NoteDraft>> {
    use std::io::Read;
    // A valid 10 MiB Markdown can expand to 60 MiB of JSON escapes.
    const MAX_JSON_BYTES: u64 = 64 * 1024 * 1024;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(file.metadata()?.len() <= MAX_JSON_BYTES, "草稿超过大小限制");
    let mut bytes = Vec::new();
    file.take(MAX_JSON_BYTES + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() as u64 <= MAX_JSON_BYTES, "草稿超过大小限制");
    let draft: Option<NoteDraft> = serde_json::from_slice(&bytes)?;
    anyhow::ensure!(
        draft
            .as_ref()
            .is_none_or(|d| d.content.len() <= 10 * 1024 * 1024),
        "Markdown 超过 10 MiB"
    );
    Ok(draft)
}

fn write_draft(path: &Path, draft: Option<&NoteDraft>) -> anyhow::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let next = path.with_extension(format!("{}.{}.next", std::process::id(), nonce));
    let bytes = serde_json::to_vec(&draft)?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&next)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(next, path)?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

enum Request {
    Load(PathBuf),
    Save(PathBuf, Option<NoteDraft>),
    Discard(PathBuf, u64),
    Persist(PathBuf, Option<NoteDraft>, u64),
    Stop,
}

pub enum DraftUpdate {
    Loaded(PathBuf, Option<NoteDraft>),
    Error(PathBuf, String),
    Discarded(PathBuf, u64),
    DiscardFailed(PathBuf, u64, String),
    Persisted(PathBuf, u64),
    PersistFailed(PathBuf, u64, String),
}

/// A single serial writer prevents delayed writes from reviving a saved draft.
/// Shutdown drains the latest debounce before joining; no files are deleted.
pub struct DraftStore {
    sender: mpsc::Sender<Request>,
    pub receiver: mpsc::Receiver<DraftUpdate>,
    thread: Option<thread::JoinHandle<()>>,
}

impl DraftStore {
    pub fn new() -> Self {
        let (sender, requests) = mpsc::channel();
        let (updates, receiver) = mpsc::channel();
        let thread = thread::spawn(move || {
            let mut pending: Option<(PathBuf, Option<NoteDraft>)> = None;
            let flush = |pending: &mut Option<(PathBuf, Option<NoteDraft>)>| {
                if let Some((path, draft)) = pending.take()
                    && let Err(error) = write_draft(&path, draft.as_ref())
                {
                    let _ = updates.send(DraftUpdate::Error(
                        path,
                        format!("草稿保存失败，请保留编辑器并重试：{error:#}"),
                    ));
                }
            };
            loop {
                match requests.recv_timeout(Duration::from_millis(650)) {
                    Ok(Request::Save(path, draft)) => {
                        if pending.as_ref().is_some_and(|(old, _)| old != &path) {
                            flush(&mut pending);
                        }
                        let clear = draft.is_none();
                        pending = Some((path, draft));
                        if clear {
                            flush(&mut pending);
                        }
                    }
                    Ok(Request::Discard(path, token)) => {
                        if pending.as_ref().is_some_and(|(old, _)| old != &path) {
                            flush(&mut pending);
                        } else {
                            // Discard supersedes the same-library debounce even
                            // if publishing null fails. The UI retains recovery
                            // content until this request is acknowledged.
                            pending = None;
                        }
                        let update = match write_draft(&path, None) {
                            Ok(()) => DraftUpdate::Discarded(path, token),
                            Err(error) => DraftUpdate::DiscardFailed(
                                path,
                                token,
                                format!("草稿删除失败，恢复内容仍保留，请重试：{error:#}"),
                            ),
                        };
                        let _ = updates.send(update);
                    }
                    Ok(Request::Persist(path, draft, token)) => {
                        if pending.as_ref().is_some_and(|(old, _)| old != &path) {
                            flush(&mut pending);
                        } else {
                            pending = None;
                        }
                        let update = match write_draft(&path, draft.as_ref()) {
                            Ok(()) => DraftUpdate::Persisted(path, token),
                            Err(error) => DraftUpdate::PersistFailed(
                                path,
                                token,
                                format!("草稿保存失败，请保留当前内容并重试：{error:#}"),
                            ),
                        };
                        let _ = updates.send(update);
                    }
                    Ok(Request::Load(path)) => {
                        flush(&mut pending);
                        let result = read_draft(&path);
                        let update = match result {
                            Ok(draft) => DraftUpdate::Loaded(path, draft),
                            Err(e) => DraftUpdate::Error(
                                path,
                                format!("无法恢复草稿，原文件仍保留：{e:#}"),
                            ),
                        };
                        let _ = updates.send(update);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => flush(&mut pending),
                    Ok(Request::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        flush(&mut pending);
                        break;
                    }
                }
            }
        });
        Self {
            sender,
            receiver,
            thread: Some(thread),
        }
    }

    pub fn path(library: &str) -> PathBuf {
        Path::new(library)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("note-draft.json")
    }
    pub fn load(&self, library: &str) {
        let _ = self.sender.send(Request::Load(Self::path(library)));
    }
    pub fn save(&self, library: &str, draft: Option<NoteDraft>) {
        if !library.is_empty() {
            let _ = self.sender.send(Request::Save(Self::path(library), draft));
        }
    }
    pub fn discard(&self, library: &str, token: u64) {
        if !library.is_empty() {
            let _ = self
                .sender
                .send(Request::Discard(Self::path(library), token));
        }
    }
    /// Explicit lifecycle persistence bypasses debounce and acknowledges the
    /// completed write. Callers retain their draft until path and token match.
    pub fn persist(&self, library: &str, draft: Option<NoteDraft>, token: u64) {
        if !library.is_empty() {
            let _ = self
                .sender
                .send(Request::Persist(Self::path(library), draft, token));
        }
    }
}

impl Drop for DraftStore {
    fn drop(&mut self) {
        let _ = self.sender.send(Request::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_markdown_with_json_escapes_can_be_recovered() {
        let directory = tempfile::tempdir().unwrap().keep();
        let library = directory
            .join("studyapp.sqlite3")
            .to_string_lossy()
            .into_owned();
        let draft = NoteDraft {
            note_id: None,
            expected_hash: None,
            folder_id: None,
            title: "转义字符".into(),
            content: "\\".repeat(7 * 1024 * 1024),
        };
        write_draft(&DraftStore::path(&library), Some(&draft)).unwrap();
        let store = DraftStore::new();
        store.load(&library);
        let update = store
            .receiver
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        match update {
            DraftUpdate::Loaded(_, Some(recovered)) => {
                assert_eq!(recovered.content.len(), draft.content.len())
            }
            _ => panic!("valid Markdown must survive JSON escaping on disk"),
        }
    }

    #[test]
    fn draft_survives_restart_and_clear_does_not_delete_files() {
        let directory = tempfile::tempdir().unwrap().keep();
        let path = directory.join("note-draft.json");
        let draft = NoteDraft {
            note_id: Some("note-id".into()),
            expected_hash: Some("version-hash".into()),
            folder_id: None,
            title: "未保存的中文标题".into(),
            content: "# 草稿\n不能丢失的内容".into(),
        };
        write_draft(&path, Some(&draft)).unwrap();
        let recovered: Option<NoteDraft> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(recovered, Some(draft));
        write_draft(&path, None).unwrap();
        assert!(path.is_file());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "null");
    }

    #[test]
    fn shutdown_flushes_latest_draft_and_keeps_libraries_separate() {
        let directory = tempfile::tempdir().unwrap().keep();
        let first = directory
            .join("first/studyapp.sqlite3")
            .to_string_lossy()
            .into_owned();
        let second = directory
            .join("second/studyapp.sqlite3")
            .to_string_lossy()
            .into_owned();
        let draft = |content: &str| NoteDraft {
            note_id: None,
            expected_hash: None,
            folder_id: None,
            title: "草稿".into(),
            content: content.into(),
        };
        let store = DraftStore::new();
        store.save(&first, Some(draft("较早的输入")));
        store.save(&first, Some(draft("最后一次输入")));
        store.save(&second, Some(draft("另一个资料库")));
        drop(store);
        let read = |library: &str| {
            serde_json::from_slice::<Option<NoteDraft>>(
                &std::fs::read(DraftStore::path(library)).unwrap(),
            )
            .unwrap()
            .unwrap()
        };
        assert_eq!(read(&first).content, "最后一次输入");
        assert_eq!(read(&second).content, "另一个资料库");
    }

    #[test]
    fn clearing_a_draft_cannot_be_undone_by_delayed_debounce() {
        let directory = tempfile::tempdir().unwrap().keep();
        let library = directory
            .join("studyapp.sqlite3")
            .to_string_lossy()
            .into_owned();
        let store = DraftStore::new();
        store.save(
            &library,
            Some(NoteDraft {
                note_id: None,
                expected_hash: None,
                folder_id: None,
                title: "旧草稿".into(),
                content: "已保存".into(),
            }),
        );
        store.save(&library, None);
        drop(store);
        assert_eq!(
            std::fs::read_to_string(DraftStore::path(&library)).unwrap(),
            "null"
        );
    }

    #[test]
    fn a_corrupt_draft_reports_error_without_modifying_original_bytes() {
        let directory = tempfile::tempdir().unwrap().keep();
        let library = directory
            .join("studyapp.sqlite3")
            .to_string_lossy()
            .into_owned();
        let path = DraftStore::path(&library);
        std::fs::write(&path, b"{interrupted data").unwrap();
        let store = DraftStore::new();
        store.load(&library);
        assert!(matches!(
            store.receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
            DraftUpdate::Error(_, _)
        ));
        drop(store);
        assert_eq!(std::fs::read(path).unwrap(), b"{interrupted data");
    }

    fn draft(content: &str) -> NoteDraft {
        NoteDraft {
            note_id: None,
            expected_hash: None,
            folder_id: None,
            title: "草稿".into(),
            content: content.into(),
        }
    }

    #[test]
    fn explicit_discard_acknowledges_null_before_restart_without_debounce_resurrection() {
        let directory = tempfile::tempdir().unwrap().keep();
        let library = directory
            .join("study.sqlite")
            .to_string_lossy()
            .into_owned();
        let path = DraftStore::path(&library);
        let store = DraftStore::new();
        store.save(&library, Some(draft("pending unsaved content")));
        store.discard(&library, 73);
        assert!(matches!(
            store.receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            DraftUpdate::Discarded(ref saved_path, 73) if saved_path == &path
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "null");
        drop(store);
        let restarted = DraftStore::new();
        restarted.load(&library);
        assert!(matches!(
            restarted.receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            DraftUpdate::Loaded(ref saved_path, None) if saved_path == &path
        ));
        assert!(path.is_file());
    }

    #[test]
    fn discard_flushes_other_library_pending_save_without_clearing_it() {
        let directory = tempfile::tempdir().unwrap().keep();
        let first = directory
            .join("first/study.sqlite")
            .to_string_lossy()
            .into_owned();
        let second = directory
            .join("second/study.sqlite")
            .to_string_lossy()
            .into_owned();
        let store = DraftStore::new();
        store.save(&first, Some(draft("other library latest draft")));
        store.discard(&second, 41);
        assert!(matches!(
            store.receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            DraftUpdate::Discarded(ref path, 41) if path == &DraftStore::path(&second)
        ));
        let first_draft: Option<NoteDraft> =
            serde_json::from_slice(&std::fs::read(DraftStore::path(&first)).unwrap()).unwrap();
        assert_eq!(first_draft, Some(draft("other library latest draft")));
        assert_eq!(
            std::fs::read_to_string(DraftStore::path(&second)).unwrap(),
            "null"
        );
    }

    #[cfg(windows)]
    #[test]
    fn failed_discard_preserves_original_file_and_never_acknowledges_success() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = tempfile::tempdir().unwrap().keep();
        let library = directory
            .join("study.sqlite")
            .to_string_lossy()
            .into_owned();
        let path = DraftStore::path(&library);
        write_draft(&path, Some(&draft("original recovery content"))).unwrap();
        let original = std::fs::read(&path).unwrap();
        // Deny replacement while permitting the verification read.
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let store = DraftStore::new();
        store.save(&library, Some(draft("pending input must not be published")));
        store.discard(&library, 88);
        assert!(matches!(
            store.receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            DraftUpdate::DiscardFailed(ref failed_path, 88, ref error)
                if failed_path == &path && !error.is_empty()
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        drop(locked);
        store.load(&library);
        assert!(matches!(
            store.receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            DraftUpdate::Loaded(ref loaded_path, Some(ref recovered))
                if loaded_path == &path && recovered == &draft("original recovery content")
        ));
        drop(store);
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn fresh_draft_after_acknowledged_discard_is_saved_normally() {
        let directory = tempfile::tempdir().unwrap().keep();
        let library = directory
            .join("study.sqlite")
            .to_string_lossy()
            .into_owned();
        let store = DraftStore::new();
        store.save(&library, Some(draft("old")));
        store.discard(&library, 9);
        assert!(matches!(
            store.receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            DraftUpdate::Discarded(_, 9)
        ));
        store.save(&library, Some(draft("new draft")));
        drop(store);
        let saved: Option<NoteDraft> =
            serde_json::from_slice(&std::fs::read(DraftStore::path(&library)).unwrap()).unwrap();
        assert_eq!(saved, Some(draft("new draft")));
    }

    #[test]
    fn explicit_persist_supersedes_debounce_and_acknowledges_durable_latest_content() {
        let directory = tempfile::tempdir().unwrap().keep();
        let library = directory
            .join("study.sqlite")
            .to_string_lossy()
            .into_owned();
        let path = DraftStore::path(&library);
        let store = DraftStore::new();
        store.save(&library, Some(draft("older queued input")));
        store.persist(&library, Some(draft("latest flushed input")), 51);
        assert!(matches!(
            store.receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            DraftUpdate::Persisted(ref saved_path, 51) if saved_path == &path
        ));
        let saved: Option<NoteDraft> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved, Some(draft("latest flushed input")));
        drop(store);
        let restarted = DraftStore::new();
        restarted.load(&library);
        assert!(matches!(
            restarted.receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            DraftUpdate::Loaded(ref loaded_path, Some(ref recovered))
                if loaded_path == &path && recovered == &draft("latest flushed input")
        ));
    }

    #[test]
    fn acknowledged_clear_flushes_the_other_library_first() {
        let directory = tempfile::tempdir().unwrap().keep();
        let first = directory
            .join("first/study.sqlite")
            .to_string_lossy()
            .into_owned();
        let second = directory
            .join("second/study.sqlite")
            .to_string_lossy()
            .into_owned();
        let store = DraftStore::new();
        store.save(&first, Some(draft("other library content")));
        store.persist(&second, None, 52);
        assert!(matches!(
            store.receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            DraftUpdate::Persisted(ref path, 52) if path == &DraftStore::path(&second)
        ));
        let saved: Option<NoteDraft> =
            serde_json::from_slice(&std::fs::read(DraftStore::path(&first)).unwrap()).unwrap();
        assert_eq!(saved, Some(draft("other library content")));
        assert_eq!(
            std::fs::read_to_string(DraftStore::path(&second)).unwrap(),
            "null"
        );
    }

    #[cfg(windows)]
    #[test]
    fn failed_persist_preserves_disk_bytes_and_only_emits_tokenized_failure() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory = tempfile::tempdir().unwrap().keep();
        let library = directory
            .join("study.sqlite")
            .to_string_lossy()
            .into_owned();
        let path = DraftStore::path(&library);
        write_draft(&path, Some(&draft("original disk content"))).unwrap();
        let original = std::fs::read(&path).unwrap();
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let store = DraftStore::new();
        store.save(&library, Some(draft("stale debounce")));
        store.persist(&library, Some(draft("latest input")), 53);
        assert!(matches!(
            store.receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            DraftUpdate::PersistFailed(ref failed_path, 53, ref error)
                if failed_path == &path && !error.is_empty()
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        drop(locked);
        store.load(&library);
        assert!(matches!(
            store.receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
            DraftUpdate::Loaded(ref loaded_path, Some(ref recovered))
                if loaded_path == &path && recovered == &draft("original disk content")
        ));
    }
}
