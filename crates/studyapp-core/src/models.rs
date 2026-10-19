use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    pub interval_minutes: u32,
    pub answer_minutes: u32,
    pub review_days: Vec<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionStatus {
    Pending,
    Learned,
    Missed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub path: String,
    pub title: String,
    pub missing: bool,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub revision: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoteVersion {
    pub id: String,
    pub note_id: String,
    pub content: String,
    pub hash: String,
    pub captured_at: i64,
    pub offline: bool,
    pub summary: String,
    pub diff: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveSpan {
    pub start: i64,
    pub end: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub active_seconds: u64,
    pub deadline: i64,
    pub status: SessionStatus,
    pub summary: String,
    pub suggested_summary: String,
    pub note_ids: Vec<String>,
    pub version_ids: Vec<String>,
    pub spans: Vec<ActiveSpan>,
    pub updated_at: i64,
    pub revision: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Review {
    pub id: String,
    pub session_id: String,
    pub title: String,
    pub note_ids: Vec<String>,
    pub learned_at: i64,
    pub last_review_at: Option<i64>,
    pub next_review_at: Option<i64>,
    pub stage: usize,
    pub intervals: Vec<u32>,
    pub revision: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewEvent {
    pub id: String,
    pub review_id: String,
    pub completed_at: i64,
    pub stage: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimerState {
    pub paused: bool,
    pub active_seconds: u64,
    pub target_seconds: u64,
    pub started_at: i64,
    pub spans: Vec<ActiveSpan>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub settings: Settings,
    pub notes: Vec<Note>,
    pub versions: std::sync::Arc<Vec<NoteVersion>>,
    pub sessions: Vec<Session>,
    pub reviews: Vec<Review>,
    pub review_events: Vec<ReviewEvent>,
    pub timer: TimerState,
    pub data_path: String,
}
#[derive(Clone, Debug)]
pub enum Command {
    Import(Vec<PathBuf>),
    Relink {
        note_id: String,
        path: PathBuf,
    },
    SaveSettings(Settings),
    SetPaused(bool),
    Submit {
        session_id: String,
        summary: String,
        note_ids: Vec<String>,
    },
    Skip {
        session_id: String,
    },
    Manual {
        started_at: i64,
        minutes: u32,
        summary: String,
        note_ids: Vec<String>,
    },
    EditSession {
        session_id: String,
        summary: String,
        learned: bool,
        note_ids: Vec<String>,
    },
    CompleteReview {
        review_id: String,
        expected_stage: usize,
    },
    UndoReview {
        review_id: String,
    },
    Backup(PathBuf),
    Restore(PathBuf),
    Scan {
        offline: bool,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DayStat {
    pub date: String,
    pub learned_seconds: u64,
    pub learned_count: usize,
    pub missed_count: usize,
    pub review_count: usize,
}
