use crate::{
    storage::{self, State, Undo},
    *,
};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub struct Engine {
    conn: Connection,
    path: PathBuf,
    state: State,
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn add_days(now: i64, days: u32) -> Result<i64> {
    now.checked_add(i64::from(days) * 86400)
        .context("时间超出范围")
}
impl Engine {
    pub fn open(database: &Path, now: i64) -> Result<Self> {
        if let Some(parent) = database.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut conn = Connection::open(database).context("无法打开学习资料库")?;
        storage::initialize(&mut conn)?;
        storage::configure(&conn)?;
        let path = std::fs::canonicalize(database)?;
        let mut state = storage::load(&conn)?.unwrap_or_else(|| State::new(now));
        state.last_tick = state.last_tick.max(now);
        state.expire(now);
        let mut engine = Self { conn, path, state };
        engine.apply(Command::Scan { offline: true }, now)?;
        Ok(engine)
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state
            .snapshot(self.path.to_string_lossy().into_owned())
    }
    pub fn apply(&mut self, command: Command, now: i64) -> Result<String> {
        if let Command::Backup(path) = &command {
            self.backup(path)?;
            return Ok(format!("备份已保存：{}", path.display()));
        }
        if let Command::Restore(path) = &command {
            return self.restore(path, now);
        }
        let mut next = self.state.clone();
        next.expire(now);
        let message = match command {
            Command::SaveSettings(settings) => {
                settings.validate()?;
                if next.timer.active_seconds == 0 {
                    next.timer.target_seconds = u64::from(settings.interval_minutes) * 60;
                }
                next.settings = settings;
                "设置已保存".into()
            }
            Command::SetPaused(paused) => {
                if next.timer.paused && !paused {
                    // Reconcile every source before admitting new activity. Candidates may
                    // have been observed while paused, or not observed at all.
                    next.unattributed_notes
                        .extend(next.notes.iter().map(|n| n.id.clone()));
                    crate::notes::scan(&mut next, false, now);
                }
                next.timer.paused = paused;
                next.last_tick = next.last_tick.max(now);
                if paused {
                    next.candidates.clear();
                }
                if paused {
                    "计时已暂停"
                } else {
                    "计时已继续"
                }
                .into()
            }
            Command::Import(paths) => crate::notes::import(&mut next, paths, now),
            Command::Relink { note_id, path } => {
                crate::notes::relink(&mut next, &note_id, path, now)?;
                "笔记已重新关联".into()
            }
            Command::Scan { offline } => crate::notes::scan(&mut next, offline, now),
            Command::Submit {
                session_id,
                summary,
                note_ids,
                version_ids,
            } => {
                Self::notes_exist(&next, &note_ids)?;
                Self::summary_valid(&summary)?;
                let note_ids = unique(note_ids);
                let index = next
                    .sessions
                    .iter()
                    .position(|s| s.id == session_id)
                    .context("找不到学习记录")?;
                let session = &next.sessions[index];
                ensure!(
                    session.status == SessionStatus::Pending && now < session.deadline,
                    "该记录已处理或已超过回答期限，请使用更正功能"
                );
                let version_ids = Self::reconcile_versions(
                    &next,
                    &note_ids,
                    &session.version_ids,
                    version_ids,
                    session.ended_at,
                )?;
                let session = &mut next.sessions[index];
                session.status = SessionStatus::Learned;
                session.summary = summary.trim().into();
                session.note_ids = note_ids;
                session.version_ids = version_ids;
                session.updated_at = now;
                session.revision += 1;
                Self::ensure_review(&mut next, &session_id)?;
                "学习记录已确认".into()
            }
            Command::Skip { session_id } => {
                let session = next
                    .sessions
                    .iter_mut()
                    .find(|s| s.id == session_id)
                    .context("找不到学习记录")?;
                ensure!(session.status == SessionStatus::Pending, "该记录已处理");
                session.status = SessionStatus::Missed;
                session.updated_at = now;
                session.revision += 1;
                "已标记为未学习".into()
            }
            Command::Manual {
                started_at,
                minutes,
                summary,
                note_ids,
                version_ids,
            } => {
                Self::summary_valid(&summary)?;
                Self::notes_exist(&next, &note_ids)?;
                ensure!((1..=1440).contains(&minutes), "补记时长应为 1–1440 分钟");
                let seconds = u64::from(minutes) * 60;
                let ended_at = started_at
                    .checked_add(seconds as i64)
                    .context("时间超出范围")?;
                ensure!(ended_at <= now, "不能补记尚未结束的学习时段");
                let note_ids = unique(note_ids);
                let version_ids =
                    Self::reconcile_versions(&next, &note_ids, &[], version_ids, ended_at)?;
                let session_id = id();
                next.sessions.push(Session {
                    id: session_id.clone(),
                    started_at,
                    ended_at,
                    active_seconds: seconds,
                    deadline: ended_at,
                    status: SessionStatus::Learned,
                    summary: summary.trim().into(),
                    suggested_summary: String::new(),
                    note_ids,
                    version_ids,
                    spans: vec![ActiveSpan {
                        start: started_at,
                        end: ended_at,
                    }],
                    updated_at: now,
                    revision: 1,
                });
                Self::ensure_review(&mut next, &session_id)?;
                "学习记录已补记".into()
            }
            Command::EditSession {
                session_id,
                summary,
                learned,
                note_ids,
                version_ids,
            } => {
                Self::notes_exist(&next, &note_ids)?;
                if learned {
                    Self::summary_valid(&summary)?;
                }
                let note_ids = unique(note_ids);
                let index = next
                    .sessions
                    .iter()
                    .position(|s| s.id == session_id)
                    .context("找不到学习记录")?;
                let session = &next.sessions[index];
                let version_ids = Self::reconcile_versions(
                    &next,
                    &note_ids,
                    &session.version_ids,
                    version_ids,
                    session.ended_at,
                )?;
                let session = &mut next.sessions[index];
                session.summary = summary.trim().into();
                session.note_ids = note_ids;
                session.version_ids = version_ids;
                session.status = if learned {
                    SessionStatus::Learned
                } else {
                    SessionStatus::Missed
                };
                session.updated_at = now;
                session.revision += 1;
                if learned {
                    Self::ensure_review(&mut next, &session_id)?;
                } else {
                    let removed: Vec<String> = next
                        .reviews
                        .iter()
                        .filter(|r| r.session_id == session_id)
                        .map(|r| r.id.clone())
                        .collect();
                    next.reviews.retain(|r| r.session_id != session_id);
                    next.review_events
                        .retain(|e| !removed.contains(&e.review_id));
                    next.undo.retain(|u| !removed.contains(&u.review.id));
                }
                "学习记录已更正".into()
            }
            Command::CompleteReview {
                review_id,
                expected_stage,
            } => {
                let review = next
                    .reviews
                    .iter_mut()
                    .find(|r| r.id == review_id)
                    .context("找不到复习计划")?;
                ensure!(
                    review.stage == expected_stage,
                    "复习状态已变化，请刷新后重试"
                );
                ensure!(
                    review.next_review_at.is_some_and(|due| due <= now),
                    "复习尚未到期或本轮已完成"
                );
                let event_id = id();
                next.undo.push(Undo {
                    event_id: event_id.clone(),
                    review: review.clone(),
                });
                next.review_events.push(ReviewEvent {
                    id: event_id,
                    review_id,
                    completed_at: now,
                    stage: review.stage,
                });
                review.stage += 1;
                review.last_review_at = Some(now);
                review.next_review_at = review
                    .intervals
                    .get(review.stage)
                    .map(|d| add_days(now, *d))
                    .transpose()?;
                review.revision += 1;
                "复习已完成".into()
            }
            Command::UndoReview { review_id } => {
                let index = next
                    .undo
                    .iter()
                    .rposition(|u| u.review.id == review_id)
                    .context("没有可撤销的复习")?;
                let undo = next.undo.remove(index);
                let review = next
                    .reviews
                    .iter_mut()
                    .find(|r| r.id == review_id)
                    .context("找不到复习计划")?;
                // Restore only scheduling state: later edits to title/links must survive undo.
                review.stage = undo.review.stage;
                review.last_review_at = undo.review.last_review_at;
                review.next_review_at = undo.review.next_review_at;
                review.revision += 1;
                next.review_events.retain(|e| e.id != undo.event_id);
                // Validate the resulting history before persisting scheduling values
                // copied from a stored prior state. Normal timer writes remain cheap.
                next.validate()?;
                "已撤销上次复习".into()
            }
            Command::Backup(_) | Command::Restore(_) => unreachable!(),
        };
        storage::save(&mut self.conn, &next, self.state.versions.len(), false)?;
        self.state = next;
        Ok(message)
    }
    pub fn tick(&mut self, now: i64, elapsed_seconds: u64) -> Result<()> {
        let mut next = self.state.clone();
        next.expire(now);
        if !next.timer.paused && (1..=10).contains(&elapsed_seconds) && now > next.last_tick {
            // A monotonic elapsed duration is bounded by wall time to prevent overlap or clock rollback.
            let available = now.saturating_sub(next.last_tick) as u64;
            let elapsed = elapsed_seconds.min(available);
            let mut cursor = now.saturating_sub(elapsed as i64);
            let mut remaining = elapsed;
            while remaining > 0 {
                let part = remaining.min(next.timer.target_seconds - next.timer.active_seconds);
                let end = cursor + part as i64;
                if let Some(span) = next.timer.spans.last_mut().filter(|s| s.end == cursor) {
                    span.end = end;
                } else {
                    next.timer.spans.push(ActiveSpan { start: cursor, end });
                }
                next.timer.active_seconds += part;
                cursor = end;
                remaining -= part;
                if next.timer.active_seconds == next.timer.target_seconds {
                    Self::finish_segment(&mut next, end)?;
                }
            }
        }
        next.last_tick = next.last_tick.max(now);
        storage::save(&mut self.conn, &next, self.state.versions.len(), false)?;
        self.state = next;
        Ok(())
    }
    pub fn checkpoint(&mut self) -> Result<()> {
        storage::save(
            &mut self.conn,
            &self.state,
            self.state.versions.len(),
            false,
        )?;
        self.conn.execute_batch("PRAGMA wal_checkpoint(FULL);")?;
        Ok(())
    }
    fn notes_exist(state: &State, ids: &[String]) -> Result<()> {
        ensure!(
            ids.iter().all(|id| state.notes.iter().any(|n| n.id == *id)),
            "关联笔记不存在"
        );
        Ok(())
    }
    fn summary_valid(summary: &str) -> Result<()> {
        ensure!(
            !summary.trim().is_empty() && summary.len() <= 100_000,
            "请填写学习摘要（最多 100000 字节）"
        );
        Ok(())
    }
    fn reconcile_versions(
        state: &State,
        notes: &[String],
        existing: &[String],
        explicit: Option<Vec<String>>,
        ended_at: i64,
    ) -> Result<Vec<String>> {
        let versions: std::collections::HashMap<&str, &NoteVersion> =
            state.versions.iter().map(|v| (v.id.as_str(), v)).collect();
        let explicit_selection = explicit.is_some();
        let mut chosen = if let Some(ids) = explicit {
            let ids = unique(ids);
            for id in &ids {
                let version = versions.get(id.as_str()).context("选择的笔记版本不存在")?;
                ensure!(notes.contains(&version.note_id), "选择的版本不属于关联笔记");
            }
            ids
        } else {
            unique(
                existing
                    .iter()
                    .filter(|id| {
                        versions
                            .get(id.as_str())
                            .is_some_and(|v| notes.contains(&v.note_id))
                    })
                    .cloned()
                    .collect(),
            )
        };
        for note_id in notes {
            if chosen.iter().any(|id| {
                versions
                    .get(id.as_str())
                    .is_some_and(|v| v.note_id == *note_id)
            }) {
                continue;
            }
            ensure!(!explicit_selection, "每份关联笔记至少需要选择一个版本");
            let matching = || state.versions.iter().filter(|v| v.note_id == *note_id);
            let version = matching()
                .filter(|v| v.captured_at <= ended_at)
                .max_by_key(|v| v.captured_at)
                .or_else(|| matching().max_by_key(|v| v.captured_at))
                .context("关联笔记没有可用的内容快照")?;
            chosen.push(version.id.clone());
        }
        Ok(chosen)
    }
    fn ensure_review(state: &mut State, session_id: &str) -> Result<()> {
        let s = state
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .context("找不到学习记录")?;
        if let Some(r) = state
            .reviews
            .iter_mut()
            .find(|r| r.session_id == session_id)
        {
            r.title = s.summary.clone();
            r.note_ids = s.note_ids.clone();
            r.revision += 1;
        } else {
            state.reviews.push(Review {
                id: id(),
                session_id: session_id.into(),
                title: s.summary.clone(),
                note_ids: s.note_ids.clone(),
                learned_at: s.ended_at,
                last_review_at: None,
                next_review_at: Some(add_days(s.ended_at, state.settings.review_days[0])?),
                stage: 0,
                intervals: state.settings.review_days.clone(),
                revision: 1,
            });
        }
        Ok(())
    }
    fn finish_segment(state: &mut State, end: i64) -> Result<()> {
        let versions: Vec<&NoteVersion> = state
            .versions
            .iter()
            .filter(|v| {
                state.active_versions.contains(&v.id)
                    && !v.offline
                    && state
                        .timer
                        .spans
                        .iter()
                        .any(|p| p.start <= v.captured_at && v.captured_at <= p.end)
            })
            .collect();
        let version_ids = versions.iter().map(|v| v.id.clone()).collect();
        let note_ids = unique(versions.iter().map(|v| v.note_id.clone()).collect());
        let suggested_summary = versions
            .iter()
            .map(|v| v.summary.as_str())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let deadline = end
            .checked_add(i64::from(state.settings.answer_minutes) * 60)
            .context("时间超出范围")?;
        state.sessions.push(Session {
            id: id(),
            started_at: state
                .timer
                .spans
                .first()
                .map_or(state.timer.started_at, |p| p.start),
            ended_at: end,
            active_seconds: state.timer.active_seconds,
            deadline,
            status: SessionStatus::Pending,
            summary: String::new(),
            suggested_summary,
            note_ids,
            version_ids,
            spans: std::mem::take(&mut state.timer.spans),
            updated_at: end,
            revision: 1,
        });
        state.active_versions.retain(|id| {
            state
                .versions
                .iter()
                .any(|v| v.id == *id && v.captured_at > end)
        });
        state.timer.active_seconds = 0;
        state.timer.target_seconds = u64::from(state.settings.interval_minutes) * 60;
        state.timer.started_at = end;
        Ok(())
    }
    fn safe_destination(&self, path: &Path) -> Result<PathBuf> {
        let absolute = if path.exists() {
            std::fs::canonicalize(path)?
        } else {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            std::fs::canonicalize(parent)?.join(path.file_name().context("请选择备份文件")?)
        };
        let live = self.path.to_string_lossy();
        let destination = absolute.to_string_lossy();
        let equal = |a: &str, b: &str| {
            if cfg!(windows) {
                a.eq_ignore_ascii_case(b)
            } else {
                a == b
            }
        };
        ensure!(
            !equal(&destination, &live)
                && !equal(&destination, &format!("{live}-wal"))
                && !equal(&destination, &format!("{live}-shm")),
            "备份或恢复路径不能是正在使用的资料库及其 WAL/SHM 文件"
        );
        Ok(absolute)
    }
    fn backup(&mut self, path: &Path) -> Result<()> {
        let destination = self.safe_destination(path)?;
        ensure!(
            !destination.exists(),
            "备份文件已存在，请选择新文件名以保留原备份"
        );
        self.checkpoint()?;
        let mut to = Connection::open(&destination)?;
        let backup = rusqlite::backup::Backup::new(&self.conn, &mut to)?;
        backup.run_to_completion(128, std::time::Duration::from_millis(5), None)?;
        Ok(())
    }
    fn restore(&mut self, path: &Path, now: i64) -> Result<String> {
        let source = self.safe_destination(path)?;
        if !source.is_file() {
            bail!("找不到备份文件");
        }
        let mut restored = storage::verified_backup(&source)?;
        let pre = self
            .path
            .with_file_name(format!("studyapp-before-restore-{now}-{}.sqlite", id()));
        self.backup(&pre)?;
        restored.last_tick = restored.last_tick.max(now);
        restored.candidates.clear();
        restored.expire(now);
        crate::notes::scan(&mut restored, true, now);
        storage::save(&mut self.conn, &restored, 0, true)?;
        self.state = restored;
        Ok(format!("资料库已恢复；恢复前备份：{}", pre.display()))
    }
}
fn unique(ids: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}
