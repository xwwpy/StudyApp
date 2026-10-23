//! Library storage inspection and explicit, single-file maintenance.
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{Read, Seek, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use studyapp_core::{Command, Engine, Note, Snapshot};

#[derive(Clone, Debug, Default)]
pub struct StorageReport {
    pub data_path: String,
    pub database_bytes: u64,
    pub managed_bytes: u64,
    pub retained_bytes: u64,
    pub auxiliary_bytes: u64,
    pub backup_bytes: u64,
    pub copies: Vec<ManagedCopy>,
}

#[derive(Clone, Debug)]
pub struct ManagedCopy {
    pub note_id: String,
    pub title: String,
    pub path: String,
    pub bytes: u64,
    pub detached: bool,
    pub deletable: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug)]
pub enum StorageEvent {
    Finished(std::result::Result<StorageReport, String>),
}

fn reparse(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Windows handles deny directory renames while traversing, and explicitly open
/// the reparse point itself. No inspected/copied file is opened through a link.
pub(crate) fn plain_handle(path: &Path, directory: bool, exclusive_read: bool) -> Result<File> {
    let before = std::fs::symlink_metadata(path)?;
    ensure!(
        !reparse(&before),
        "拒绝访问链接或重解析点：{}",
        path.display()
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000 | 0x0200_0000);
        options.share_mode(if directory {
            3
        } else if exclusive_read {
            1
        } else {
            7
        });
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x20000); // O_NOFOLLOW
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100); // O_NOFOLLOW
    }
    #[cfg(not(windows))]
    let _ = exclusive_read;
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        !reparse(&metadata),
        "拒绝访问链接或重解析点：{}",
        path.display()
    );
    ensure!(
        if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        },
        "路径类型不正确：{}",
        path.display()
    );
    Ok(file)
}

pub(crate) fn guard_directories(path: &Path, create: bool) -> Result<Vec<File>> {
    ensure!(path.is_absolute(), "需要绝对目录路径");
    ensure!(
        !path.components().any(|c| matches!(c, Component::ParentDir)),
        "目录不能包含上级路径"
    );
    let mut guards = Vec::new();
    let mut ancestors: Vec<_> = path.ancestors().collect();
    ancestors.reverse();
    for directory in ancestors {
        if create && !directory.try_exists()? {
            std::fs::create_dir(directory)?;
        }
        guards.push(plain_handle(directory, true, false)?);
    }
    Ok(guards)
}

fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

fn root(snapshot: &Snapshot) -> Result<&Path> {
    Path::new(&snapshot.data_path)
        .parent()
        .context("资料库目录不存在")
}

fn owned_path(snapshot: &Snapshot, note: &Note) -> Result<PathBuf> {
    ensure!(note.managed, "外部源文件不能删除");
    let id = note.id.as_bytes();
    ensure!(
        id.len() == 36
            && id.iter().enumerate().all(|(i, byte)| {
                if [8, 13, 18, 23].contains(&i) {
                    *byte == b'-'
                } else {
                    byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)
                }
            }),
        "笔记身份不是规范 UUID"
    );
    let expected = root(snapshot)?
        .join("notes")
        .join(format!("{}.md", note.id));
    ensure!(
        same_path(Path::new(&note.path), &expected),
        "副本不属于当前资料库，原位置已保留"
    );
    Ok(expected)
}

fn visit_tree(
    root: &Path,
    relative: &Path,
    reject_links: bool,
    skip: &impl Fn(&Path) -> bool,
    count: &mut usize,
    visitor: &mut impl FnMut(&Path, &Path, File) -> Result<()>,
) -> Result<()> {
    ensure!(relative.components().count() <= 64, "资料库目录层数过深");
    let directory = root.join(relative);
    let _guard = plain_handle(&directory, true, false)?;
    for entry in std::fs::read_dir(&directory)? {
        let entry = entry?;
        *count += 1;
        ensure!(*count <= 100_000, "资料库文件数量超过统计限制");
        let path = entry.path();
        let relative = relative.join(entry.file_name());
        if skip(&relative) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path)?;
        if reparse(&metadata) {
            ensure!(
                !reject_links,
                "资料库含有链接或重解析点，未迁移：{}",
                path.display()
            );
            continue;
        }
        if metadata.is_dir() {
            visit_tree(root, &relative, reject_links, skip, count, visitor)?;
        } else if metadata.is_file() {
            let file = plain_handle(&path, false, reject_links)?;
            visitor(&path, &relative, file)?;
        } else {
            ensure!(!reject_links, "资料库含有不支持的特殊文件");
        }
    }
    Ok(())
}

