use crate::components::*;
use gpui_kit::{
    component::{
        Disableable, Selectable, Sizable,
        button::{Button, ButtonVariants},
        input::{Input, InputState},
    },
    *,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use studyapp_core::{Command, MAX_FOLDER_DEPTH, Note, NoteFolder, Snapshot};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FolderFilter {
    #[default]
    All,
    Root,
    Folder(String),
}

impl FolderFilter {
    /// Folder membership only. The caller keeps its usual detached/search filters.
    pub fn matches(&self, note: &Note) -> bool {
        match self {
            Self::All => true,
            Self::Root => note.folder_id.is_none(),
            Self::Folder(id) => note.folder_id.as_ref() == Some(id),
        }
    }

    pub fn folder_id(&self) -> Option<&str> {
        match self {
            Self::Folder(id) => Some(id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum FolderEvent {
    Filter(FolderFilter),
    Command(Command),
}

#[derive(Clone, Debug)]
struct FolderRow {
    id: String,
    name: String,
    path: String,
    depth: usize,
    ancestors: Vec<String>,
    has_children: bool,
}

#[derive(Clone)]
enum FolderAction {
    Create {
        parent_id: Option<String>,
    },
    Rename {
        folder_id: String,
    },
    MoveFolder {
        folder_id: String,
        destination: Option<String>,
    },
    MoveNote {
        note_id: String,
        destination: Option<String>,
    },
    Delete {
        folder_id: String,
    },
}

/// A bounded native sidebar. Give its parent a height; the tree scrolls within it.
///
/// `begin_move_note` lets the host's selected-note toolbar reuse this panel's
/// destination picker. All mutations are events; this component performs no IO.
pub struct FolderPanel {
    snapshot: Option<Arc<Snapshot>>,
    rows: Vec<FolderRow>,
    counts: HashMap<String, usize>,
    all_count: usize,
    root_count: usize,
    filter: FolderFilter,
    collapsed: HashSet<String>,
    name: Entity<InputState>,
    action: Option<FolderAction>,
    error: Option<String>,
    busy: bool,
}

impl EventEmitter<FolderEvent> for FolderPanel {}

impl FolderPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("目录名称"));
        cx.subscribe(
            &name,
            |_, _, _: &gpui_kit::component::input::InputEvent, cx| cx.notify(),
        )
        .detach();
        Self {
            snapshot: None,
            rows: Vec::new(),
            counts: HashMap::new(),
            all_count: 0,
            root_count: 0,
            filter: FolderFilter::All,
            collapsed: HashSet::new(),
            name,
            action: None,
            error: None,
            busy: false,
        }
    }

    pub fn filter(&self) -> &FolderFilter {
        &self.filter
    }

    pub fn set_filter(&mut self, filter: FolderFilter, cx: &mut Context<Self>) {
        let filter = match filter {
            FolderFilter::Folder(id) if !self.rows.iter().any(|row| row.id == id) => {
                FolderFilter::All
            }
            filter => filter,
        };
        if let FolderFilter::Folder(id) = &filter
            && let Some(row) = self.rows.iter().find(|row| &row.id == id)
        {
            for ancestor in &row.ancestors {
                self.collapsed.remove(ancestor);
            }
        }
        self.action = None;
        self.error = None;
        if self.filter != filter {
            self.filter = filter;
            cx.emit(FolderEvent::Filter(self.filter.clone()));
        }
        cx.notify();
    }

    pub fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        if self.busy != busy {
            self.busy = busy;
            cx.notify();
        }
    }

    pub fn accept(&mut self, snapshot: Arc<Snapshot>, window: &mut Window, cx: &mut Context<Self>) {
        let library_changed = self
            .snapshot
            .as_ref()
            .is_some_and(|old| old.data_path != snapshot.data_path);
        let folders_changed = library_changed
            || self.snapshot.as_ref().is_none_or(|old| {
                old.folders.len() != snapshot.folders.len()
                    || old.folders.iter().zip(&snapshot.folders).any(|(a, b)| {
                        a.id != b.id || a.name != b.name || a.parent_id != b.parent_id
                    })
            });
        if folders_changed {
            self.rows = folder_rows(&snapshot.folders);
            self.collapsed
                .retain(|id| snapshot.folders.iter().any(|folder| &folder.id == id));
        }
        self.counts.clear();
        self.all_count = 0;
        self.root_count = 0;
        for note in snapshot.notes.iter().filter(|note| !note.detached) {
            self.all_count += 1;
            if let Some(folder_id) = &note.folder_id {
                *self.counts.entry(folder_id.clone()).or_default() += 1;
            } else {
                self.root_count += 1;
            }
        }
        if library_changed {
            self.collapsed.clear();
            self.action = None;
            self.error = None;
            self.name
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
        if self.action.as_ref().is_some_and(|action| {
            let folder_id = match action {
                FolderAction::Rename { folder_id }
                | FolderAction::MoveFolder { folder_id, .. }
                | FolderAction::Delete { folder_id } => Some(folder_id),
                FolderAction::Create { parent_id } => parent_id.as_ref(),
                FolderAction::MoveNote { note_id, .. } => {
                    return !snapshot.notes.iter().any(|note| &note.id == note_id);
                }
            };
            folder_id.is_some_and(|id| !snapshot.folders.iter().any(|folder| &folder.id == id))
        }) {
            self.action = None;
            self.error = None;
        }
        self.snapshot = Some(snapshot);
        if library_changed
            || self
                .filter
                .folder_id()
                .is_some_and(|id| !self.rows.iter().any(|row| row.id == id))
        {
            self.filter = FolderFilter::All;
            cx.emit(FolderEvent::Filter(self.filter.clone()));
        }
        cx.notify();
    }

    pub fn begin_move_note(&mut self, note_id: String, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if let Some(note) = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.notes.iter().find(|note| note.id == note_id))
        {
            self.action = Some(FolderAction::MoveNote {
                note_id,
                destination: note.folder_id.clone(),
            });
            self.error = None;
            cx.notify();
        }
    }

    fn edit_name(
        &mut self,
        action: FolderAction,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.action = Some(action);
        self.error = None;
        self.name
            .update(cx, |input, cx| input.set_value(name, window, cx));
        self.name.focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    fn folder_name(&self, id: &str) -> String {
        self.rows
            .iter()
            .find(|row| row.id == id)
            .map(|row| row.name.clone())
            .unwrap_or_else(|| "目录已移除".into())
    }

    fn destination_name(&self, id: Option<&str>) -> String {
        id.and_then(|id| self.rows.iter().find(|row| row.id == id))
            .map(|row| row.path.clone())
            .unwrap_or_else(|| "未分类 / 根目录".into())
    }

    fn choose_destination(&mut self, destination: Option<String>, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        match &mut self.action {
            Some(FolderAction::MoveFolder {
                destination: target,
                ..
            })
            | Some(FolderAction::MoveNote {
                destination: target,
                ..
            }) => *target = destination,
            _ => return,
        }
        self.error = None;
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let (Some(snapshot), Some(action)) = (&self.snapshot, self.action.clone()) else {
            return;
        };
        let result = (|| -> Result<Command, String> {
            Ok(match action {
                FolderAction::Create { parent_id } => {
                    let name = validated_name(
                        self.name.read(cx).value().as_ref(),
                        &snapshot.folders,
                        parent_id.as_deref(),
                        None,
                    )?;
                    Command::CreateFolder { name, parent_id }
                }
                FolderAction::Rename { folder_id } => {
                    let folder = snapshot
                        .folders
                        .iter()
                        .find(|folder| folder.id == folder_id)
                        .ok_or("目录已移除，请重新选择。")?;
                    let name = validated_name(
                        self.name.read(cx).value().as_ref(),
                        &snapshot.folders,
                        folder.parent_id.as_deref(),
                        Some(&folder_id),
                    )?;
                    Command::RenameFolder { folder_id, name }
                }
                FolderAction::MoveFolder {
                    folder_id,
                    destination,
                } => {
                    if !valid_destination(&self.rows, &folder_id, destination.as_deref()) {
                        return Err("该位置无法作为上级目录，请重新选择。".into());
                    }
                    Command::MoveFolder {
                        folder_id,
                        parent_id: destination,
                    }
                }
                FolderAction::MoveNote {
                    note_id,
                    destination,
                } => {
                    if destination
                        .as_ref()
                        .is_some_and(|id| !self.rows.iter().any(|row| &row.id == id))
                    {
                        return Err("目标目录已移除，请重新选择。".into());
                    }
                    Command::MoveNote {
                        note_id,
                        folder_id: destination,
                    }
                }
                FolderAction::Delete { folder_id } => {
                    if !empty_folder(snapshot, &folder_id) {
                        return Err("此目录仍有笔记或子目录，请先将它们移出。".into());
                    }
                    Command::DeleteFolder { folder_id }
                }
            })
        })();
        match result {
            Ok(command) => {
                self.action = None;
                self.error = None;
                cx.emit(FolderEvent::Command(command));
            }
            Err(message) => self.error = Some(message),
        }
        cx.notify();
    }

    fn action_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(action) = &self.action else {
            return div().into_any_element();
        };
        // The form replaces the tree while open, so even a short sidebar retains
        // one bounded scroll area and its submit/cancel controls remain reachable.
        let mut form = column()
            .id("folder-action-scroll")
            .test_support()
            .gap_2()
            .p_2()
            .rounded_lg()
            .bg(rgb(BG))
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll();
        let mut submit_label = "保存";
        let mut can_submit = !self.busy;
        match action {
            FolderAction::Create { parent_id } => {
                submit_label = "创建目录";
                form = form
                    .child(muted(format!(
                        "新建于：{}",
                        self.destination_name(parent_id.as_deref())
                    )))
                    .child(Input::new(&self.name).small().disabled(self.busy));
                can_submit &= !self.name.read(cx).value().trim().is_empty();
            }
            FolderAction::Rename { folder_id } => {
                form = form
                    .child(muted(format!("重命名：{}", self.folder_name(folder_id))))
                    .child(Input::new(&self.name).small().disabled(self.busy));
                can_submit &= !self.name.read(cx).value().trim().is_empty();
            }
            FolderAction::Delete { folder_id } => {
                submit_label = "确认删除空目录";
                form = form
                    .child(muted(format!("删除「{}」？", self.folder_name(folder_id))))
                    .child(muted("仅移除这个空的逻辑目录。"));
                can_submit &= self
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| empty_folder(snapshot, folder_id));
            }
            FolderAction::MoveFolder {
                folder_id,
                destination,
            }
            | FolderAction::MoveNote {
                note_id: folder_id,
                destination,
            } => {
                submit_label = "移到此处";
                let moving_folder = matches!(action, FolderAction::MoveFolder { .. });
                let subtree_height = moving_folder
                    .then(|| moving_subtree_height(&self.rows, folder_id))
                    .flatten();
                let current = self.snapshot.as_ref().and_then(|snapshot| {
                    if moving_folder {
                        snapshot
                            .folders
                            .iter()
                            .find(|folder| &folder.id == folder_id)
                            .map(|folder| folder.parent_id.as_ref())
                    } else {
                        snapshot
                            .notes
                            .iter()
                            .find(|note| &note.id == folder_id)
                            .map(|note| note.folder_id.as_ref())
                    }
                });
                can_submit &= current.is_some_and(|current| current != destination.as_ref());
                let title = if moving_folder {
                    format!("移动目录：{}", self.folder_name(folder_id))
                } else {
                    self.snapshot
                        .as_ref()
                        .and_then(|snapshot| {
                            snapshot.notes.iter().find(|note| &note.id == folder_id)
                        })
                        .map(|note| format!("移动笔记：{}", note.title))
                        .unwrap_or_else(|| "移动笔记".into())
                };
                let mut destinations = column()
                    .id("folder-destination-scroll")
                    .test_support()
                    .gap_1()
                    .h(px(132.))
                    .flex_shrink_0()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        Button::new("folder-destination-root")
                            .small()
                            .ghost()
                            .w_full()
                            .label("未分类 / 根目录")
                            .selected(destination.is_none())
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.choose_destination(None, cx)),
                            ),
                    );
                for row in self.rows.iter().filter(|row| {
                    !moving_folder
                        || subtree_height
                            .is_some_and(|height| accepts_destination(row, folder_id, height))
                }) {
                    let id = row.id.clone();
                    destinations = destinations.child(
                        Button::new(SharedString::from(format!("folder-destination-{id}")))
                            .small()
                            .ghost()
                            .w_full()
                            .label(row.path.clone())
                            .tooltip(row.path.clone())
                            .selected(destination.as_ref() == Some(&id))
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.choose_destination(Some(id.clone()), cx)
                            })),
                    );
                }
                form = form
                    .child(muted(title).truncate().flex_shrink_0())
                    .child(destinations)
                    .child(muted(if moving_folder {
                        "仅调整逻辑目录层级，源文件位置保持不变。"
                    } else {
                        "只改变笔记分类，源文件位置保持不变。"
                    }));
            }
        }
        form.children(
            self.error
                .as_ref()
                .map(|message| muted(message.clone()).text_color(rgb(0xAA4F34))),
        )
        .child(
            row()
                .gap_1()
                .child(
                    Button::new("folder-submit")
                        .small()
                        .primary()
                        .label(submit_label)
                        .disabled(!can_submit)
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                )
                .child(
                    Button::new("folder-cancel")
                        .small()
                        .ghost()
                        .label("取消")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.action = None;
                            this.error = None;
                            cx.notify();
                        })),
                ),
        )
        .into_any_element()
    }
}

