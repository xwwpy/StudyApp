//! Persistent learning rules, independent of the desktop interface.
mod engine;
mod folders;
pub mod integration;
mod managed;
mod models;
mod notes;
mod stats;
mod storage;
pub use engine::Engine;
pub use models::*;
pub use stats::daily_stats;
