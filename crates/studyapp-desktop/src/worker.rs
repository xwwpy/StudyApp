use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
use studyapp_core::{Command, Engine, Snapshot};

pub enum Work {
    Apply(Command),
    SwitchLibrary(PathBuf),
    Shutdown,
}

pub struct Update {
    pub snapshot: Option<Arc<Snapshot>>,
    pub notice: Option<Result<String, String>>,
    pub editor_result: bool,
    pub library_replaced: bool,
}

pub struct Worker {
    pub sender: mpsc::Sender<Work>,
    pub receiver: mpsc::Receiver<Update>,
    handle: Option<thread::JoinHandle<()>>,
}

fn open_library(path: &std::path::Path, now: i64) -> anyhow::Result<(std::fs::File, Engine)> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))?;
    lock.try_lock()
        .map_err(|_| anyhow::anyhow!("该资料库正在另一个 StudyApp 窗口中使用，请先关闭那个窗口"))?;
    let engine = Engine::open(path, now)?;
    Ok((lock, engine))
}

impl Worker {
    pub fn start(path: PathBuf, location_config: Option<PathBuf>) -> Self {
        let (sender, commands) = mpsc::channel();
        let (updates, receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut library_lock, mut engine) =
                match open_library(&path, chrono::Utc::now().timestamp()) {
                    Ok(library) => library,
                    Err(error) => {
                        let _ = updates.send(Update {
                            snapshot: None,
                            notice: Some(Err(format!("无法打开资料库：{error:#}"))),
                            editor_result: false,
                            library_replaced: false,
                        });
                        return;
                    }
                };
            let _ = updates.send(Update {
                snapshot: Some(Arc::new(engine.snapshot())),
                notice: None,
                editor_result: false,
                library_replaced: false,
            });
            let mut last_tick = Instant::now();
            let mut last_scan = Instant::now();
            let mut last_checkpoint = Instant::now();
            loop {
                let work = commands.recv_timeout(Duration::from_millis(150));
                let now = chrono::Utc::now().timestamp();
                let mut notice = None;
                let mut changed = false;
                let mut editor_result = false;
                let mut library_replaced = false;
                match work {
                    Ok(Work::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        if let Err(error) = engine.checkpoint() {
                            eprintln!("保存退出状态失败：{error:#}");
                        }
                        break;
                    }
                    Ok(Work::Apply(command)) => {
                        editor_result = matches!(
                            &command,
                            Command::Submit { .. }
                                | Command::Manual { .. }
                                | Command::EditSession { .. }
                        );
                        let restoring = matches!(&command, Command::Restore(_));
                        let result = engine.apply(command, now).map_err(|e| format!("{e:#}"));
                        library_replaced = restoring && result.is_ok();
                        notice = Some(result);
                        changed = true;
                    }
                    Ok(Work::SwitchLibrary(directory)) => {
                        let result = (|| -> anyhow::Result<String> {
                            engine.checkpoint()?;
                            std::fs::create_dir_all(&directory)?;
                            let (next_lock, next) =
                                open_library(&directory.join("studyapp.sqlite3"), now)?;
                            if let Some(config) = &location_config {
                                if let Some(parent) = config.parent() {
                                    std::fs::create_dir_all(parent)?;
                                }
                                std::fs::write(config, serde_json::to_vec(&directory)?)?;
                            }
                            engine = next;
                            library_lock = next_lock;
                            last_tick = Instant::now();
                            Ok("已切换资料库，原资料仍保存在原目录".into())
                        })();
                        library_replaced = result.is_ok();
                        notice = Some(result.map_err(|e| format!("{e:#}")));
                        changed = true;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                // Polling uses content hashes and the core's stability debounce.
                // All reads happen off the UI thread, including the boundary scan.
                if last_scan.elapsed() >= Duration::from_secs(3) {
                    if let Err(e) = engine.apply(Command::Scan { offline: false }, now) {
                        notice = Some(Err(format!("笔记扫描失败：{e:#}")));
                    }
                    last_scan = Instant::now();
                    changed = true;
                }
                let elapsed = last_tick.elapsed().as_secs();
                if elapsed >= 1 {
                    if let Err(e) = engine.tick(now, elapsed) {
                        notice = Some(Err(format!("计时保存失败：{e:#}")));
                    }
                    last_tick += Duration::from_secs(elapsed);
                    changed = true;
                }
                if last_checkpoint.elapsed() >= Duration::from_secs(5) {
                    if let Err(e) = engine.checkpoint() {
                        notice = Some(Err(format!("资料保存失败：{e:#}")));
                    }
                    last_checkpoint = Instant::now();
                }
                if changed
                    && updates
                        .send(Update {
                            snapshot: Some(Arc::new(engine.snapshot())),
                            notice,
                            editor_result,
                            library_replaced,
                        })
                        .is_err()
                {
                    break;
                }
            }
            drop(library_lock);
        });
        Self {
            sender,
            receiver,
            handle: Some(handle),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.sender.send(Work::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receive_notice(worker: &Worker) -> Update {
        loop {
            let update = worker
                .receiver
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
            if update.notice.is_some() {
                return update;
            }
            assert!(!update.library_replaced);
        }
    }

    fn apply(worker: &Worker, command: Command) -> Update {
        worker.sender.send(Work::Apply(command)).unwrap();
        receive_notice(worker)
    }

    #[test]
    fn restore_same_library_marks_success_and_preserves_data_on_failure() {
        let directory = tempfile::tempdir().unwrap().keep();
        let worker = Worker::start(directory.join("study.sqlite3"), None);
        let initial = worker
            .receiver
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        assert!(initial.snapshot.is_some());
        assert!(!initial.library_replaced);
        let settings = studyapp_core::Settings {
            interval_minutes: 5,
            ..Default::default()
        };
        let updated = apply(&worker, Command::SaveSettings(settings));
        assert!(updated.notice.unwrap().is_ok());
        assert!(!updated.library_replaced);
        let now = chrono::Utc::now().timestamp();
        let saved = apply(
            &worker,
            Command::Manual {
                version_ids: None,
                started_at: now - 120,
                minutes: 1,
                summary: "备份中的学习".into(),
                note_ids: vec![],
            },
        );
        assert!(saved.notice.unwrap().is_ok());
        assert!(!saved.library_replaced);
        let original_id = saved.snapshot.unwrap().sessions[0].id.clone();
        let backup = directory.join("saved.sqlite3");
        let backed_up = apply(&worker, Command::Backup(backup.clone()));
        assert!(backed_up.notice.unwrap().is_ok());
        assert!(!backed_up.library_replaced);
        let updated = apply(
            &worker,
            Command::SaveSettings(studyapp_core::Settings {
                interval_minutes: 10,
                ..Default::default()
            }),
        );
        assert!(updated.notice.unwrap().is_ok());
        assert!(!updated.library_replaced);
        let extra = apply(
            &worker,
            Command::Manual {
                version_ids: None,
                started_at: now - 240,
                minutes: 1,
                summary: "备份以后的学习".into(),
                note_ids: vec![],
            },
        );
        assert_eq!(extra.snapshot.unwrap().sessions.len(), 2);
        assert!(!extra.library_replaced);
        let restored = apply(&worker, Command::Restore(backup));
        assert!(restored.notice.unwrap().is_ok());
        assert!(restored.library_replaced);
        let snapshot = restored.snapshot.unwrap();
        assert_eq!(snapshot.settings.interval_minutes, 5);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].id, original_id);
        assert_eq!(snapshot.sessions[0].summary, "备份中的学习");
        let failed = apply(&worker, Command::Restore(directory.join("missing.sqlite3")));
        assert!(failed.notice.unwrap().is_err());
        assert!(!failed.library_replaced);
        let unchanged = failed.snapshot.unwrap();
        assert_eq!(unchanged.data_path, snapshot.data_path);
        assert_eq!(unchanged.settings.interval_minutes, 5);
        assert_eq!(unchanged.sessions.len(), 1);
        assert_eq!(unchanged.sessions[0].id, original_id);
    }

    #[test]
    fn library_switch_marks_only_successful_replacements() {
        let directory = tempfile::tempdir().unwrap().keep();
        let worker = Worker::start(directory.join("original.sqlite3"), None);
        let initial = worker
            .receiver
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        assert!(!initial.library_replaced);
        let original_path = initial.snapshot.unwrap().data_path.clone();
        let invalid_directory = directory.join("file.txt");
        std::fs::write(&invalid_directory, "preserve").unwrap();
        worker
            .sender
            .send(Work::SwitchLibrary(invalid_directory))
            .unwrap();
        let failed = receive_notice(&worker);
        assert!(failed.notice.unwrap().is_err());
        assert!(!failed.library_replaced);
        assert_eq!(failed.snapshot.unwrap().data_path, original_path);
        worker
            .sender
            .send(Work::SwitchLibrary(directory.join("next")))
            .unwrap();
        let switched = receive_notice(&worker);
        assert!(switched.notice.unwrap().is_ok());
        assert!(switched.library_replaced);
        assert_ne!(switched.snapshot.unwrap().data_path, original_path);
    }

    #[test]
    fn worker_reports_validation_errors_and_persists_before_shutdown() {
        let directory = tempfile::tempdir().unwrap().keep();
        let path = directory.join("study.sqlite3");
        let worker = Worker::start(path.clone(), None);
        assert!(
            worker
                .receiver
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .snapshot
                .is_some()
        );
        let invalid = apply(
            &worker,
            Command::Manual {
                version_ids: None,
                started_at: chrono::Utc::now().timestamp() - 600,
                minutes: 0,
                summary: "无效时长".into(),
                note_ids: vec![],
            },
        );
        assert!(invalid.notice.unwrap().is_err());
        assert!(invalid.editor_result);
        assert!(!invalid.library_replaced);
        assert!(invalid.snapshot.unwrap().sessions.is_empty());
        worker
            .sender
            .send(Work::Apply(Command::Manual {
                version_ids: None,
                started_at: chrono::Utc::now().timestamp() - 600,
                minutes: 10,
                summary: "复习所有权".into(),
                note_ids: vec![],
            }))
            .unwrap();
        loop {
            let result = worker
                .receiver
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
            if let Some(notice) = result.notice {
                assert!(notice.is_ok());
                break;
            }
        }
        drop(worker);
        let engine = Engine::open(&path, chrono::Utc::now().timestamp()).unwrap();
        assert_eq!(engine.snapshot().sessions.len(), 1);
    }

    #[test]
    fn a_second_worker_cannot_overwrite_an_open_library() {
        let directory = tempfile::tempdir().unwrap().keep();
        let path = directory.join("study.sqlite3");
        let first = Worker::start(path.clone(), None);
        assert!(
            first
                .receiver
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .snapshot
                .is_some()
        );
        let second = Worker::start(path, None);
        let result = second
            .receiver
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        assert!(result.snapshot.is_none());
        assert!(!result.library_replaced);
        assert!(result.notice.unwrap().unwrap_err().contains("另一个"));
    }
}
