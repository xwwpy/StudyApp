//! Immutable, GPUI-independent presentation prepared with a storage scan.
use super::{CopyState, StorageReport};
use std::collections::HashMap;
use studyapp_desktop::presentation::display_path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageRow {
    Category(usize),
    ImageSummary,
    ImageLocation(usize),
    IssuesHeading,
    Issue(usize),
    CopiesHeading,
    NoCopies,
    Copy(usize),
}

#[derive(Clone, Debug)]
pub struct StorageCategory {
    pub title: &'static str,
    pub bytes: String,
    pub description: &'static str,
}

#[derive(Clone, Debug)]
pub struct StorageIssueView {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct ManagedCopyView {
    pub note_id: String,
    pub title: String,
    pub path: String,
    pub bytes: String,
    pub state: CopyState,
    pub deletable: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct StoragePresentation {
    pub logical_label: String,
    pub scanned_label: String,
    pub categories: Vec<StorageCategory>,
    pub image_summary: String,
    pub image_locations: Vec<String>,
    pub issues: Vec<StorageIssueView>,
    pub copies: Vec<ManagedCopyView>,
    copy_indices: HashMap<String, usize>,
    rows_with_paths: Vec<StorageRow>,
    rows_without_paths: Vec<StorageRow>,
}

impl StoragePresentation {
    pub fn prepare(report: &StorageReport) -> Self {
        let timestamp = chrono::DateTime::from_timestamp(report.scanned_at, 0)
            .map(|time| {
                time.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_else(|| "尚未统计".into());
        let categories = [
            (
                "数据库与历史",
                report.database_bytes,
                "笔记历史、学习记录与数据库日志",
            ),
            (
                "托管笔记",
                report.managed_bytes,
                "当前资料库自有的 Markdown 副本",
            ),
            (
                "本地图片",
                report.image_bytes,
                "库内图片及索引登记的当前、历史位置",
            ),
            ("草稿", report.draft_bytes, "未提交的笔记草稿，支持多份草稿"),
            (
                "保留与恢复文件",
                report.retained_bytes,
                "发布暂存、原始副本及未归属文件",
            ),
            ("备份", report.backup_bytes, "资料库目录内的数据库备份"),
            (
                "配置与其他",
                report.auxiliary_bytes,
                "设置版本、图片索引与其他文件",
            ),
        ]
        .into_iter()
        .map(|(title, bytes, description)| StorageCategory {
            title,
            bytes: size_label(bytes),
            description,
        })
        .collect::<Vec<_>>();
        let copies = report
            .copies
            .iter()
            .map(|copy| ManagedCopyView {
                note_id: copy.note_id.clone(),
                title: copy.title.clone(),
                path: display_path(&copy.path),
                bytes: size_label(copy.bytes),
                state: copy.state,
                deletable: copy.deletable,
                reason: copy.reason.clone(),
            })
            .collect::<Vec<_>>();
        let copy_indices = copies
            .iter()
            .enumerate()
            .map(|(ix, copy)| (copy.note_id.clone(), ix))
            .collect();
        let mut rows_with_paths = Vec::with_capacity(
            categories.len()
                + report.image_locations.len()
                + report.issues.len()
                + copies.len()
                + 4,
        );
        rows_with_paths.extend((0..categories.len()).map(StorageRow::Category));
        if !report.image_locations.is_empty() {
            rows_with_paths.push(StorageRow::ImageSummary);
            rows_with_paths
                .extend((0..report.image_locations.len()).map(StorageRow::ImageLocation));
        }
        if report.incomplete {
            rows_with_paths.push(StorageRow::IssuesHeading);
            rows_with_paths.extend((0..report.issues.len()).map(StorageRow::Issue));
        }
        rows_with_paths.push(StorageRow::CopiesHeading);
        if copies.is_empty() {
            rows_with_paths.push(StorageRow::NoCopies);
        } else {
            rows_with_paths.extend((0..copies.len()).map(StorageRow::Copy));
        }
        let rows_without_paths = rows_with_paths
            .iter()
            .copied()
            .filter(|row| !matches!(row, StorageRow::ImageLocation(_)))
            .collect();
        Self {
            logical_label: format!(
                "已统计逻辑大小 {}{}",
                size_label(report.logical_bytes()),
                if report.incomplete {
                    "（统计不完整）"
                } else {
                    ""
                }
            ),
            scanned_label: format!("统计时间：{timestamp} · 包含已登记的外置图片"),
            categories,
            image_summary: format!(
                "已登记图片位置：{} 处；只统计已知资产，不扫描外置目录中的其他文件。",
                report.image_locations.len()
            ),
            image_locations: report
                .image_locations
                .iter()
                .map(|path| display_path(path))
                .collect(),
            issues: report
                .issues
                .iter()
                .map(|issue| StorageIssueView {
                    path: display_path(&issue.path),
                    message: issue.message.clone(),
                })
                .collect(),
            copies,
            copy_indices,
            rows_with_paths,
            rows_without_paths,
        }
    }

    pub fn rows(&self, show_paths: bool) -> &[StorageRow] {
        if show_paths {
            &self.rows_with_paths
        } else {
            &self.rows_without_paths
        }
    }

    pub fn copy_by_id(&self, note_id: &str) -> Option<&ManagedCopyView> {
        self.copy_indices.get(note_id).map(|&ix| &self.copies[ix])
    }

    /// Locate the same visible item after a fresh scan or path visibility change.
    pub fn relocate_row(
        &self,
        previous: &Self,
        row: StorageRow,
        show_paths: bool,
    ) -> Option<usize> {
        let row =
            match row {
                StorageRow::Copy(ix) => {
                    let copy = previous.copies.get(ix)?;
                    let &next_ix = self.copy_indices.get(&copy.note_id)?;
                    // Copies occupy the final contiguous range; identity lookup is O(1).
                    return Some(self.rows(show_paths).len() - self.copies.len() + next_ix);
                }
                StorageRow::ImageLocation(ix) => {
                    let location = previous.image_locations.get(ix)?;
                    StorageRow::ImageLocation(
                        self.image_locations
                            .iter()
                            .position(|path| path == location)?,
                    )
                }
                StorageRow::Issue(ix) => {
                    let issue = previous.issues.get(ix)?;
                    StorageRow::Issue(self.issues.iter().position(|next| {
                        next.path == issue.path && next.message == issue.message
                    })?)
                }
                row => row,
            };
        self.rows(show_paths).iter().position(|&next| next == row)
    }

    /// Preserve context when a refresh or path toggle removes the visible row.
    pub fn relocate_or_nearest(
        &self,
        previous: &Self,
        row: StorageRow,
        show_paths: bool,
    ) -> Option<usize> {
        if let Some(exact) = self.relocate_row(previous, row, show_paths) {
            return Some(exact);
        }
        let rows = self.rows(show_paths);
        let section = |target| rows.iter().position(|&candidate| candidate == target);
        match row {
            StorageRow::ImageLocation(ix) => {
                previous.image_locations.get(ix)?;
                section(StorageRow::ImageSummary)
                    .or_else(|| section(StorageRow::IssuesHeading))
                    .or_else(|| section(StorageRow::CopiesHeading))
            }
            StorageRow::Issue(ix) => {
                previous.issues.get(ix)?;
                section(StorageRow::IssuesHeading).or_else(|| section(StorageRow::CopiesHeading))
            }
            StorageRow::Copy(ix) => {
                previous.copies.get(ix)?;
                // A scan can remove several copies at once. Search old neighbors
                // by identity using O(1) destination lookups, not repeated row scans.
                let neighbor = previous.copies[ix + 1..]
                    .iter()
                    .chain(previous.copies[..ix].iter().rev())
                    .find_map(|copy| self.copy_indices.get(&copy.note_id));
                neighbor
                    .map(|&index| rows.len() - self.copies.len() + index)
                    .or_else(|| section(StorageRow::NoCopies))
                    .or_else(|| section(StorageRow::CopiesHeading))
            }
            StorageRow::ImageSummary => {
                section(StorageRow::IssuesHeading).or_else(|| section(StorageRow::CopiesHeading))
            }
            StorageRow::IssuesHeading | StorageRow::NoCopies => section(StorageRow::CopiesHeading),
            StorageRow::Category(_) | StorageRow::CopiesHeading => None,
        }
    }
}

fn size_label(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024. * 1024.))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ManagedCopy, StorageIssue};
    use super::*;

