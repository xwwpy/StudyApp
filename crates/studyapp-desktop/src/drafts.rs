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
    pub title: String,
    pub content: String,
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
    std::fs::rename(next, path)?;
    Ok(())
}

enum Request {
    Load(PathBuf),
    Save(PathBuf, Option<NoteDraft>),
    Stop,
}

pub enum DraftUpdate {
    Loaded(PathBuf, Option<NoteDraft>),
    Error(PathBuf, String),
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
                    Ok(Request::Load(path)) => {
                        flush(&mut pending);
                        let result = (|| -> anyhow::Result<Option<NoteDraft>> {
                            match std::fs::metadata(&path) {
                                Ok(meta) => anyhow::ensure!(
                                    meta.len() <= 12 * 1024 * 1024,
                                    "草稿超过大小限制"
                                ),
                                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                    return Ok(None);
                                }
                                Err(e) => return Err(e.into()),
                            }
                            Ok(serde_json::from_slice(&std::fs::read(&path)?)?)
                        })();
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
    fn draft_survives_restart_and_clear_does_not_delete_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note-draft.json");
        let draft = NoteDraft {
            note_id: Some("note-id".into()),
            expected_hash: Some("version-hash".into()),
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
        let directory = tempfile::tempdir().unwrap();
        let first = directory
            .path()
            .join("first/studyapp.sqlite3")
            .to_string_lossy()
            .into_owned();
        let second = directory
            .path()
            .join("second/studyapp.sqlite3")
            .to_string_lossy()
            .into_owned();
        let draft = |content: &str| NoteDraft {
            note_id: None,
            expected_hash: None,
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
        let directory = tempfile::tempdir().unwrap();
        let library = directory
            .path()
            .join("studyapp.sqlite3")
            .to_string_lossy()
            .into_owned();
        let store = DraftStore::new();
        store.save(
            &library,
            Some(NoteDraft {
                note_id: None,
                expected_hash: None,
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
        let directory = tempfile::tempdir().unwrap();
        let library = directory
            .path()
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
}
