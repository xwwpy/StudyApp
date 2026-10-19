use crate::{storage::State, *};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use similar::{ChangeTag, TextDiff};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};
const MAX_BYTES: u64 = 10 * 1024 * 1024;
fn read(path: &Path) -> Result<String> {
    ensure!(
        path.extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown")),
        "仅支持 .md 或 .markdown 文件"
    );
    let file = std::fs::File::open(path).with_context(|| format!("无法读取 {}", path.display()))?;
    ensure!(file.metadata()?.len() <= MAX_BYTES, "笔记超过 10 MiB 限制");
    let mut bytes = vec![];
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_BYTES, "笔记超过 10 MiB 限制");
    let text = String::from_utf8(bytes).context("笔记不是有效的 UTF-8 文本")?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).into())
}
fn hash(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}
fn title(content: &str, path: &Path) -> String {
    content
        .lines()
        .find_map(|line| {
            line.strip_prefix("# ")
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("未命名笔记")
        })
        .chars()
        .take(200)
        .collect()
}
fn append_version(
    state: &mut State,
    index: usize,
    content: String,
    now: i64,
    offline: bool,
    baseline: bool,
) {
    let note = &state.notes[index];
    let previous = state
        .versions
        .iter()
        .rev()
        .find(|v| v.note_id == note.id)
        .map(|v| v.content.as_str())
        .unwrap_or("");
    let (diff, summary) = if baseline {
        (String::new(), String::new())
    } else {
        let delta = TextDiff::from_lines(previous, &content);
        let diff = delta
            .unified_diff()
            .context_radius(3)
            .header("上一版本", "当前版本")
            .to_string();
        let mut pieces: Vec<String> = vec![];
        let mut heading = String::new();
        for change in delta.iter_all_changes() {
            let line = change.value().trim();
            if line.starts_with('#') && change.tag() != ChangeTag::Delete {
                heading = line.into();
            }
            if change.tag() == ChangeTag::Insert && !line.is_empty() {
                if !heading.is_empty() && !pieces.contains(&heading) {
                    pieces.push(heading.clone());
                }
                if pieces.len() < 16 {
                    pieces.push(line.chars().take(160).collect());
                }
            }
        }
        if pieces.is_empty() && previous != content {
            pieces.push("删除或调整了笔记内容".into());
        }
        (diff, pieces.join("\n"))
    };
    let version_id = uuid::Uuid::new_v4().to_string();
    let version = NoteVersion {
        id: version_id.clone(),
        note_id: note.id.clone(),
        hash: hash(&content),
        content,
        captured_at: now,
        offline,
        summary,
        diff,
    };
    state.notes[index].title = title(&version.content, Path::new(&state.notes[index].path));
    state.notes[index].updated_at = now;
    state.notes[index].revision += 1;
    Arc::make_mut(&mut state.versions).push(version);
    if !baseline
        && !offline
        && !state.timer.paused
        && !state.unattributed_notes.contains(&state.notes[index].id)
    {
        state.active_versions.push(version_id);
    }
}
pub(crate) fn import(state: &mut State, paths: Vec<PathBuf>, now: i64) -> String {
    let mut success = 0;
    let mut duplicate = 0;
    let mut failures = vec![];
    for path in paths {
        let result = (|| -> Result<()> {
            let canonical = std::fs::canonicalize(&path)
                .with_context(|| format!("找不到 {}", path.display()))?;
            let key = canonical.to_string_lossy().into_owned();
            if state.notes.iter().any(|n| same_path(&n.path, &key)) {
                duplicate += 1;
                return Ok(());
            }
            let content = read(&canonical)?;
            state.notes.push(Note {
                id: uuid::Uuid::new_v4().to_string(),
                path: key,
                title: title(&content, &canonical),
                missing: false,
                error: None,
                created_at: now,
                updated_at: now,
                revision: 0,
            });
            append_version(state, state.notes.len() - 1, content, now, false, true);
            success += 1;
            Ok(())
        })();
        if let Err(e) = result {
            failures.push(format!("{}：{e:#}", path.display()));
        }
    }
    format!(
        "导入成功 {success} 份，已存在 {duplicate} 份，失败 {} 份{}",
        failures.len(),
        if failures.is_empty() {
            String::new()
        } else {
            format!("\n{}", failures.join("\n"))
        }
    )
}
fn same_path(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}
pub(crate) fn relink(state: &mut State, note_id: &str, path: PathBuf, now: i64) -> Result<()> {
    let index = state
        .notes
        .iter()
        .position(|n| n.id == note_id)
        .context("找不到笔记")?;
    let path = std::fs::canonicalize(path)?;
    let key = path.to_string_lossy().into_owned();
    ensure!(
        !state
            .notes
            .iter()
            .any(|n| n.id != note_id && same_path(&n.path, &key)),
        "该路径已属于另一份笔记"
    );
    let content = read(&path)?;
    state.notes[index].path = key;
    state.notes[index].missing = false;
    state.notes[index].error = None;
    state.notes[index].updated_at = now;
    state.notes[index].revision += 1;
    state.candidates.remove(note_id);
    state.unattributed_notes.remove(note_id);
    if state
        .versions
        .iter()
        .rev()
        .find(|v| v.note_id == note_id)
        .is_none_or(|v| v.hash != hash(&content))
    {
        append_version(state, index, content, now, true, false);
    }
    Ok(())
}
pub(crate) fn scan(state: &mut State, offline: bool, now: i64) -> String {
    let mut captured = 0;
    let mut errors = 0;
    for index in 0..state.notes.len() {
        let path = PathBuf::from(&state.notes[index].path);
        let note_id = state.notes[index].id.clone();
        match read(&path) {
            Ok(content) => {
                if state.notes[index].missing || state.notes[index].error.is_some() {
                    state.notes[index].updated_at = now;
                    state.notes[index].revision += 1;
                }
                state.notes[index].missing = false;
                state.notes[index].error = None;
                let digest = hash(&content);
                if state
                    .versions
                    .iter()
                    .rev()
                    .find(|v| v.note_id == note_id)
                    .is_some_and(|v| v.hash == digest)
                {
                    state.candidates.remove(&note_id);
                    state.unattributed_notes.remove(&note_id);
                    continue;
                }
                if offline
                    || state.unattributed_notes.contains(&note_id)
                    || state
                        .candidates
                        .get(&note_id)
                        .is_some_and(|(candidate, first)| {
                            *candidate == digest && now.saturating_sub(*first) >= 3
                        })
                {
                    append_version(state, index, content, now, offline, false);
                    state.candidates.remove(&note_id);
                    state.unattributed_notes.remove(&note_id);
                    captured += 1;
                } else if state
                    .candidates
                    .get(&note_id)
                    .is_none_or(|(candidate, _)| *candidate != digest)
                {
                    state.candidates.insert(note_id, (digest, now));
                }
            }
            Err(error) => {
                let missing = !path.exists();
                let error = format!("{error:#}");
                if state.notes[index].missing != missing
                    || state.notes[index].error.as_ref() != Some(&error)
                {
                    state.notes[index].updated_at = now;
                    state.notes[index].revision += 1;
                }
                state.notes[index].missing = missing;
                state.notes[index].error = Some(error);
                state.candidates.remove(&note_id);
                errors += 1;
            }
        }
    }
    format!("扫描完成，新增 {captured} 个版本，读取失败 {errors} 份")
}