    fn report(count: usize) -> StorageReport {
        StorageReport {
            database_bytes: 1024,
            managed_bytes: 2048,
            scanned_at: 1_700_000_000,
            copies: (0..count)
                .map(|ix| ManagedCopy {
                    note_id: format!("copy-{ix}"),
                    title: format!("托管副本 {ix}"),
                    path: format!(r"\\?\C:\library\notes\copy-{ix}.md"),
                    bytes: 2048,
                    state: CopyState::Present,
                    deletable: true,
                    reason: None,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn storage_presentation_prepares_totals_and_display_paths() {
        let report = report(1);
        let view = StoragePresentation::prepare(&report);
        assert_eq!(view.logical_label, "已统计逻辑大小 3.0 KiB");
        assert!(view.scanned_label.starts_with("统计时间："));
        assert_eq!(view.categories.len(), 7);
        let copy = view.copy_by_id("copy-0").unwrap();
        assert_eq!(copy.path, r"C:\library\notes\copy-0.md");
        assert_eq!(copy.bytes, "2.0 KiB");
        assert_eq!(report.copies[0].path, r"\\?\C:\library\notes\copy-0.md");
    }

    #[test]
    fn storage_presentation_rows_scale_without_materializing_copies() {
        for count in [100, 1000, 10_000] {
            let view = StoragePresentation::prepare(&report(count));
            assert_eq!(view.rows(false).len(), count + 8);
            assert_eq!(view.rows(true).len(), count + 8);
            let viewport = &view.rows(true)[count..count + 8];
            assert_eq!(viewport.len(), 8);
            assert_eq!(viewport.last(), Some(&StorageRow::Copy(count - 1)));
            assert!(
                view.copy_by_id(&format!("copy-{}", count - 1))
                    .unwrap()
                    .deletable
            );
        }
    }

    #[test]
    fn storage_presentation_preserves_copy_identity_after_cleanup() {
        let mut report = report(2);
        report.copies[1].state = CopyState::Cleaned;
        report.copies[1].deletable = false;
        report.copies[1].bytes = 0;
        report.copies[1].reason = Some("副本已清理，历史与学习记录仍保留。".into());
        let view = StoragePresentation::prepare(&report);
        let cleaned = view.copy_by_id("copy-1").unwrap();
        assert_eq!(cleaned.state, CopyState::Cleaned);
        assert!(!cleaned.deletable);
        assert_eq!(cleaned.bytes, "0 B");
        assert!(cleaned.reason.as_ref().unwrap().contains("历史"));
        assert_eq!(view.rows(false).last(), Some(&StorageRow::Copy(1)));
    }

    #[test]
    fn storage_presentation_path_visibility_only_changes_path_rows() {
        let mut report = report(1);
        report.image_locations = vec![r"\\?\C:\images".into(), r"\\?\D:\images".into()];
        report.incomplete = true;
        report.issues.push(StorageIssue {
            path: r"\\?\D:\images".into(),
            message: "无法访问".into(),
        });
        let view = StoragePresentation::prepare(&report);
        assert!(view.logical_label.ends_with("（统计不完整）"));
        assert_eq!(view.rows(true).len(), view.rows(false).len() + 2);
        assert!(
            !view
                .rows(false)
                .iter()
                .any(|row| matches!(row, StorageRow::ImageLocation(_)))
        );
        assert!(view.rows(false).contains(&StorageRow::Issue(0)));
        assert_eq!(view.issues[0].path, r"D:\images");
        assert_eq!(view.image_locations[0], r"C:\images");
    }

    #[test]
    fn storage_presentation_restores_scroll_anchor_by_note_identity() {
        let mut source = report(3);
        source.image_locations = vec!["C:/images".into()];
        let previous = StoragePresentation::prepare(&source);
        source.copies.swap(0, 2);
        source.copies[0].state = CopyState::Cleaned;
        source.copies[0].deletable = false;
        let next = StoragePresentation::prepare(&source);
        let old_row = StorageRow::Copy(2);
        let row = next.relocate_row(&previous, old_row, false).unwrap();
        assert_eq!(next.rows(false)[row], StorageRow::Copy(0));
        assert_eq!(next.copies[0].note_id, "copy-2");
        let row_with_path = next.relocate_row(&previous, old_row, true).unwrap();
        assert_eq!(row_with_path, row + 1);
        assert_eq!(
            next.relocate_row(&previous, StorageRow::ImageLocation(0), false),
            None
        );
    }

    #[test]
    fn removed_path_and_resolved_issue_keep_their_nearest_section_anchor() {
        let mut source = report(3);
        source.image_locations = vec!["C:/images-a".into(), "D:/images-b".into()];
        source.incomplete = true;
        source.issues.push(StorageIssue {
            path: "D:/images-b".into(),
            message: "无法访问".into(),
        });
        let previous = StoragePresentation::prepare(&source);
        let hidden = previous.relocate_or_nearest(&previous, StorageRow::ImageLocation(1), false);
        assert_eq!(
            hidden.map(|index| previous.rows(false)[index]),
            Some(StorageRow::ImageSummary),
            "Hiding paths must keep the image section in view, not reset to the first category"
        );
        source.incomplete = false;
        source.issues.clear();
        let resolved = StoragePresentation::prepare(&source);
        let refreshed = resolved.relocate_or_nearest(&previous, StorageRow::Issue(0), true);
        assert_eq!(
            refreshed.map(|index| resolved.rows(true)[index]),
            Some(StorageRow::CopiesHeading),
            "A resolved issue must fall forward to the following section"
        );
    }

    #[test]
    fn removed_copy_anchor_uses_next_surviving_identity_then_previous_or_empty_state() {
        let mut source = report(4);
        let previous = StoragePresentation::prepare(&source);
        source.copies.remove(1);
        let next = StoragePresentation::prepare(&source);
        let anchored = next
            .relocate_or_nearest(&previous, StorageRow::Copy(1), false)
            .unwrap();
        assert_eq!(next.rows(false)[anchored], StorageRow::Copy(1));
        assert_eq!(next.copies[1].note_id, "copy-2");
        source.copies.pop();
        let shorter = StoragePresentation::prepare(&source);
        let anchored = shorter
            .relocate_or_nearest(&previous, StorageRow::Copy(3), false)
            .unwrap();
        assert_eq!(shorter.rows(false)[anchored], StorageRow::Copy(1));
        assert_eq!(shorter.copies[1].note_id, "copy-2");
        source.copies.clear();
        let empty = StoragePresentation::prepare(&source);
        let anchored = empty
            .relocate_or_nearest(&previous, StorageRow::Copy(3), false)
            .unwrap();
        assert_eq!(empty.rows(false)[anchored], StorageRow::NoCopies);
    }
}
