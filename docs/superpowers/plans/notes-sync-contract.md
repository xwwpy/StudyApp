# 0.2 shared contract

Core keeps existing Engine::apply -> Result<String> and Snapshot interfaces. Add serde-defaulted `Note.managed: bool` (old/imported=false). Managed note paths are always derived from the current database parent + `notes/<UUID>.md`.

Add commands:

```rust
CreateNote { title: String, content: String },
SaveNote { note_id: String, expected_hash: String, content: String },
ReceiveSyncedNote { note_id: String, expected_hash: Option<String>, content: String },
ExportNote { note_id: String, version_id: Option<String>, path: PathBuf },
```

CreateNote allocates UUID, SaveNote only edits managed notes and checks latest snapshot and actual file against expected hash. ReceiveSyncedNote uses supplied validated UUID, None requires absent note, Some requires matching existing managed note; records incoming version as offline, never active study credit. Imported external sources never written. ExportNote requires a new destination and never overwrites. A no-op save must not add a version. Title/content bounded (title <=200 chars, content <=10MiB). Core implementer owns transactional/recovery design and must report it before consumers rely on failure behavior. Snapshot order lets caller identify newly created notes by before/after IDs; no parsing Chinese messages.

Implemented persistence contract: schema 2 migrates schema 1 on open, and accepts schema 1 backups. A SQLite `write_intents` row durably retains the proposed state and new version contents before file publication. Existing managed originals are renamed to unique `.original-*.md` files; fully written/fsynced `.pending-*.md` files are published via same-directory create-only hard links. These retained files are never deleted. The final SQLite transaction installs state/versions and clears the intent. NTFS supports this publication path. A cached per-Engine hard-link capability probe runs in the database directory before the first write intent; unsupported filesystems fail before journaling and leave existing library commands usable. Its tiny random probe files are retained, not deleted.

Early CAS failures leave state/file untouched and require the editor to retain its draft. A later filesystem or final database failure returns Err, preserves the intent, and keeps the previous in-memory snapshot; the next command/tick or restart retries it. Persistent filesystem failure can prevent Engine::open until the path or file lock problem is resolved; the returned error explains that original/draft contents are retained. Do not equate an Err with “nothing was written.” A replay that observes new external content leaves that file in place and finalizes both the intended draft and external content as versions, with external content latest/offline and a readable Note.error; a running command receives Err for this conflict. Restarts still reconcile all other sources offline.

Managed paths are derived from the current database directory on open/restore, never trusted from a backup. Restore uses the same intent protocol, retains local conflicting managed content as versions before restoring the backup snapshot, and preserves a pre-restore database backup. Imported sources remain read-only. Supplied UUIDs use canonical lowercase hyphenated spelling. Worker acknowledgement should identify the newly created note using snapshot ID differences, while failed forms retain draft content.

Sync crate is independent of GPUI/core. Public types (implementer may refine before integration, report exact API):

- Provider trait: list UUID Markdown objects, read returning content + revision token, write with expected token (None=create-only), test connection. No remove API.
- Local folder provider isolates files under chosen root/StudyApp/notes, protects StudyApp writers and compares hashes; WebDAV provider same prefix, Depth:1 PROPFIND and HTTP conditional writes. HTTPS only except loopback fixtures, no redirects, bounded responses and timeouts.
- Sync input: immutable note snapshots {id, title, content, hash, managed}, baseline per ID {local_hash, remote_hash}; report includes pushes/pulls/new notes/conflict copies and next baselines. Network planner must not hold mutable Engine or block its worker.
- Sync actions returned to parent are applied with core CAS commands. Baselines advance only after actions succeed. Local changed after network snapshot is a surfaced retry/conflict, never discarded.
- If both managed sides changed: preserve local content as a new conflict note before receiving remote original; external-source pull always creates a managed copy and leaves source untouched. No deletions propagate.

Desktop UI owns note editor entities and draft handling; root owns worker + sync integration. UI communicates via existing Work::Apply. Add Update.note_result: Option<Result<String,String>> (success note ID, error) for create/save; parent supplies this. Pending form stays until acknowledgement. UI exports via existing picker action extended with note ID/version. Root sends Update.sync_status separately. No credentials in Snapshot/core backups.
