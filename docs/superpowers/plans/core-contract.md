# Shared core / desktop contract

All types exported at `studyapp_core::*`. Owned strings and vectors, Clone + Debug; persistent models Serialize + Deserialize. All timestamps `i64` UTC Unix seconds, IDs `String`. Errors `anyhow::Result`. No GPUI dependency in core.

```rust
pub struct Settings {
    pub interval_minutes: u32, // default 60, range 5..=240
    pub answer_minutes: u32, // default 5, range 1..=30
    pub review_days: Vec<u32>, // default [1,3,7,14,30], positive <=3650, length 1..=20
}
pub enum SessionStatus { Pending, Learned, Missed }
pub struct Note { pub id: String, pub path: String, pub title: String, pub missing: bool, pub error: Option<String>, pub created_at: i64, pub updated_at: i64, pub revision: u64 }
pub struct NoteVersion { pub id: String, pub note_id: String, pub content: String, pub hash: String, pub captured_at: i64, pub offline: bool, pub summary: String, pub diff: String }
pub struct ActiveSpan { pub start: i64, pub end: i64 }
pub struct Session { pub id: String, pub started_at: i64, pub ended_at: i64, pub active_seconds: u64, pub deadline: i64, pub status: SessionStatus, pub summary: String, pub suggested_summary: String, pub note_ids: Vec<String>, pub version_ids: Vec<String>, pub spans: Vec<ActiveSpan>, pub updated_at: i64, pub revision: u64 }
pub struct Review { pub id: String, pub session_id: String, pub title: String, pub note_ids: Vec<String>, pub learned_at: i64, pub last_review_at: Option<i64>, pub next_review_at: Option<i64>, pub stage: usize, pub intervals: Vec<u32>, pub revision: u64 }
pub struct ReviewEvent { pub id: String, pub review_id: String, pub completed_at: i64, pub stage: usize }
pub struct TimerState { pub paused: bool, pub active_seconds: u64, pub target_seconds: u64, pub started_at: i64, pub spans: Vec<ActiveSpan> }
pub struct Snapshot { pub settings: Settings, pub notes: Vec<Note>, pub versions: std::sync::Arc<Vec<NoteVersion>>, pub sessions: Vec<Session>, pub reviews: Vec<Review>, pub review_events: Vec<ReviewEvent>, pub timer: TimerState, pub data_path: String }
pub enum Command {
    Import(Vec<std::path::PathBuf>),
    Relink { note_id: String, path: std::path::PathBuf },
    SaveSettings(Settings),
    SetPaused(bool),
    Submit { session_id: String, summary: String, note_ids: Vec<String>, version_ids: Option<Vec<String>> },
    Skip { session_id: String },
    Manual { started_at: i64, minutes: u32, summary: String, note_ids: Vec<String>, version_ids: Option<Vec<String>> },
    EditSession { session_id: String, summary: String, learned: bool, note_ids: Vec<String>, version_ids: Option<Vec<String>> },
    CompleteReview { review_id: String, expected_stage: usize },
    UndoReview { review_id: String },
    Backup(std::path::PathBuf),
    Restore(std::path::PathBuf),
    Scan { offline: bool },
}
pub struct Engine { /* private */ }
impl Engine {
    pub fn open(database: &std::path::Path, now: i64) -> anyhow::Result<Self>;
    pub fn snapshot(&self) -> Snapshot;
    pub fn apply(&mut self, command: Command, now: i64) -> anyhow::Result<String>; // human-readable Chinese status
    pub fn tick(&mut self, now: i64, elapsed_seconds: u64) -> anyhow::Result<()>; // 0 or >10 elapsed => exclude activity gap, still expire pending
    pub fn checkpoint(&mut self) -> anyhow::Result<()>;
}
pub struct DayStat { pub date: String, pub learned_seconds: u64, pub learned_count: usize, pub missed_count: usize, pub review_count: usize }
pub fn daily_stats(snapshot: &Snapshot, offset_seconds: i32, end_date: &str, days: usize) -> Vec<DayStat>;
```

`daily_stats` fixed offset is usable for deterministic tests; desktop local DST calendar can request offset of each date separately if needed. Split active spans across midnight. Midnight counts sessions by ending day. Review counts use event dates.

Core owns transactions and validates every command. `Import` may report partial failures in status while committing successful files; no content deletion. Debounce is polling stable hashes: background scans every 3 seconds. New files are baseline versions, subsequent scanned versions have diffs. Only non-offline versions in a session's active spans contribute suggested summary. UI uses snapshot versions for Markdown and history (no file reads on render). Raw HTML and remote image fetching disabled at UI render layer.

Rules to resolve edge cases: setting new interval when timer has zero progress updates target immediately, otherwise next cycle; Submit only Pending before deadline; manual/correction handles missed entries explicitly; changing summary/notes updates an existing review title and links without erasing completed review history; learned->missed correction archives/removes its review and associated events transactionally (DB records, never filesystem deletion). Repeated complete uses expected_stage to reject stale clicks, and rejects future due dates. Undo restores prior due time exactly, using persisted previous values internally. Backup destination must not be live DB or its WAL/SHM; restore requires app schema ID/integrity, pre-restore backup and reload. Restored clock excludes time outside active app runtime. Failure must leave in-memory state and disk consistent.

Runtime state should also retain accumulated active version associations across pause/restart. Source polling before ticking segment boundary ensures changes fall in the ending segment. Timeout runs on wall clock even while paused. Stop worker performs checkpoint. No file system deletion in implementation.