fn live_database_file(relative: &Path, database: &Path) -> bool {
    let Some(name) = database.file_name() else {
        return false;
    };
    if relative.components().count() != 1 {
        return false;
    }
    let name = name.to_string_lossy();
    let relative = relative.to_string_lossy();
    relative == name
        || ["-wal", "-shm", "-journal"]
            .iter()
            .any(|suffix| relative == format!("{name}{suffix}"))
}

fn runtime_lock_file(relative: &Path, database: &Path) -> bool {
    relative.components().count() == 1
        && relative.file_name() == database.with_extension("lock").file_name()
}

pub fn inspect(snapshot: &Snapshot) -> Result<StorageReport> {
    let directory = root(snapshot)?;
    let _guards = guard_directories(directory, false)?;
    let mut report = StorageReport {
        data_path: snapshot.data_path.clone(),
        ..Default::default()
    };
    let mut owned = BTreeSet::new();
    for note in snapshot.notes.iter().filter(|note| note.managed) {
        let inspected = (|| -> Result<(PathBuf, u64)> {
            let path = owned_path(snapshot, note)?;
            let _guard = plain_handle(&directory.join("notes"), true, false)?;
            let file = plain_handle(&path, false, false)?;
            Ok((path, file.metadata()?.len()))
        })();
        let (bytes, reason) = match inspected {
            Ok((path, bytes)) => {
                owned.insert(path);
                (bytes, None)
            }
            Err(error) => (0, Some(format!("{error:#}"))),
        };
        report.copies.push(ManagedCopy {
            note_id: note.id.clone(),
            title: note.title.clone(),
            path: note.path.clone(),
            bytes,
            detached: note.detached,
            deletable: reason.is_none(),
            reason,
        });
    }
    visit_tree(
        directory,
        Path::new(""),
        false,
        &|_| false,
        &mut 0,
        &mut |path, relative, file| {
            let bytes = file.metadata()?.len();
            let category = if live_database_file(relative, Path::new(&snapshot.data_path)) {
                &mut report.database_bytes
            } else if owned.contains(path) {
                &mut report.managed_bytes
            } else if relative.starts_with("notes")
                || relative
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(".studyapp-"))
            {
                &mut report.retained_bytes
            } else if relative
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| {
                    ["sqlite", "sqlite3", "db"]
                        .iter()
                        .any(|expected| e.eq_ignore_ascii_case(expected))
                })
            {
                &mut report.backup_bytes
            } else if runtime_lock_file(relative, Path::new(&snapshot.data_path)) {
                return Ok(());
            } else {
                &mut report.auxiliary_bytes
            };
            *category = category.checked_add(bytes).context("存储统计超出范围")?;
            Ok(())
        },
    )?;
    Ok(report)
}

pub fn delete_managed_copy(engine: &mut Engine, note_id: &str, now: i64) -> Result<StorageReport> {
    let snapshot = engine.snapshot();
    let note = snapshot
        .notes
        .iter()
        .find(|note| note.id == note_id)
        .context("找不到笔记")?;
    let path = owned_path(&snapshot, note)?;
    let _guards = guard_directories(path.parent().context("副本目录不存在")?, false)?;
    let file = plain_handle(&path, false, false).context("副本不可删除，原文件已保留")?;
    engine.apply(
        Command::UnmanageNote {
            note_id: note_id.into(),
        },
        now,
    )?;
    drop(file);
    // Exactly one validated owned file. Its parent handles remain pinned on Windows.
    std::fs::remove_file(&path).context("副本删除失败；已停止管理，原文件仍保留，可重试")?;
    inspect(&engine.snapshot())
}