impl Render for FolderPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_id = self.filter.folder_id().map(str::to_owned);
        let can_create = !self.busy
            && self.action.is_none()
            && self.snapshot.is_some()
            && selected_id.as_ref().is_none_or(|id| {
                self.rows
                    .iter()
                    .find(|row| &row.id == id)
                    .is_none_or(|row| row.depth + 1 < MAX_FOLDER_DEPTH)
            });
        let mut panel = column()
            .id("note-folders")
            .gap_2()
            .w_full()
            .h_full()
            .min_h_0()
            .min_w_0()
            .child(
                row()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("笔记目录"),
                    )
                    .child(
                        Button::new("folder-create")
                            .small()
                            .ghost()
                            .label("＋ 新建")
                            .tooltip("在当前选中目录内创建，最多八层")
                            .disabled(!can_create)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let parent_id = this.filter.folder_id().map(str::to_owned);
                                this.edit_name(
                                    FolderAction::Create { parent_id },
                                    String::new(),
                                    window,
                                    cx,
                                );
                            })),
                    ),
            );
        if self.action.is_some() {
            return panel.child(self.action_view(cx));
        }
        let mut tree = column()
            .id("folder-tree-scroll")
            .test_support()
            .gap_1()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                Button::new("folders-all")
                    .small()
                    .ghost()
                    .w_full()
                    .label(format!("全部笔记  {}", self.all_count))
                    .selected(self.filter == FolderFilter::All)
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.set_filter(FolderFilter::All, cx))),
            )
            .child(
                Button::new("folders-root")
                    .small()
                    .ghost()
                    .w_full()
                    .label(format!("未分类  {}", self.root_count))
                    .selected(self.filter == FolderFilter::Root)
                    .disabled(self.busy)
                    .on_click(
                        cx.listener(|this, _, _, cx| this.set_filter(FolderFilter::Root, cx)),
                    ),
            );
        for folder in self.rows.iter().filter(|row| {
            !row.ancestors
                .iter()
                .any(|ancestor| self.collapsed.contains(ancestor))
        }) {
            let id = folder.id.clone();
            let expanded = !self.collapsed.contains(&id);
            let count = self.counts.get(&id).copied().unwrap_or(0);
            let mut entry = row().gap_0().min_w_0().flex_shrink_0().pl(px((folder
                .depth
                .min(MAX_FOLDER_DEPTH)
                * 12)
                as f32));
            if folder.has_children {
                let collapse_id = id.clone();
                entry = entry.child(
                    Button::new(SharedString::from(format!("folder-expand-{id}")))
                        .small()
                        .ghost()
                        .w(px(24.))
                        .flex_shrink_0()
                        .label(if expanded { "▾" } else { "▸" })
                        .disabled(self.busy)
                        .accessibility_label(format!(
                            "{}{}",
                            if expanded { "折叠" } else { "展开" },
                            folder.name
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.collapsed.remove(&collapse_id) {
                                this.collapsed.insert(collapse_id.clone());
                            }
                            cx.notify();
                        })),
                );
            } else {
                entry = entry.child(div().w(px(24.)).flex_shrink_0());
            }
            tree = tree.child(
                entry.child(
                    Button::new(SharedString::from(format!("folder-row-{id}")))
                        .small()
                        .ghost()
                        .flex_1()
                        .min_w_0()
                        .label(format!("{}  {count}", folder.name))
                        .tooltip(format!("{} · 直接包含 {count} 份笔记", folder.path))
                        .selected(selected_id.as_ref() == Some(&id))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_filter(FolderFilter::Folder(id.clone()), cx)
                        })),
                ),
            );
        }
        if self.rows.is_empty() {
            tree = tree.child(muted("新建目录，按主题整理笔记。").text_xs().p_2());
        }
        panel = panel.child(tree);
        if let Some(folder_id) = selected_id {
            let rename_id = folder_id.clone();
            let move_id = folder_id.clone();
            let can_delete = !self.busy
                && self
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| empty_folder(snapshot, &folder_id));
            panel = panel.child(
                row()
                    .gap_1()
                    .child(
                        Button::new("folder-rename")
                            .small()
                            .ghost()
                            .label("重命名")
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let name = this.folder_name(&rename_id);
                                this.edit_name(
                                    FolderAction::Rename {
                                        folder_id: rename_id.clone(),
                                    },
                                    name,
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .child(
                        Button::new("folder-move")
                            .small()
                            .ghost()
                            .label("移动")
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let destination = this
                                    .snapshot
                                    .as_ref()
                                    .and_then(|snapshot| {
                                        snapshot.folders.iter().find(|folder| folder.id == move_id)
                                    })
                                    .and_then(|folder| folder.parent_id.clone());
                                this.action = Some(FolderAction::MoveFolder {
                                    folder_id: move_id.clone(),
                                    destination,
                                });
                                this.error = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("folder-delete")
                            .small()
                            .ghost()
                            .label("删除")
                            .tooltip(if can_delete {
                                "删除这个空目录"
                            } else {
                                "先移出全部笔记和子目录，才能删除"
                            })
                            .disabled(!can_delete)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.action = Some(FolderAction::Delete {
                                    folder_id: folder_id.clone(),
                                });
                                this.error = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        panel
    }
}

fn empty_folder(snapshot: &Snapshot, id: &str) -> bool {
    snapshot.folders.iter().any(|folder| folder.id == id)
        && !snapshot
            .folders
            .iter()
            .any(|folder| folder.parent_id.as_deref() == Some(id))
        && !snapshot
            .notes
            .iter()
            .any(|note| note.folder_id.as_deref() == Some(id))
}

fn validated_name(
    raw: &str,
    folders: &[NoteFolder],
    parent: Option<&str>,
    except: Option<&str>,
) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 200 {
        return Err("请输入 1–200 个字符的目录名称。".into());
    }
    if matches!(name, "." | "..")
        || name
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
    {
        return Err("目录名称不能含路径分隔符或控制字符。".into());
    }
    if folders.iter().any(|folder| {
        Some(folder.id.as_str()) != except
            && folder.parent_id.as_deref() == parent
            && folder.name.to_lowercase() == name.to_lowercase()
    }) {
        return Err("同一上级目录中已有这个名称。".into());
    }
    Ok(name.to_owned())
}

fn folder_rows(folders: &[NoteFolder]) -> Vec<FolderRow> {
    let ids: HashSet<&str> = folders.iter().map(|folder| folder.id.as_str()).collect();
    let mut children: HashMap<Option<&str>, Vec<&NoteFolder>> = HashMap::new();
    for folder in folders {
        let parent = folder.parent_id.as_deref().filter(|id| ids.contains(id));
        children.entry(parent).or_default().push(folder);
    }
    for siblings in children.values_mut() {
        siblings.sort_by_cached_key(|folder| (folder.name.to_lowercase(), folder.id.clone()));
    }
    let mut rows = Vec::with_capacity(folders.len());
    let mut seen = HashSet::new();
    let mut stack: Vec<(&NoteFolder, Vec<String>, String)> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .map(|folder| (*folder, Vec::new(), String::new()))
        .collect();
    // Core rejects cycles. The fallback also keeps a corrupt/stale tree finite
    // and visible so selection and rendering cannot recurse forever.
    let mut remaining = folders.iter();
    loop {
        if stack.is_empty() {
            if let Some(folder) = remaining.find(|folder| !seen.contains(&folder.id)) {
                stack.push((folder, Vec::new(), String::new()));
            } else {
                break;
            }
        }
        let (folder, ancestors, parent_path) = stack.pop().unwrap();
        if !seen.insert(folder.id.clone()) {
            continue;
        }
        let path = if parent_path.is_empty() {
            folder.name.clone()
        } else {
            format!("{parent_path} / {}", folder.name)
        };
        let descendants = children.get(&Some(folder.id.as_str()));
        rows.push(FolderRow {
            id: folder.id.clone(),
            name: folder.name.clone(),
            path: path.clone(),
            depth: ancestors.len(),
            ancestors: ancestors.clone(),
            has_children: descendants.is_some_and(|children| !children.is_empty()),
        });
        if let Some(descendants) = descendants {
            let mut next_ancestors = ancestors;
            next_ancestors.push(folder.id.clone());
            for child in descendants.iter().rev() {
                stack.push((child, next_ancestors.clone(), path.clone()));
            }
        }
    }
    rows
}

fn moving_subtree_height(rows: &[FolderRow], moving: &str) -> Option<usize> {
    let source = rows.iter().find(|row| row.id == moving)?;
    rows.iter()
        .filter(|row| row.id == moving || row.ancestors.iter().any(|id| id == moving))
        .map(|row| row.depth.saturating_sub(source.depth) + 1)
        .max()
}

fn accepts_destination(row: &FolderRow, moving: &str, subtree_height: usize) -> bool {
    row.id != moving
        && !row.ancestors.iter().any(|ancestor| ancestor == moving)
        && row.depth + 1 + subtree_height <= MAX_FOLDER_DEPTH
}

fn valid_destination(rows: &[FolderRow], moving: &str, destination: Option<&str>) -> bool {
    let Some(subtree_height) = moving_subtree_height(rows, moving) else {
        return false;
    };
    match destination {
        None => subtree_height <= MAX_FOLDER_DEPTH,
        Some(id) => rows
            .iter()
            .find(|row| row.id == id)
            .is_some_and(|row| accepts_destination(row, moving, subtree_height)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str, name: &str, parent: Option<&str>) -> NoteFolder {
        NoteFolder {
            id: id.into(),
            name: name.into(),
            parent_id: parent.map(str::to_owned),
        }
    }

    #[::core::prelude::v1::test]
    fn tree_orders_siblings_and_retains_hierarchy() {
        let rows = folder_rows(&[
            folder("b", "Beta", None),
            folder("child", "Child", Some("a")),
            folder("a", "Alpha", None),
        ]);
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["a", "child", "b"]
        );
        assert_eq!(rows[1].path, "Alpha / Child");
        assert_eq!(rows[1].ancestors, ["a"]);
        assert!(rows[0].has_children);
    }

    #[::core::prelude::v1::test]
    fn stale_cycles_and_orphans_cannot_loop_or_hide_rows() {
        let rows = folder_rows(&[
            folder("a", "A", Some("b")),
            folder("b", "B", Some("a")),
            folder("c", "C", Some("missing")),
        ]);
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter().map(|row| &row.id).collect::<HashSet<_>>().len(),
            3
        );
    }

    #[::core::prelude::v1::test]
    fn move_choices_exclude_self_descendants_and_depth_overflow() {
        let folders: Vec<_> = (0..8)
            .map(|level| {
                folder(
                    &level.to_string(),
                    &format!("Level {level}"),
                    (level > 0).then(|| (level - 1).to_string()).as_deref(),
                )
            })
            .chain([folder("other", "Other", None)])
            .collect();
        let rows = folder_rows(&folders);
        assert!(!valid_destination(&rows, "0", Some("0")));
        assert!(!valid_destination(&rows, "0", Some("7")));
        assert!(!valid_destination(&rows, "other", Some("7")));
        assert!(valid_destination(&rows, "other", Some("6")));
        assert!(valid_destination(&rows, "0", None));
    }

    #[::core::prelude::v1::test]
    fn names_are_trimmed_and_unique_only_among_siblings() {
        let folders = [
            folder("a", "Research", None),
            folder("b", "Research", Some("a")),
        ];
        assert!(validated_name(" research ", &folders, None, None).is_err());
        assert_eq!(
            validated_name(" Research ", &folders, None, Some("a")).unwrap(),
            "Research"
        );
        assert!(validated_name("../folder", &folders, None, None).is_err());
        assert!(validated_name("..", &folders, None, None).is_err());
        assert!(validated_name(&"中".repeat(201), &folders, None, None).is_err());
        assert_eq!(
            validated_name("  新目录  ", &folders, None, None).unwrap(),
            "新目录"
        );
    }
}
