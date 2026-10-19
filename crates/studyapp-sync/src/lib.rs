//! Bounded, conditional synchronization of UUID-named Markdown notes.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;

mod local;
mod planner;
mod webdav;
pub use local::LocalFolderProvider;
pub use planner::plan_sync;
pub use webdav::WebDavProvider;

pub const MAX_CONTENT_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_XML_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_SYNC_BYTES: usize = 64 * 1024 * 1024;
pub type Result<T> = std::result::Result<T, SyncError>;

#[derive(Debug)]
pub enum SyncError {
    InvalidId,
    InvalidPath,
    InvalidUrl,
    InvalidContent,
    LimitExceeded,
    Conflict,
    MissingRevision,
    InvalidXml,
    Http(u16),
    Transport,
    Io(std::io::ErrorKind),
    LockBusy,
}
impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRevision => write!(f, "WebDAV 未提供强 ETag，已拒绝覆盖现有笔记"),
            Self::Conflict => write!(f, "远端内容已更改，请重新同步；未覆盖未知版本"),
            Self::LockBusy => write!(f, "同步目录正在被其他 StudyApp 使用，请稍后重试"),
            _ => write!(f, "同步失败：{self:?}"),
        }
    }
}
impl std::error::Error for SyncError {}
impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.kind())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteNote {
    pub content: String,
    pub hash: String,
    pub revision: Option<String>,
}

/// `None` means create-only. Existing objects require a previously read revision.
/// Implementations deliberately expose no deletion operation.
pub trait Provider: Send + Sync {
    fn list(&self) -> Result<Vec<String>>;
    fn read(&self, id: &str) -> Result<Option<RemoteNote>>;
    fn write(&self, id: &str, content: &str, expected_revision: Option<&str>)
    -> Result<RemoteNote>;
    fn test_connection(&self) -> Result<()>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncNote {
    pub id: String,
    pub title: String,
    pub content: String,
    pub hash: String,
    pub managed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Baseline {
    pub local_hash: String,
    pub remote_hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotePlan {
    pub id: String,
    pub expected_local_hash: Option<String>,
    pub action: SyncAction,
    pub next_baseline: Baseline,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SyncAction {
    Unchanged,
    Push {
        content: String,
        expected_revision: Option<String>,
    },
    Pull {
        content: String,
    },
    Conflict {
        local_content: String,
        remote_content: String,
    },
    ImportRemote {
        content: String,
    },
    CopyExternal {
        content: String,
    },
}
pub fn content_hash(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

fn validate_id(id: &str) -> Result<()> {
    let uuid = uuid::Uuid::parse_str(id).map_err(|_| SyncError::InvalidId)?;
    if uuid.to_string() != id {
        return Err(SyncError::InvalidId);
    }
    Ok(())
}
fn validate_content(content: &str) -> Result<()> {
    if content.len() > MAX_CONTENT_BYTES {
        return Err(SyncError::LimitExceeded);
    }
    if content.contains('\0') {
        return Err(SyncError::InvalidContent);
    }
    Ok(())
}
fn bounded_read(reader: impl Read, limit: usize) -> Result<String> {
    let mut bytes = Vec::new();
    reader.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(SyncError::LimitExceeded);
    }
    String::from_utf8(bytes).map_err(|_| SyncError::InvalidContent)
}

#[cfg(test)]
mod tests;