pub fn prepare_migration(engine: &mut Engine, target: &Path, now: i64) -> Result<PathBuf> {
    let snapshot = engine.snapshot();
    let source = root(&snapshot)?;
    let _source_guards = guard_directories(source, false)?;
    ensure!(target.is_absolute(), "迁移目标需要绝对路径");
    ensure!(
        !target
            .components()
            .any(|c| matches!(c, Component::ParentDir)),
        "迁移路径不能包含上级路径"
    );
    let mut existing = target;
    let mut tail = Vec::new();
    while !existing.try_exists()? {
        tail.push(existing.file_name().context("迁移目录无效")?.to_owned());
        existing = existing.parent().context("迁移目录无效")?;
    }
    let _existing_guards = guard_directories(existing, false)?;
    let mut resolved = std::fs::canonicalize(existing)?;
    for component in tail.iter().rev() {
        resolved.push(component);
    }
    ensure!(
        !resolved.starts_with(source),
        "迁移目标不能位于当前资料库内"
    );
    let _target_guards = guard_directories(&resolved, true)?;
    ensure!(
        std::fs::read_dir(&resolved)?.next().is_none(),
        "迁移目标必须是空目录"
    );
    let database = resolved.join("studyapp.sqlite3");
    engine.checkpoint()?;
    engine.apply(Command::Backup(database.clone()), now)?;
    let skip = |relative: &Path| {
        live_database_file(relative, Path::new(&snapshot.data_path))
            || runtime_lock_file(relative, Path::new(&snapshot.data_path))
    };
    visit_tree(
        source,
        Path::new(""),
        true,
        &skip,
        &mut 0,
        &mut |_, relative, mut file| {
            let destination = resolved.join(relative);
            let _guards = guard_directories(destination.parent().context("迁移目录无效")?, true)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)?;
            file.rewind()?;
            std::io::copy(&mut file, &mut output)?;
            output.sync_all()?;
            drop(output);
            let mut copied = plain_handle(&destination, false, true)?;
            file.rewind()?;
            let mut source_bytes = [0; 64 * 1024];
            let mut copied_bytes = [0; 64 * 1024];
            loop {
                let count = file.read(&mut source_bytes)?;
                if count == 0 {
                    ensure!(
                        copied.read(&mut copied_bytes[..1])? == 0,
                        "迁移文件校验失败"
                    );
                    break;
                }
                copied.read_exact(&mut copied_bytes[..count])?;
                ensure!(
                    source_bytes[..count] == copied_bytes[..count],
                    "迁移文件校验失败"
                );
            }
            Ok(())
        },
    )?;
    Ok(database)
}

