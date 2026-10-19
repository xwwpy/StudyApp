use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use studyapp_core::{Command, Engine, Snapshot};
use studyapp_sync::{
    Baseline, LocalFolderProvider, NotePlan, Provider, SyncAction, SyncNote, WebDavProvider,
    plan_sync,
};
use zeroize::Zeroizing;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncTarget {
    #[default]
    Disabled,
    Folder {
        path: String,
    },
    WebDav {
        url: String,
        username: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSettings {
    pub target: SyncTarget,
    pub auto_minutes: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct SyncStatus {
    pub library: String,
    pub settings: SyncSettings,
    pub busy: bool,
    pub last_success: Option<i64>,
    pub message: String,
    pub credential_saved: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Persisted {
    settings: SyncSettings,
    protected_password: Vec<u8>,
    baselines: BTreeMap<String, Baseline>,
    last_success: Option<i64>,
}

pub struct SyncRuntime {
    path: PathBuf,
    data: Persisted,
    pub status: SyncStatus,
}

pub struct SyncCompletion {
    pub plans: Vec<NotePlan>,
    pub errors: Vec<String>,
    pub test_only: bool,
}

// UUIDv8: deterministic application-defined identity for a downloaded copy.
// Retrying after a config-write failure reuses the same copy without overwriting it.
fn remote_copy_id(source: &str, content: &str) -> String {
    let hash = studyapp_sync::content_hash(&format!(
        "StudyApp/remote-copy/v1/{source}/{}",
        studyapp_sync::content_hash(content)
    ));
    format!(
        "{}-{}-8{}-a{}-{}",
        &hash[..8],
        &hash[8..12],
        &hash[13..16],
        &hash[17..20],
        &hash[20..32]
    )
}

impl SyncRuntime {
    pub fn load(database: &Path) -> Self {
        let path = database.with_file_name("sync-config.json");
        let loaded = (|| -> Result<Persisted> {
            if !path.exists() {
                return Ok(Persisted::default());
            }
            ensure!(
                std::fs::metadata(&path)?.len() <= 8 * 1024 * 1024,
                "同步配置文件超过限制"
            );
            Ok(serde_json::from_slice(&std::fs::read(&path)?)?)
        })();
        let (data, message) = match loaded {
            Ok(data) => (data, "尚未同步；笔记和学习记录始终保存在本机".into()),
            Err(_) => (
                Persisted::default(),
                "同步配置无法读取，请重新配置；原配置文件保留".into(),
            ),
        };
        let status = SyncStatus {
            library: database.to_string_lossy().into_owned(),
            settings: data.settings.clone(),
            last_success: data.last_success,
            credential_saved: !data.protected_password.is_empty(),
            message,
            busy: false,
        };
        Self { path, data, status }
    }

    fn persist(&self) -> Result<()> {
        let temporary = self.path.with_file_name(format!(
            "sync-config-{}.tmp",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let bytes = serde_json::to_vec_pretty(&self.data)?;
        ensure!(
            bytes.len() <= 8 * 1024 * 1024,
            "同步配置超过容量限制；请新建独立同步目标"
        );
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(temporary, &self.path).context("同步配置保存失败")?;
        Ok(())
    }

    pub fn configure(
        &mut self,
        settings: SyncSettings,
        password: Option<Zeroizing<String>>,
    ) -> Result<()> {
        ensure!(!self.status.busy, "同步正在运行，请完成后再修改设置");
        ensure!(
            settings
                .auto_minutes
                .is_none_or(|m| (5..=1440).contains(&m)),
            "自动同步间隔应为 5–1440 分钟"
        );
        let previous = self.data.clone();
        let changed_target = settings.target != self.data.settings.target;
        let encrypted = match &settings.target {
            SyncTarget::WebDav { url, username } => {
                ensure!(!username.trim().is_empty(), "请填写 WebDAV 用户名");
                ensure!(
                    url.len() <= 4096 && username.len() <= 1024,
                    "服务器地址或用户名过长"
                );
                let password = match password {
                    Some(password) => password,
                    None => {
                        ensure!(
                            !changed_target && !self.data.protected_password.is_empty(),
                            "请填写此 WebDAV 服务的密码或应用密码"
                        );
                        crate::credentials::unprotect(&self.data.protected_password)?
                    }
                };
                // Constructor validates URL and TLS policy; it performs no network writes.
                let _ = WebDavProvider::new(url, username, password.as_str())?;
                crate::credentials::protect(&password)?
            }
            SyncTarget::Folder { path } => {
                ensure!(
                    !path.trim().is_empty() && Path::new(path).is_absolute(),
                    "请选择绝对路径的同步文件夹"
                );
                vec![]
            }
            SyncTarget::Disabled => vec![],
        };
        self.data.settings = settings.clone();
        self.data.protected_password = encrypted;
        if changed_target {
            self.data.baselines.clear();
            self.data.last_success = None;
        }
        if let Err(error) = self.persist() {
            self.data = previous;
            return Err(error);
        }
        self.status = SyncStatus {
            library: self.status.library.clone(),
            settings,
            busy: false,
            last_success: self.data.last_success,
            credential_saved: !self.data.protected_password.is_empty(),
            message: "同步设置已保存；可先测试连接，再开始同步".into(),
        };
        Ok(())
    }

    pub fn task(
        &mut self,
        snapshot: Arc<Snapshot>,
        test_only: bool,
    ) -> Result<Box<dyn FnOnce() -> Result<SyncCompletion> + Send>> {
        ensure!(!self.status.busy, "同步已在运行");
        ensure!(
            !matches!(self.data.settings.target, SyncTarget::Disabled),
            "请先配置同步目标"
        );
        let target = self.data.settings.target.clone();
        let secret = if matches!(target, SyncTarget::WebDav { .. }) {
            Some(crate::credentials::unprotect(
                &self.data.protected_password,
            )?)
        } else {
            None
        };
        let baselines = self.data.baselines.clone();
        self.status.busy = true;
        self.status.message = if test_only {
            "正在测试连接…"
        } else {
            "正在比较并同步 Markdown 笔记…"
        }
        .into();
        Ok(Box::new(move || {
            let provider: Box<dyn Provider> = match &target {
                SyncTarget::Disabled => anyhow::bail!("请先配置同步目标"),
                SyncTarget::Folder { path } => Box::new(LocalFolderProvider::new(path)?),
                SyncTarget::WebDav { url, username } => Box::new(WebDavProvider::new(
                    url,
                    username,
                    secret.as_ref().context("缺少 WebDAV 凭据")?.as_str(),
                )?),
            };
            provider.test_connection()?;
            if test_only {
                return Ok(SyncCompletion {
                    plans: vec![],
                    errors: vec![],
                    test_only,
                });
            }
            let notes: Vec<_> = snapshot
                .notes
                .iter()
                .filter_map(|note| {
                    snapshot
                        .versions
                        .iter()
                        .rev()
                        .find(|v| v.note_id == note.id)
                        .map(|version| SyncNote {
                            id: note.id.clone(),
                            title: note.title.clone(),
                            content: version.content.clone(),
                            hash: version.hash.clone(),
                            managed: note.managed,
                        })
                })
                .collect();
            let plans = plan_sync(provider.as_ref(), &notes, &baselines)?;
            let mut completed = vec![];
            let mut errors = vec![];
            for plan in plans {
                if let SyncAction::Push {
                    content,
                    expected_revision,
                } = &plan.action
                    && let Err(error) =
                        provider.write(&plan.id, content, expected_revision.as_deref())
                {
                    errors.push(format!("笔记 {} 上传失败：{}", plan.id, error));
                    continue;
                }
                completed.push(plan);
            }
            Ok(SyncCompletion {
                plans: completed,
                errors,
                test_only,
            })
        }))
    }

    pub fn finish(
        &mut self,
        completion: Result<SyncCompletion, String>,
        engine: &mut Engine,
        now: i64,
    ) -> Result<String> {
        self.status.busy = false;
        let completion = completion.map_err(anyhow::Error::msg)?;
        if completion.test_only {
            self.status.message = "连接测试成功".into();
            return Ok(self.status.message.clone());
        }
        let previous = self.data.clone();
        let mut errors = completion.errors;
        let mut changed = 0;
        let mut conflicts = 0;
        for plan in completion.plans {
            let result = (|| -> Result<()> {
                let snapshot = engine.snapshot();
                let current = snapshot
                    .versions
                    .iter()
                    .rev()
                    .find(|v| v.note_id == plan.id);
                ensure!(
                    current.map(|v| &v.hash) == plan.expected_local_hash.as_ref(),
                    "本地笔记在同步期间改变，已保留修改，请再次同步"
                );
                match &plan.action {
                    SyncAction::Unchanged | SyncAction::Push { .. } => {}
                    SyncAction::Pull { content } | SyncAction::ImportRemote { content } => {
                        engine.apply(
                            Command::ReceiveSyncedNote {
                                note_id: plan.id.clone(),
                                expected_hash: plan.expected_local_hash.clone(),
                                content: content.clone(),
                            },
                            now,
                        )?;
                        changed += 1;
                    }
                    SyncAction::Conflict {
                        local_content,
                        remote_content,
                    } => {
                        let title = snapshot
                            .notes
                            .iter()
                            .find(|n| n.id == plan.id)
                            .map_or("笔记", |n| n.title.as_str());
                        engine.apply(
                            Command::CreateNote {
                                title: format!(
                                    "冲突副本 · {}",
                                    title.chars().take(120).collect::<String>()
                                ),
                                content: local_content.clone(),
                            },
                            now,
                        )?;
                        engine.apply(
                            Command::ReceiveSyncedNote {
                                note_id: plan.id.clone(),
                                expected_hash: plan.expected_local_hash.clone(),
                                content: remote_content.clone(),
                            },
                            now,
                        )?;
                        conflicts += 1;
                        changed += 1;
                    }
                    SyncAction::CopyExternal { content } => {
                        let copy_id = remote_copy_id(&plan.id, content);
                        if let Some(existing) = snapshot
                            .versions
                            .iter()
                            .rev()
                            .find(|v| v.note_id == copy_id)
                        {
                            ensure!(
                                snapshot.notes.iter().any(|n| n.id == copy_id && n.managed)
                                    && snapshot
                                        .versions
                                        .iter()
                                        .any(|v| v.note_id == existing.note_id
                                            && v.hash == studyapp_sync::content_hash(content)),
                                "远端副本已在本地编辑，已保留，请手动检查后再次同步"
                            );
                        } else {
                            engine.apply(
                                Command::ReceiveSyncedCopy {
                                    note_id: copy_id,
                                    title: format!(
                                        "远端副本 · {}",
                                        snapshot
                                            .notes
                                            .iter()
                                            .find(|n| n.id == plan.id)
                                            .map_or("导入笔记", |n| n.title.as_str())
                                            .chars()
                                            .take(120)
                                            .collect::<String>()
                                    ),
                                    content: content.clone(),
                                },
                                now,
                            )?;
                        }
                        changed += 1;
                    }
                }
                Ok(())
            })();
            match result {
                Ok(()) => {
                    self.data.baselines.insert(plan.id, plan.next_baseline);
                }
                Err(error) => errors.push(format!("{}：{}", plan.id, error)),
            }
        }
        if errors.is_empty() {
            self.data.last_success = Some(now);
        }
        if let Err(error) = self.persist() {
            self.data = previous;
            return Err(error);
        }
        self.status.last_success = self.data.last_success;
        self.status.message = format!(
            "同步完成：接收 {changed} 份，保留 {conflicts} 份冲突副本{}",
            if errors.is_empty() {
                String::new()
            } else {
                format!(
                    "；{} 项未完成：{}",
                    errors.len(),
                    errors.into_iter().take(3).collect::<Vec<_>>().join("；")
                )
            }
        );
        Ok(self.status.message.clone())
    }

    pub fn fail(&mut self, message: String) {
        self.status.busy = false;
        self.status.message = message;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use studyapp_sync::content_hash;

    const NOW: i64 = 1_790_000_000;
    fn library(root: &Path, name: &str) -> (Engine, SyncRuntime) {
        let directory = root.join(name);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("study.sqlite3");
        (Engine::open(&path, NOW).unwrap(), SyncRuntime::load(&path))
    }
    fn configure(runtime: &mut SyncRuntime, folder: &Path) {
        std::fs::create_dir_all(folder).unwrap();
        runtime
            .configure(
                SyncSettings {
                    target: SyncTarget::Folder {
                        path: folder.to_string_lossy().into_owned(),
                    },
                    auto_minutes: None,
                },
                None,
            )
            .unwrap();
    }
    fn sync(runtime: &mut SyncRuntime, engine: &mut Engine) -> String {
        let result =
            runtime.task(Arc::new(engine.snapshot()), false).unwrap()().map_err(|e| e.to_string());
        runtime.finish(result, engine, NOW + 60).unwrap()
    }
    fn create(engine: &mut Engine, content: &str) -> String {
        engine
            .apply(
                Command::CreateNote {
                    title: "学习笔记".into(),
                    content: content.into(),
                },
                NOW,
            )
            .unwrap();
        engine.snapshot().notes.last().unwrap().id.clone()
    }
    fn edit(engine: &mut Engine, id: &str, content: &str) {
        let hash = engine
            .snapshot()
            .versions
            .iter()
            .rev()
            .find(|v| v.note_id == id)
            .unwrap()
            .hash
            .clone();
        engine
            .apply(
                Command::SaveNote {
                    note_id: id.into(),
                    expected_hash: hash,
                    content: content.into(),
                },
                NOW + 10,
            )
            .unwrap();
    }

    #[test]
    fn two_libraries_exchange_notes_and_preserve_both_conflicting_edits() {
        let root = tempfile::tempdir().unwrap().keep();
        let folder = root.join("shared");
        let (mut first, mut a) = library(&root, "a");
        let (mut second, mut b) = library(&root, "b");
        configure(&mut a, &folder);
        configure(&mut b, &folder);
        let id = create(&mut first, "original");
        sync(&mut a, &mut first);
        sync(&mut b, &mut second);
        assert_eq!(second.snapshot().notes[0].id, id);
        assert!(second.snapshot().versions[0].offline);
        edit(&mut second, &id, "remote edit");
        sync(&mut b, &mut second);
        edit(&mut first, &id, "local edit");
        assert!(sync(&mut a, &mut first).contains("1 份冲突副本"));
        let snapshot = first.snapshot();
        assert_eq!(snapshot.notes.len(), 2);
        assert_eq!(
            snapshot
                .versions
                .iter()
                .rev()
                .find(|v| v.note_id == id)
                .unwrap()
                .content,
            "remote edit"
        );
        assert!(
            snapshot
                .notes
                .iter()
                .any(|n| n.title.starts_with("冲突副本"))
        );
        assert!(
            snapshot
                .versions
                .iter()
                .any(|v| v.note_id != id && v.content == "local edit")
        );
        sync(&mut a, &mut first);
        sync(&mut b, &mut second);
        assert_eq!(second.snapshot().notes.len(), 2);
        assert_eq!(
            SyncRuntime::load(Path::new(&snapshot.data_path))
                .data
                .baselines
                .len(),
            2
        );
    }

    #[test]
    fn external_remote_changes_create_one_copy_without_touching_original() {
        let root = tempfile::tempdir().unwrap().keep();
        let (mut engine, mut runtime) = library(&root, "local");
        let source = root.join("source.md");
        std::fs::write(&source, "external original").unwrap();
        engine
            .apply(Command::Import(vec![source.clone()]), NOW)
            .unwrap();
        let id = engine.snapshot().notes[0].id.clone();
        let folder = root.join("shared");
        configure(&mut runtime, &folder);
        sync(&mut runtime, &mut engine);
        let provider = LocalFolderProvider::new(&folder).unwrap();
        let previous = provider.read(&id).unwrap().unwrap();
        provider
            .write(&id, "remote changed", previous.revision.as_deref())
            .unwrap();
        sync(&mut runtime, &mut engine);
        assert_eq!(engine.snapshot().notes.len(), 2);
        assert!(
            engine
                .snapshot()
                .versions
                .iter()
                .filter(|v| v.note_id != id)
                .all(|v| v.offline)
        );
        assert_eq!(
            std::fs::read_to_string(source).unwrap(),
            "external original"
        );
        sync(&mut runtime, &mut engine);
        assert_eq!(engine.snapshot().notes.len(), 2);
    }

    #[test]
    fn downloaded_external_copy_is_idempotent_after_config_failure() {
        let root = tempfile::tempdir().unwrap().keep();
        let (mut engine, mut runtime) = library(&root, "local");
        let source = root.join("source.md");
        std::fs::write(&source, "original").unwrap();
        engine.apply(Command::Import(vec![source]), NOW).unwrap();
        let id = engine.snapshot().notes[0].id.clone();
        let folder = root.join("shared");
        configure(&mut runtime, &folder);
        sync(&mut runtime, &mut engine);
        let provider = LocalFolderProvider::new(folder).unwrap();
        let remote = provider.read(&id).unwrap().unwrap();
        provider
            .write(&id, "downloaded", remote.revision.as_deref())
            .unwrap();
        let completion = runtime.task(Arc::new(engine.snapshot()), false).unwrap()().unwrap();
        let path = runtime.path.clone();
        runtime.path = root.clone();
        assert!(runtime.finish(Ok(completion), &mut engine, NOW).is_err());
        assert_eq!(engine.snapshot().notes.len(), 2);
        let copy = engine
            .snapshot()
            .notes
            .iter()
            .find(|n| n.id != id)
            .unwrap()
            .id
            .clone();
        edit(&mut engine, &copy, "locally edited downloaded copy");
        runtime.path = path;
        sync(&mut runtime, &mut engine);
        assert_eq!(engine.snapshot().notes.len(), 2);
        assert_eq!(
            runtime.data.baselines[&id].remote_hash,
            studyapp_sync::content_hash("downloaded")
        );
        assert!(
            engine
                .snapshot()
                .versions
                .iter()
                .any(|v| v.note_id == copy && v.content == "locally edited downloaded copy")
        );
    }

    #[test]
    fn maximum_baseline_count_survives_restart_and_duplicate_start_stays_busy() {
        let root = tempfile::tempdir().unwrap().keep();
        let (engine, mut runtime) = library(&root, "local");
        configure(&mut runtime, &root.join("shared"));
        for i in 0..studyapp_sync::MAX_ENTRIES {
            runtime.data.baselines.insert(
                remote_copy_id(&i.to_string(), "note"),
                Baseline {
                    local_hash: "a".repeat(64),
                    remote_hash: "b".repeat(64),
                },
            );
        }
        runtime.persist().unwrap();
        assert_eq!(
            SyncRuntime::load(Path::new(&engine.snapshot().data_path))
                .data
                .baselines
                .len(),
            studyapp_sync::MAX_ENTRIES
        );
        let _job = runtime.task(Arc::new(engine.snapshot()), true).unwrap();
        assert!(runtime.task(Arc::new(engine.snapshot()), true).is_err());
        assert!(runtime.status.busy);
    }

    #[test]
    fn an_edit_while_network_work_runs_prevents_pull_and_baseline_advancement() {
        let root = tempfile::tempdir().unwrap().keep();
        let (mut engine, mut runtime) = library(&root, "local");
        let id = create(&mut engine, "original");
        let folder = root.join("shared");
        configure(&mut runtime, &folder);
        sync(&mut runtime, &mut engine);
        let original_baseline = runtime.data.baselines.clone();
        let provider = LocalFolderProvider::new(folder).unwrap();
        let previous = provider.read(&id).unwrap().unwrap();
        provider
            .write(&id, "remote edit", previous.revision.as_deref())
            .unwrap();
        let completion = runtime.task(Arc::new(engine.snapshot()), false).unwrap()().unwrap();
        edit(&mut engine, &id, "during sync");
        let message = runtime
            .finish(Ok(completion), &mut engine, NOW + 90)
            .unwrap();
        assert!(message.contains("未完成"));
        assert_eq!(runtime.data.baselines, original_baseline);
        assert_eq!(
            engine.snapshot().versions.last().unwrap().content,
            "during sync"
        );
    }

    #[test]
    fn config_write_failure_keeps_baselines_retryable() {
        let root = tempfile::tempdir().unwrap().keep();
        let (mut engine, mut runtime) = library(&root, "local");
        let id = create(&mut engine, "original");
        configure(&mut runtime, &root.join("shared"));
        let completion = runtime.task(Arc::new(engine.snapshot()), false).unwrap()().unwrap();
        let config = runtime.path.clone();
        // A directory at the publication path forces rename failure without deleting anything.
        let blocked = root.join("blocked-config");
        std::fs::create_dir(&blocked).unwrap();
        runtime.path = blocked;
        assert!(runtime.finish(Ok(completion), &mut engine, NOW).is_err());
        assert!(runtime.data.baselines.is_empty());
        assert!(runtime.data.last_success.is_none());
        runtime.path = config;
        sync(&mut runtime, &mut engine);
        assert_eq!(
            runtime.data.baselines[&id].local_hash,
            content_hash("original")
        );
    }

    #[test]
    fn test_connection_does_not_upload_notes_and_settings_are_library_scoped() {
        let root = tempfile::tempdir().unwrap().keep();
        let (mut engine, mut runtime) = library(&root, "local");
        create(&mut engine, "private note");
        let folder = root.join("shared");
        configure(&mut runtime, &folder);
        let result = runtime.task(Arc::new(engine.snapshot()), true).unwrap()().unwrap();
        runtime.finish(Ok(result), &mut engine, NOW).unwrap();
        assert!(
            LocalFolderProvider::new(folder)
                .unwrap()
                .list()
                .unwrap()
                .is_empty()
        );
        assert!(runtime.data.baselines.is_empty());
        assert!(matches!(
            library(&root, "other").1.status.settings.target,
            SyncTarget::Disabled
        ));
    }

    #[test]
    fn webdav_configuration_seals_credentials_and_does_not_accept_http() {
        let root = tempfile::tempdir().unwrap().keep();
        let (_, mut runtime) = library(&root, "local");
        let settings = SyncSettings {
            target: SyncTarget::WebDav {
                url: "https://example.com/dav/".into(),
                username: "learner".into(),
            },
            auto_minutes: Some(15),
        };
        runtime
            .configure(
                settings.clone(),
                Some(Zeroizing::new("test-app-password".into())),
            )
            .unwrap();
        assert!(
            !std::fs::read_to_string(&runtime.path)
                .unwrap()
                .contains("test-app-password")
        );
        assert_eq!(
            crate::credentials::unprotect(&runtime.data.protected_password)
                .unwrap()
                .as_str(),
            "test-app-password"
        );
        runtime.configure(settings.clone(), None).unwrap();
        assert!(
            runtime
                .configure(
                    SyncSettings {
                        target: SyncTarget::WebDav {
                            url: "http://example.com/".into(),
                            username: "learner".into()
                        },
                        auto_minutes: None
                    },
                    Some(Zeroizing::new("secret".into()))
                )
                .is_err()
        );
        assert_eq!(runtime.status.settings, settings);
    }
}