pub fn persist_location(config: &Path, directory: &Path) -> Result<()> {
    let parent = config.parent().context("启动配置目录不存在")?;
    let _guards = guard_directories(parent, true)?;
    if let Ok(metadata) = std::fs::symlink_metadata(config) {
        ensure!(
            !reparse(&metadata) && metadata.is_file(),
            "启动配置必须是普通文件"
        );
    }
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let temporary = parent.join(format!(
        ".location-{}-{}-{}.next",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    output.write_all(&serde_json::to_vec(directory)?)?;
    output.sync_all()?;
    drop(output);
    std::fs::rename(&temporary, config).context("无法更新启动位置；当前资料库未切换")?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use studyapp_core::Command;

    fn library() -> (PathBuf, Engine) {
        let root = tempfile::tempdir().unwrap().keep();
        let engine = Engine::open(&root.join("studyapp.sqlite3"), 1000).unwrap();
        (root, engine)
    }

    fn create(engine: &mut Engine) -> studyapp_core::Note {
        engine
            .apply(
                Command::CreateNote {
                    title: "owned copy".into(),
                    content: "managed content".into(),
                },
                1001,
            )
            .unwrap();
        engine.snapshot().notes.last().unwrap().clone()
    }

    #[test]
    fn migration_copies_categorized_preferences_and_counts_them_as_auxiliary_data() {
        let (root, mut engine) = library();
        let database = root.join("studyapp.sqlite3");
        let (mut store, mut runtime) =
            crate::preferences::PreferencesStore::open(&database, &mut engine).unwrap();
        let before = store.snapshot();
        let mut settings = before.settings.clone();
        settings.show_file_paths = true;
        store
            .save(
                crate::preferences::SavePreferences {
                    expected_revision: before.revision,
                    settings,
                    shortcuts: before.shortcuts,
                    sync: before.sync,
                    password: None,
                },
                &mut engine,
                &mut runtime,
            )
            .unwrap();
        let manifest = std::fs::read(root.join("settings/manifest.json")).unwrap();
        let report = inspect(&engine.snapshot()).unwrap();
        assert!(report.auxiliary_bytes > manifest.len() as u64);
        let destination = tempfile::tempdir().unwrap().keep().join("migrated");
        let migrated_database = prepare_migration(&mut engine, &destination, 1001).unwrap();
        let mut migrated = Engine::open(&migrated_database, 1002).unwrap();
        let (copied, _) =
            crate::preferences::PreferencesStore::open(&migrated_database, &mut migrated).unwrap();
        assert_eq!(copied.snapshot().revision, store.snapshot().revision);
        assert!(migrated.snapshot().settings.show_file_paths);
        assert_eq!(
            std::fs::read(destination.join("settings/manifest.json")).unwrap(),
            manifest
        );
        assert_eq!(
            std::fs::read(root.join("settings/manifest.json")).unwrap(),
            manifest
        );
    }

    #[test]
    fn report_counts_owned_copies_backups_and_retained_data_without_external_sources() {
        let (root, mut engine) = library();
        let note = create(&mut engine);
        let external_root = tempfile::tempdir().unwrap().keep();
        let external = external_root.join("external.md");
        std::fs::write(&external, "never count or remove external source").unwrap();
        engine.apply(Command::Import(vec![external]), 1002).unwrap();
        std::fs::write(root.join("note-draft.json"), "null").unwrap();
        std::fs::write(root.join("notes/unknown-orphan.md"), "retained").unwrap();
        let backup = root.join("backup.sqlite3");
        engine.apply(Command::Backup(backup.clone()), 1003).unwrap();
        let report = inspect(&engine.snapshot()).unwrap();
        assert_eq!(report.data_path, engine.snapshot().data_path);
        assert!(report.database_bytes > 0);
        assert_eq!(report.managed_bytes, "managed content".len() as u64);
        assert!(report.retained_bytes >= "retained".len() as u64);
        assert_eq!(
            report.backup_bytes,
            std::fs::metadata(backup).unwrap().len()
        );
        assert_eq!(report.auxiliary_bytes, 4);
        assert_eq!(report.copies.len(), 1);
        assert_eq!(report.copies[0].note_id, note.id);
        assert!(report.copies[0].deletable);
    }

    #[test]
    fn delete_owned_copy_detaches_and_preserves_history_and_external_files() {
        let (root, mut engine) = library();
        let note = create(&mut engine);
        let external = root.join("external.md");
        std::fs::write(&external, "external").unwrap();
        engine
            .apply(Command::Import(vec![external.clone()]), 1002)
            .unwrap();
        let external_id = engine.snapshot().notes[1].id.clone();
        assert!(delete_managed_copy(&mut engine, &external_id, 1003).is_err());
        assert_eq!(std::fs::read_to_string(&external).unwrap(), "external");
        assert!(!engine.snapshot().notes[1].detached);
        let report = delete_managed_copy(&mut engine, &note.id, 1004).unwrap();
        assert!(!Path::new(&note.path).exists());
        assert!(engine.snapshot().notes[0].detached);
        assert_eq!(engine.snapshot().versions.len(), 2);
        assert!(!report.copies[0].deletable);
        assert_eq!(std::fs::read_to_string(external).unwrap(), "external");
    }

    #[cfg(windows)]
    #[test]
    fn failed_single_copy_deletion_keeps_file_detached_and_can_retry() {
        use std::os::windows::fs::OpenOptionsExt;
        let (_, mut engine) = library();
        let note = create(&mut engine);
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&note.path)
            .unwrap();
        assert!(delete_managed_copy(&mut engine, &note.id, 1002).is_err());
        assert!(engine.snapshot().notes[0].detached);
        assert_eq!(
            std::fs::read_to_string(&note.path).unwrap(),
            "managed content"
        );
        drop(held);
        delete_managed_copy(&mut engine, &note.id, 1003).unwrap();
        assert!(!Path::new(&note.path).exists());
    }

    #[test]
    fn migration_copies_consistent_library_and_auxiliary_files_without_changing_source() {
        let (root, mut engine) = library();
        let note = create(&mut engine);
        let draft = r#"{"note_id":null,"expected_hash":null,"title":"draft","content":"latest"}"#;
        std::fs::write(root.join("note-draft.json"), draft).unwrap();
        std::fs::write(root.join("sync-config.json"), "{}").unwrap();
        std::fs::create_dir(root.join("retained")).unwrap();
        std::fs::write(root.join("retained/user.lock"), "user content").unwrap();
        let target_root = tempfile::tempdir().unwrap().keep();
        let target = target_root.join("migrated");
        let database = prepare_migration(&mut engine, &target, 1002).unwrap();
        let migrated = Engine::open(&database, 1003).unwrap();
        let snapshot = migrated.snapshot();
        assert_eq!(snapshot.notes[0].id, note.id);
        assert_eq!(snapshot.versions[0].content, "managed content");
        assert!(
            Path::new(&snapshot.notes[0].path).starts_with(std::fs::canonicalize(&target).unwrap())
        );
        assert_eq!(
            std::fs::read_to_string(target.join("note-draft.json")).unwrap(),
            draft
        );
        assert_eq!(
            std::fs::read_to_string(target.join("sync-config.json")).unwrap(),
            "{}"
        );
        assert_eq!(
            std::fs::read_to_string(target.join("retained/user.lock")).unwrap(),
            "user content"
        );
        assert_eq!(
            std::fs::read_to_string(&note.path).unwrap(),
            "managed content"
        );
        assert!(root.join("studyapp.sqlite3").is_file());
        let config = target_root.join("config/location.json");
        persist_location(&config, &target).unwrap();
        let remembered: PathBuf = serde_json::from_slice(&std::fs::read(config).unwrap()).unwrap();
        assert_eq!(remembered, target);
    }

    #[test]
    fn migration_refuses_nonempty_or_nested_destination_and_preserves_source() {
        let (root, mut engine) = library();
        let note = create(&mut engine);
        let target = tempfile::tempdir().unwrap().keep();
        std::fs::write(target.join("user.txt"), "keep").unwrap();
        assert!(prepare_migration(&mut engine, &target, 1002).is_err());
        assert_eq!(
            std::fs::read_to_string(target.join("user.txt")).unwrap(),
            "keep"
        );
        assert!(prepare_migration(&mut engine, &root.join("nested"), 1003).is_err());
        assert_eq!(
            std::fs::read_to_string(&note.path).unwrap(),
            "managed content"
        );
        assert!(!engine.snapshot().notes[0].detached);
    }

    #[test]
    fn detached_copy_from_another_library_cannot_be_deleted() {
        let (source, mut original) = library();
        let note = create(&mut original);
        original
            .apply(
                Command::UnmanageNote {
                    note_id: note.id.clone(),
                },
                1002,
            )
            .unwrap();
        let backup = source.join("detached.sqlite3");
        original
            .apply(Command::Backup(backup.clone()), 1003)
            .unwrap();
        let (_, mut restored) = library();
        restored.apply(Command::Restore(backup), 1004).unwrap();
        assert!(delete_managed_copy(&mut restored, &note.id, 1005).is_err());
        assert_eq!(
            std::fs::read_to_string(&note.path).unwrap(),
            "managed content"
        );
        let report = inspect(&restored.snapshot()).unwrap();
        assert!(!report.copies[0].deletable);
        assert!(
            report.copies[0]
                .reason
                .as_ref()
                .unwrap()
                .contains("不属于当前资料库")
        );
    }

    #[cfg(windows)]
    #[test]
    fn junctions_are_not_counted_copied_or_used_to_delete_outside_files() {
        let (root, mut engine) = library();
        let note = create(&mut engine);
        let external = tempfile::tempdir().unwrap().keep();
        let victim = external.join(format!("{}.md", note.id));
        std::fs::write(&victim, "external victim").unwrap();
        let original_notes = root.join("notes");
        let retained_notes = root.join("retained-notes");
        assert!(original_notes.starts_with(&root) && retained_notes.starts_with(&root));
        std::fs::rename(&original_notes, &retained_notes).unwrap();
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&original_notes)
            .arg(&external)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = inspect(&engine.snapshot()).unwrap();
        assert_eq!(report.managed_bytes, 0);
        assert!(!report.copies[0].deletable);
        assert!(delete_managed_copy(&mut engine, &note.id, 1002).is_err());
        assert!(!engine.snapshot().notes[0].detached);
        let target_root = tempfile::tempdir().unwrap().keep();
        let error = prepare_migration(&mut engine, &target_root.join("target"), 1003).unwrap_err();
        assert!(error.to_string().contains("重解析点"), "{error:#}");
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "external victim");
        assert!(!target_root.join("target/notes").exists());
    }

    #[cfg(windows)]
    #[test]
    fn location_publish_failure_preserves_previous_location_bytes() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = tempfile::tempdir().unwrap().keep();
        let config = root.join("location.json");
        let previous = serde_json::to_vec(&root.join("original")).unwrap();
        std::fs::write(&config, &previous).unwrap();
        let held = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&config)
            .unwrap();
        assert!(persist_location(&config, &root.join("new")).is_err());
        assert_eq!(std::fs::read(&config).unwrap(), previous);
        drop(held);
        persist_location(&config, &root.join("new")).unwrap();
        let current: PathBuf = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
        assert_eq!(current, root.join("new"));
    }
}
