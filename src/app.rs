use crate::compare;
use crate::diff::{self, DiffRow, DiffTag};
use crate::highlight::{self, HlSpan};
use crate::jobs::{self, DiffLoaded, Job, JobDone, JobEvent, Secret};
use crate::model::*;
use crate::paths::parent_of_input;
use crate::textutil::{format_mode, format_size, format_time};
use crate::theme::{self, status_color};
use crate::transfer::CopyItem;
use eframe::egui::{self, Color32, FontId, RichText};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

pub struct CompareApp {
    rt: Arc<jobs::Runtime>,
    left_text: String,
    right_text: String,
    left_loc: Option<Location>,
    right_loc: Option<Location>,
    mode: CompareMode,
    ignore_text: String,
    tree: Vec<CompareNode>,
    stats: CompareStats,
    flat: Vec<FlatRow>,
    expanded: HashSet<String>,
    loaded: HashSet<String>,
    loading: Option<String>,
    pending_dir: Option<String>,
    selected: HashSet<String>,
    anchor: Option<String>,
    status: String,
    status_err: bool,
    busy: bool,
    started: bool,
    filter_open: bool,
    rx: Option<Receiver<JobEvent>>,
    cancel: Option<Arc<AtomicBool>>,
    recents: Vec<RecentSession>,
    password: Option<PasswordDialog>,
    props: Option<PropsView>,
    delete_dialog: Option<DeleteDialog>,
    diff: Option<DiffState>,
    secret: Option<Secret>,
}

#[derive(Clone)]
struct FlatRow {
    rel: String,
    name: String,
    depth: u32,
    open: bool,
    loading: bool,
    status: ItemStatus,
    ty: EntryType,
    left_size: Option<u64>,
    right_size: Option<u64>,
    left_mtime: i64,
    right_mtime: i64,
}

struct PropsView {
    left: Option<FileProps>,
    right: Option<FileProps>,
}

enum DeleteDialog {
    ChooseSide,
    Confirm { locs: Vec<Location>, body: String },
}

struct PasswordDialog {
    username: String,
    host: String,
    port: u16,
    path: String,
    keys_tried: Vec<String>,
    password: String,
    remember: bool,
    focus: bool,
}

struct DiffState {
    rel: String,
    left: Location,
    right: Location,
    file: FileCompare,
    rows: Vec<DiffRow>,
    changed: usize,
    language: String,
    left_hl: Vec<Vec<HlSpan>>,
    right_hl: Vec<Vec<HlSpan>>,
    editing: bool,
    left_edit: String,
    right_edit: String,
    left_dirty: bool,
    right_dirty: bool,
    line_sel: HashSet<usize>,
    line_anchor: Option<usize>,
}

enum PathAction {
    Submit,
    Up(Side),
    Browse(Side),
    Recent(String, String),
}

impl CompareApp {
    pub fn new(rt: Arc<jobs::Runtime>) -> Self {
        let ignore = rt.store.ignore();
        let recents = rt.store.recents();
        let (left_text, right_text, status) = if let Some(last) = recents.first() {
            (
                last.left_display.clone(),
                last.right_display.clone(),
                "已打开上次的位置".into(),
            )
        } else {
            (
                crate::paths::home_dir().display().to_string(),
                String::new(),
                "右侧可以留空。两边都填上之后，同名文件会标出差异。".into(),
            )
        };
        let app = Self {
            recents,
            mode: rt.store.mode(),
            ignore_text: ignore.join(", "),
            rt,
            left_text,
            right_text,
            left_loc: None,
            right_loc: None,
            tree: Vec::new(),
            stats: CompareStats::default(),
            flat: Vec::new(),
            expanded: HashSet::new(),
            loaded: HashSet::new(),
            loading: None,
            pending_dir: None,
            selected: HashSet::new(),
            anchor: None,
            status,
            status_err: false,
            busy: false,
            started: false,
            filter_open: false,
            rx: None,
            cancel: None,
            password: None,
            props: None,
            delete_dialog: None,
            diff: None,
            secret: None,
        };
        std::thread::spawn(highlight::preload);
        app
    }

    fn pump(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else { return };
        let mut finished = None;
        loop {
            match rx.try_recv() {
                Ok(JobEvent::Progress { scanned, path }) => {
                    self.status_err = false;
                    self.status = if path.starts_with("正在") {
                        path
                    } else if self.loading.is_some() {
                        format!("正在展开… {scanned} · {path}")
                    } else if scanned == 0 {
                        format!("正在读取… {path}")
                    } else {
                        format!("正在比较… {scanned} 项 · {path}")
                    };
                    ctx.request_repaint();
                }
                Ok(JobEvent::Finished(result)) => finished = Some(result),
                Err(_) => break,
            }
        }
        if let Some(result) = finished {
            self.rx = None;
            self.cancel = None;
            self.busy = false;
            self.finish(result);
            ctx.request_repaint();
        }
    }

    fn finish(&mut self, result: std::result::Result<JobDone, String>) {
        match result {
            Ok(JobDone::Compared(done)) => {
                let split = done.left.is_some() && done.right.is_some();
                self.left_loc = done.left;
                self.right_loc = done.right;
                self.left_text = done.left_display;
                self.right_text = done.right_display;
                self.tree = done.result.tree;
                self.stats = done.result.stats;
                self.selected.clear();
                self.anchor = None;
                self.expanded.clear();
                self.loaded.clear();
                self.loading = None;
                self.pending_dir = None;
                self.rebuild_flat();
                self.refresh_saved();
                self.secret = None;
                self.status = summary_status(split, self.flat.len(), &self.stats, &done.note);
                self.status_err = false;
                self.diff = None;
            }
            Ok(JobDone::NeedPassword(need)) => {
                self.refresh_saved();
                self.password = Some(PasswordDialog {
                    username: need.username,
                    host: need.host,
                    port: need.port,
                    path: need.path,
                    keys_tried: need.keys_tried,
                    password: String::new(),
                    remember: true,
                    focus: true,
                });
                self.status = "需要密码或私钥口令".into();
                self.status_err = false;
            }
            Ok(JobDone::Diff(loaded)) => {
                self.diff = Some(diff_state(loaded));
                self.status = "单击选中一行，按住 Shift 或拖动选中一段，再点「选中复制到右」。单行也可以点中间的箭头。".into();
                self.status_err = false;
            }
            Ok(JobDone::Noted(message)) => {
                self.status = message;
                self.status_err = false;
            }
            Ok(JobDone::Inspected { left, right }) => {
                self.props = Some(PropsView { left, right });
                self.status = "文件属性".into();
                self.status_err = false;
            }
            Ok(JobDone::Expanded { rel, nodes }) => {
                self.loading = None;
                let count = nodes.len();
                let mut nodes = nodes;
                prefix_rel(&mut nodes, &rel);
                if attach_children(&mut self.tree, &rel, nodes) {
                    self.loaded.insert(rel);
                }
                self.rebuild_flat();
                self.status = if count == 0 {
                    "这个文件夹是空的".into()
                } else {
                    format!("展开了 {count} 项")
                };
                self.status_err = false;
            }
            Err(err) => {
                if let Some(rel) = self.loading.take() {
                    self.expanded.remove(&rel);
                    self.rebuild_flat();
                }
                self.status_err = err != "已取消";
                self.status = err;
            }
        }
    }

    fn refresh_saved(&mut self) {
        self.recents = self.rt.store.recents();
    }

    fn rebuild_flat(&mut self) {
        let mut flat = Vec::new();
        flatten(&self.tree, &self.expanded, &self.loading, 0, &mut flat);
        self.flat = flat;
    }

    fn start_compare(&mut self) {
        if self.busy || self.password.is_some() {
            return;
        }
        let ignore = parse_ignore(&self.ignore_text);
        self.status = opening_status(&self.left_text, &self.right_text);
        self.status_err = false;
        self.tree.clear();
        self.flat.clear();
        self.stats = CompareStats::default();
        self.selected.clear();
        self.anchor = None;
        self.expanded.clear();
        self.loaded.clear();
        self.loading = None;
        self.pending_dir = None;
        self.spawn(Job::Compare {
            left_text: self.left_text.clone(),
            right_text: self.right_text.clone(),
            mode: self.mode,
            ignore,
            secret: self.secret.clone(),
        });
    }

    fn spawn(&mut self, job: Job) {
        if self.busy {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let rt = Arc::clone(&self.rt);
        let flag = Arc::clone(&cancel);
        if std::thread::Builder::new()
            .name("compareshow-job".into())
            .spawn(move || {
                let done = jobs::execute(&rt, job, &tx, &flag);
                let _ = tx.send(JobEvent::Finished(done));
            })
            .is_err()
        {
            self.status = "无法启动后台任务".into();
            self.status_err = true;
            return;
        }
        self.rx = Some(rx);
        self.cancel = Some(cancel);
        self.busy = true;
        self.status_err = false;
        if !self.status.starts_with("正在") {
            self.status = "正在工作…".into();
        }
    }

    fn cancel_job(&mut self) {
        if let Some(flag) = &self.cancel {
            flag.store(true, Ordering::Relaxed);
        }
        self.status = "正在取消…".into();
        self.status_err = false;
    }

    fn apply_path(&mut self, action: PathAction) {
        match action {
            PathAction::Submit => self.start_compare(),
            PathAction::Up(side) => {
                let current = match side {
                    Side::Left => self.left_text.clone(),
                    Side::Right => self.right_text.clone(),
                };
                if let Some(parent) = parent_of_input(&current) {
                    match side {
                        Side::Left => self.left_text = parent,
                        Side::Right => self.right_text = parent,
                    }
                    self.start_compare();
                }
            }
            PathAction::Browse(side) => self.browse(side),
            PathAction::Recent(left, right) => {
                self.left_text = left;
                self.right_text = right;
                self.diff = None;
                self.start_compare();
            }
        }
    }

    fn browse(&mut self, side: Side) {
        let Some(path) = rfd::FileDialog::new().set_title("选择文件夹").pick_folder() else {
            return;
        };
        let text = path.display().to_string();
        match side {
            Side::Left => self.left_text = text,
            Side::Right => self.right_text = text,
        }
        self.start_compare();
    }

    fn copy_selected(&mut self, to_right: bool) {
        let Some(items) = self.selected_items(to_right) else {
            self.status = "另一侧还没有路径".into();
            self.status_err = true;
            return;
        };
        if items.is_empty() {
            self.status = "没有可复制的项目".into();
            self.status_err = true;
            return;
        }
        self.spawn_copy(items);
    }

    fn selected_items(&self, to_right: bool) -> Option<Vec<CopyItem>> {
        let left = self.left_loc.as_ref()?;
        let right = self.right_loc.as_ref()?;
        let mut rels: Vec<String> = self.selected.iter().cloned().collect();
        rels.sort();
        let snapshot = rels.clone();
        rels.retain(|rel| {
            !snapshot
                .iter()
                .any(|other| other != rel && rel.starts_with(&format!("{other}/")))
        });
        let nodes: Vec<CompareNode> = rels
            .iter()
            .filter_map(|rel| compare::find_node(&self.tree, rel).cloned())
            .filter(|node| {
                if to_right {
                    node.left.is_some()
                } else {
                    node.right.is_some()
                }
            })
            .collect();
        Some(copy_items(&nodes, to_right, left, right))
    }

    fn ensure_selected(&mut self, rel: &str) {
        if !self.selected.contains(rel) {
            self.selected.clear();
            self.selected.insert(rel.to_string());
            self.anchor = Some(rel.to_string());
        }
    }

    fn ask_delete_key(&mut self) {
        match (self.left_loc.is_some(), self.right_loc.is_some()) {
            (true, false) => self.ask_delete(Side::Left),
            (false, true) => self.ask_delete(Side::Right),
            (true, true) => self.delete_dialog = Some(DeleteDialog::ChooseSide),
            _ => {}
        }
    }

    fn ask_delete(&mut self, side: Side) {
        if self.busy {
            self.status = "正在读取，稍后再删除".into();
            self.status_err = false;
            return;
        }
        let root = match side {
            Side::Left => self.left_loc.clone(),
            Side::Right => self.right_loc.clone(),
        };
        let Some(root) = root else {
            self.status = "这一侧没有路径".into();
            self.status_err = true;
            return;
        };
        let mut rels: Vec<String> = self.selected.iter().cloned().collect();
        rels.sort();
        let snapshot = rels.clone();
        rels.retain(|rel| {
            !snapshot
                .iter()
                .any(|other| other != rel && rel.starts_with(&format!("{other}/")))
        });
        let mut locs = Vec::new();
        let mut names = Vec::new();
        for rel in rels {
            let Some(node) = compare::find_node(&self.tree, &rel) else {
                continue;
            };
            let meta = match side {
                Side::Left => node.left.as_ref(),
                Side::Right => node.right.as_ref(),
            };
            let Some(meta) = meta else { continue };
            let loc = Location {
                kind: root.kind,
                path: meta.path.clone(),
                connection_id: root.connection_id.clone(),
            };
            if loc.path == root.path || unsafe_delete(&loc.path) {
                continue;
            }
            names.push(node.name.clone());
            locs.push(loc);
        }
        if locs.is_empty() {
            self.status = "没有可删除的项目".into();
            self.status_err = true;
            return;
        }
        let side_name = match side {
            Side::Left => "左侧",
            Side::Right => "右侧",
        };
        let preview = names.iter().take(6).cloned().collect::<Vec<_>>().join("、");
        let extra = if names.len() > 6 {
            format!(" 等 {} 项", names.len())
        } else {
            String::new()
        };
        self.delete_dialog = Some(DeleteDialog::Confirm {
            locs,
            body: format!("将从{side_name}删除 {preview}{extra}。删除后无法恢复。"),
        });
    }

    fn run_delete(&mut self, locs: Vec<Location>) {
        let count = locs.len();
        self.status = format!("正在删除 {count} 项");
        self.spawn(Job::Delete {
            locs,
            left_text: self.left_text.clone(),
            right_text: self.right_text.clone(),
            mode: self.mode,
            ignore: parse_ignore(&self.ignore_text),
            secret: self.secret.clone(),
        });
    }

    fn inspect_rel(&mut self, rel: &str) {
        let Some(node) = compare::find_node(&self.tree, rel) else {
            return;
        };
        let left = located(self.left_loc.as_ref(), node.left.as_ref());
        let right = located(self.right_loc.as_ref(), node.right.as_ref());
        if left.is_none() && right.is_none() {
            self.status = "读不到这一项".into();
            self.status_err = true;
            return;
        }
        self.status = "正在读取属性".into();
        self.spawn(Job::Inspect { left, right });
    }

    fn spawn_copy(&mut self, items: Vec<CopyItem>) {
        self.spawn(Job::Copy {
            items,
            left_text: self.left_text.clone(),
            right_text: self.right_text.clone(),
            mode: self.mode,
            ignore: parse_ignore(&self.ignore_text),
            secret: self.secret.clone(),
        });
    }

    fn open_diff(&mut self, rel: &str) {
        let Some(node) = compare::find_node(&self.tree, rel) else {
            return;
        };
        if node.ty != EntryType::File {
            return;
        }
        let left = file_location(self.left_loc.as_ref(), node.left.as_ref());
        let right = file_location(self.right_loc.as_ref(), node.right.as_ref());
        if left.path.is_empty() && right.path.is_empty() {
            return;
        }
        self.status = "正在读取文件".into();
        self.status_err = false;
        self.spawn(Job::Diff {
            left,
            right,
            rel: rel.to_string(),
        });
    }

    fn toggle_dir(&mut self, rel: &str) {
        if self.expanded.remove(rel) {
            self.rebuild_flat();
            return;
        }
        if self.loaded.contains(rel) {
            self.expanded.insert(rel.to_string());
            self.rebuild_flat();
            return;
        }
        if self.busy {
            self.status = "正在读取，请稍候再展开".into();
            self.status_err = false;
            return;
        }
        let Some(node) = compare::find_node(&self.tree, rel) else {
            return;
        };
        if node.ty != EntryType::Dir {
            return;
        }
        let name = node.name.clone();
        let left = node.left.as_ref().and_then(|meta| {
            self.left_loc.as_ref().map(|root| Location {
                kind: root.kind,
                path: meta.path.clone(),
                connection_id: root.connection_id.clone(),
            })
        });
        let right = node.right.as_ref().and_then(|meta| {
            self.right_loc.as_ref().map(|root| Location {
                kind: root.kind,
                path: meta.path.clone(),
                connection_id: root.connection_id.clone(),
            })
        });
        if left.is_none() && right.is_none() {
            return;
        }
        self.expanded.insert(rel.to_string());
        self.loading = Some(rel.to_string());
        self.rebuild_flat();
        self.status = format!("正在展开 {name}");
        self.spawn(Job::Expand {
            rel: rel.to_string(),
            left,
            right,
            mode: self.mode,
            ignore: parse_ignore(&self.ignore_text),
        });
    }

    fn save_side(&mut self, side: Side) {
        let Some(diff) = &self.diff else { return };
        let (loc, text, encoding) = match side {
            Side::Left => (
                &diff.left,
                diff.left_edit.clone(),
                diff.file.left_encoding.clone(),
            ),
            Side::Right => (
                &diff.right,
                diff.right_edit.clone(),
                diff.file.right_encoding.clone(),
            ),
        };
        if loc.path.is_empty() {
            self.status = "这一侧没有可保存的路径".into();
            self.status_err = true;
            return;
        }
        self.spawn(Job::Save {
            loc: loc.clone(),
            text,
            encoding,
            reload: Some(jobs::ReloadDiff {
                left: diff.left.clone(),
                right: diff.right.clone(),
                rel: diff.rel.clone(),
            }),
        });
    }

    fn overwrite_side(&mut self, to_right: bool) {
        let Some(diff) = &self.diff else { return };
        let (src, dest) = if to_right {
            (diff.left.clone(), diff.right.clone())
        } else {
            (diff.right.clone(), diff.left.clone())
        };
        if src.path.is_empty() || dest.path.is_empty() {
            self.status = "另一侧没有这个文件".into();
            self.status_err = true;
            return;
        }
        self.spawn(Job::Overwrite {
            src,
            dest,
            reload: jobs::ReloadDiff {
                left: diff.left.clone(),
                right: diff.right.clone(),
                rel: diff.rel.clone(),
            },
        });
    }

    fn apply_line_copy(&mut self, indices: &[usize], to_right: bool) {
        let Some(diff) = &mut self.diff else { return };
        let Some((left, right)) = diff::apply_copy_indices(
            &diff.rows,
            indices,
            to_right,
            &diff.file.left_newline,
            &diff.file.right_newline,
            diff.file.left_trailing_nl,
            diff.file.right_trailing_nl,
        ) else {
            return;
        };
        let count = indices.len();
        diff.left_edit = left;
        diff.right_edit = right;
        let aligned = diff::align(&diff.left_edit, &diff.right_edit);
        diff.changed = aligned.changed;
        diff.rows = aligned.rows;
        diff.line_sel.clear();
        diff.line_anchor = None;
        colorize(diff);
        if to_right {
            diff.right_dirty = true;
        } else {
            diff.left_dirty = true;
        }
        self.status = if to_right {
            format!("已把 {count} 行复制到右侧，点「保存右侧」写回文件")
        } else {
            format!("已把 {count} 行复制到左侧，点「保存左侧」写回文件")
        };
        self.status_err = false;
    }
}

impl eframe::App for CompareApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump(ctx);
        if !self.started {
            self.started = true;
            self.start_compare();
        }
        if self.busy {
            ctx.request_repaint_after(Duration::from_millis(80));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        ui.painter().rect_filled(ui.max_rect(), 0.0, theme::BG);
        egui::Frame::NONE.inner_margin(16.0).show(ui, |ui| {
            self.toolbar(ui);
            self.path_bars(ui);
            if self.busy {
                self.loading_bar(ui);
            }
            ui.add_space(2.0);
            if self.diff.is_some() {
                self.diff_view(ui);
            } else {
                self.folder_view(ui);
            }
            self.footer(ui);
        });
        self.overlays(&ctx);
    }
}

impl CompareApp {
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.password.is_some() {
            return;
        }
        let typing = ctx.egui_wants_keyboard_input();
        let (f5, escape, left, right, delete) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::F5),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            )
        });
        if escape && (self.props.is_some() || self.delete_dialog.is_some()) {
            self.props = None;
            self.delete_dialog = None;
            return;
        }
        if self.props.is_some() || self.delete_dialog.is_some() {
            return;
        }
        if escape && self.diff.is_some() {
            self.diff = None;
        }
        if f5 && !typing {
            let reload = self
                .diff
                .as_ref()
                .map(|diff| (diff.left.clone(), diff.right.clone(), diff.rel.clone()));
            if let Some((left, right, rel)) = reload {
                self.spawn(Job::Diff { left, right, rel });
            } else {
                self.start_compare();
            }
        }
        if !typing && self.diff.is_none() && left {
            self.copy_selected(false);
        }
        if !typing && self.diff.is_none() && right {
            self.copy_selected(true);
        }
        if delete && !typing && self.diff.is_none() && !self.selected.is_empty() {
            self.ask_delete_key();
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let quick = ui
                .selectable_label(self.mode == CompareMode::Quick, CompareMode::Quick.label())
                .clicked();
            let content = ui
                .selectable_label(
                    self.mode == CompareMode::Content,
                    CompareMode::Content.label(),
                )
                .clicked();
            if quick && self.mode != CompareMode::Quick {
                self.mode = CompareMode::Quick;
                self.start_compare();
            }
            if content && self.mode != CompareMode::Content {
                self.mode = CompareMode::Content;
                self.start_compare();
            }
            if ui
                .add_enabled(!self.busy, egui::Button::new("刷新"))
                .clicked()
            {
                self.start_compare();
            }
            if self.busy && ui.button("取消").clicked() {
                self.cancel_job();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.recent_box(ui);
                if ui
                    .selectable_label(self.filter_open, "过滤")
                    .on_hover_text("忽略的文件名")
                    .clicked()
                {
                    self.filter_open = !self.filter_open;
                }
            });
        });
        if self.filter_open {
            ui.horizontal(|ui| {
                ui.label(RichText::new("忽略").color(theme::MUTED));
                ui.add(
                    egui::TextEdit::singleline(&mut self.ignore_text)
                        .desired_width(ui.available_width())
                        .hint_text("逗号分隔，例如 node_modules, .git"),
                );
            });
        }
    }

    fn recent_box(&mut self, ui: &mut egui::Ui) {
        let recents = self.recents.clone();
        let mut picked = None;
        egui::ComboBox::from_id_salt("recents")
            .width(220.0)
            .selected_text("历史")
            .show_ui(ui, |ui| {
                if recents.is_empty() {
                    ui.label(RichText::new("还没有打开过的位置").color(theme::MUTED));
                }
                for recent in &recents {
                    let label = history_label(recent);
                    if ui
                        .selectable_label(false, &label)
                        .on_hover_text(history_tip(recent))
                        .clicked()
                    {
                        picked = Some((recent.left_display.clone(), recent.right_display.clone()));
                    }
                }
            });
        if let Some((left, right)) = picked {
            self.apply_path(PathAction::Recent(left, right));
        }
    }

    fn path_bars(&mut self, ui: &mut egui::Ui) {
        let mut action = None;
        let gap = 12.0;
        let col = ((ui.available_width() - gap) / 2.0).max(120.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(
                egui::vec2(col, 40.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    egui::Frame::NONE
                        .fill(theme::RAISE)
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| self.path_row(ui, Side::Left, &mut action));
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(col, 40.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    egui::Frame::NONE
                        .fill(theme::RAISE)
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| self.path_row(ui, Side::Right, &mut action));
                },
            );
        });
        if let Some(action) = action {
            self.apply_path(action);
        }
    }

    fn path_row(&mut self, ui: &mut egui::Ui, side: Side, action: &mut Option<PathAction>) {
        let busy = self.busy;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let accent = match side {
                Side::Left => theme::LEFT,
                Side::Right => theme::RIGHT,
            };
            let (bar, _) = ui.allocate_exact_size(egui::vec2(3.0, 16.0), egui::Sense::hover());
            ui.painter().rect_filled(bar, 2.0, accent);
            let reserve = 92.0;
            let text_w = (ui.available_width() - reserve).max(48.0);
            let resp = {
                let text = match side {
                    Side::Left => &mut self.left_text,
                    Side::Right => &mut self.right_text,
                };
                let hint = match side {
                    Side::Left => "路径，或 sftp://user@host/path",
                    Side::Right => "另一侧，可以留空",
                };
                ui.add(
                    egui::TextEdit::singleline(text)
                        .desired_width(text_w)
                        .hint_text(hint),
                )
            };
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                *action = Some(PathAction::Submit);
            }
            if ui
                .add_enabled(
                    !busy,
                    egui::Button::new("↑").min_size(egui::vec2(32.0, 28.0)),
                )
                .on_hover_text("上级目录")
                .clicked()
            {
                *action = Some(PathAction::Up(side));
            }
            if ui.add_enabled(!busy, egui::Button::new("选择")).clicked() {
                *action = Some(PathAction::Browse(side));
            }
        });
    }

    fn loading_bar(&self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        egui::Frame::NONE
            .fill(theme::ACTIVE)
            .corner_radius(8.0)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    ui.add(egui::Spinner::new().size(16.0).color(theme::ACCENT));
                    ui.label(RichText::new(&self.status).color(theme::TEXT));
                });
            });
    }

    fn folder_view(&mut self, ui: &mut egui::Ui) {
        if let Some(side) = self.single_side() {
            self.up_row(ui, side);
        }
        let len = self.flat.len();
        if len == 0 {
            let height = (ui.available_height() - 40.0).max(120.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.add_space((height * 0.28).min(120.0));
                    if self.busy {
                        ui.add(egui::Spinner::new().size(36.0).color(theme::ACCENT));
                        ui.add_space(12.0);
                        ui.label(RichText::new(&self.status).size(16.0));
                        ui.label(
                            RichText::new("连接和读取可能要几秒，可以点取消。").color(theme::MUTED),
                        );
                    } else {
                        ui.label(RichText::new("这里是空的").color(theme::MUTED).size(16.0));
                        ui.label(RichText::new("换一个路径，或检查过滤规则。").color(theme::FAINT));
                    }
                },
            );
            return;
        }
        let height = (ui.available_height() - 40.0).max(80.0);
        egui::Frame::NONE
            .fill(theme::RAISE)
            .corner_radius(8.0)
            .inner_margin(4.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(height)
                    .show_rows(ui, 36.0, len, |ui, range| {
                        for index in range {
                            if index >= self.flat.len() {
                                break;
                            }
                            self.paint_row(ui, index);
                        }
                    });
            });
        if let Some(rel) = self.pending_dir.take() {
            self.toggle_dir(&rel);
        }
    }

    fn single_side(&self) -> Option<Side> {
        match (self.left_loc.is_some(), self.right_loc.is_some()) {
            (true, false) => Some(Side::Left),
            (false, true) => Some(Side::Right),
            _ => None,
        }
    }

    fn up_row(&mut self, ui: &mut egui::Ui, side: Side) {
        let current = match side {
            Side::Left => self.left_text.clone(),
            Side::Right => self.right_text.clone(),
        };
        if parent_of_input(&current).is_none() {
            return;
        }
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click());
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect.shrink(1.0), 6.0, theme::HOVER);
        }
        paint_clipped(
            ui,
            rect.shrink2(egui::vec2(12.0, 0.0)),
            "↑    上级目录",
            FontId::new(14.0, egui::FontFamily::Proportional),
            theme::MUTED,
        );
        if resp.clicked() {
            self.apply_path(PathAction::Up(side));
        }
    }

    fn paint_row(&mut self, ui: &mut egui::Ui, index: usize) {
        let Some(row) = self.flat.get(index).cloned() else {
            return;
        };
        let split = self.left_loc.is_some() && self.right_loc.is_some();
        let selected = self.selected.contains(&row.rel);
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 36.0), egui::Sense::click());
        let gutter = 28.0;
        let mid = rect.center().x;
        let left_rect =
            egui::Rect::from_min_max(rect.min, egui::pos2(mid - gutter / 2.0, rect.max.y));
        let right_rect =
            egui::Rect::from_min_max(egui::pos2(mid + gutter / 2.0, rect.min.y), rect.max);
        if selected {
            ui.painter()
                .rect_filled(rect.shrink(1.0), 6.0, theme::SELECTED);
        } else if resp.hovered() {
            ui.painter()
                .rect_filled(rect.shrink(1.0), 6.0, theme::HOVER);
        } else if split {
            paint_status_fill(ui, left_rect, right_rect, row.status);
        }
        if split {
            if side_has(&row, Side::Left) {
                paint_entry(ui, left_rect, &row, Side::Left);
            }
            if side_has(&row, Side::Right) {
                paint_entry(ui, right_rect, &row, Side::Right);
            }
            let mark = match row.status {
                ItemStatus::Different | ItemStatus::LeftOnly | ItemStatus::RightOnly => {
                    row.status.mark()
                }
                _ => "",
            };
            if !mark.is_empty() {
                paint_clipped(
                    ui,
                    egui::Rect::from_min_max(
                        egui::pos2(mid - gutter / 2.0, rect.min.y),
                        egui::pos2(mid + gutter / 2.0, rect.max.y),
                    ),
                    mark,
                    FontId::new(13.0, egui::FontFamily::Monospace),
                    status_color(row.status),
                );
            }
        } else {
            let side = if side_has(&row, Side::Left) {
                Side::Left
            } else {
                Side::Right
            };
            paint_entry(ui, rect, &row, side);
        }

        if resp.double_clicked() {
            if row.ty == EntryType::File {
                self.open_diff(&row.rel);
            }
        } else if resp.clicked() {
            let shift = ui.input(|i| i.modifiers.shift);
            let toggle = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
            self.select_index(index, shift, toggle);
            if row.ty == EntryType::Dir {
                self.pending_dir = Some(row.rel.clone());
            }
        }
        let can_copy = split;
        let dir_label = if row.open { "折叠" } else { "展开" };
        resp.context_menu(|ui| {
            if row.ty == EntryType::Dir && ui.button(dir_label).clicked() {
                self.pending_dir = Some(row.rel.clone());
                ui.close();
            }
            if row.ty == EntryType::File && ui.button("看差异").clicked() {
                self.open_diff(&row.rel);
                ui.close();
            }
            if can_copy && side_has(&row, Side::Left) && ui.button("复制到右").clicked() {
                self.selected.clear();
                self.selected.insert(row.rel.clone());
                self.copy_selected(true);
                ui.close();
            }
            if can_copy && side_has(&row, Side::Right) && ui.button("复制到左").clicked() {
                self.selected.clear();
                self.selected.insert(row.rel.clone());
                self.copy_selected(false);
                ui.close();
            }
            if ui.button("属性").clicked() {
                self.ensure_selected(&row.rel);
                self.inspect_rel(&row.rel);
                ui.close();
            }
            if side_has(&row, Side::Left) && ui.button(delete_label(split, Side::Left)).clicked() {
                self.ensure_selected(&row.rel);
                self.ask_delete(Side::Left);
                ui.close();
            }
            if side_has(&row, Side::Right) && ui.button(delete_label(split, Side::Right)).clicked()
            {
                self.ensure_selected(&row.rel);
                self.ask_delete(Side::Right);
                ui.close();
            }
        });
        let action = if row.ty == EntryType::Dir {
            if row.open {
                "点击折叠"
            } else {
                "点击展开"
            }
        } else {
            "双击打开"
        };
        let hover = if split && !row.status.label().is_empty() {
            format!("{}，{action}", row.status.label())
        } else {
            action.to_string()
        };
        resp.on_hover_text(hover);
    }

    fn select_index(&mut self, index: usize, shift: bool, toggle: bool) {
        let Some(rel) = self.flat.get(index).map(|row| row.rel.clone()) else {
            return;
        };
        if shift {
            if let Some(anchor) = &self.anchor {
                if let Some(from) = self.flat.iter().position(|row| &row.rel == anchor) {
                    let (a, b) = if from <= index {
                        (from, index)
                    } else {
                        (index, from)
                    };
                    self.selected = self.flat[a..=b].iter().map(|row| row.rel.clone()).collect();
                    return;
                }
            }
        }
        if toggle {
            if !self.selected.remove(&rel) {
                self.selected.insert(rel.clone());
            }
        } else {
            self.selected.clear();
            self.selected.insert(rel.clone());
        }
        self.anchor = Some(rel);
    }

    fn diff_view(&mut self, ui: &mut egui::Ui) {
        if self.diff.is_none() {
            return;
        }
        let busy = self.busy;
        let mut back = false;
        let mut save_left = false;
        let mut save_right = false;
        let mut overwrite_right = false;
        let mut overwrite_left = false;
        let mut toggle_edit = false;
        let mut line_copy: Option<(Vec<usize>, bool)> = None;
        let mut line_gesture = None;
        let mut pick_note = None;
        {
            let diff = self.diff.as_mut().unwrap();
            let text_ok = diff.file.left_text.is_some() || diff.file.right_text.is_some();
            let can_edit = text_ok
                && !diff.file.too_large
                && !diff.file.left_binary
                && !diff.file.right_binary
                && diff.rows.len() <= 4000;
            let edit_label = if diff.editing {
                "完成编辑"
            } else {
                "编辑"
            };
            let left_label = if diff.left_dirty {
                "保存左侧 *"
            } else {
                "保存左侧"
            };
            let right_label = if diff.right_dirty {
                "保存右侧 *"
            } else {
                "保存右侧"
            };
            let changed = diff.changed;
            let identical = diff.file.identical;
            ui.horizontal_wrapped(|ui| {
                if ui.button("返回列表").clicked() {
                    back = true;
                }
                ui.label(RichText::new(&diff.rel).strong());
                if !diff.language.is_empty() {
                    ui.label(
                        RichText::new(&diff.language)
                            .color(theme::ACCENT)
                            .size(12.0),
                    );
                }
                ui.label(
                    RichText::new(format!("{changed} 处差异")).color(if changed == 0 {
                        theme::OK
                    } else {
                        theme::WARN
                    }),
                );
                if identical {
                    ui.label(RichText::new("内容相同").color(theme::OK));
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("用左侧覆盖右侧"))
                    .clicked()
                {
                    overwrite_right = true;
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("用右侧覆盖左侧"))
                    .on_hover_text("整份文件覆盖")
                    .clicked()
                {
                    overwrite_left = true;
                }
                let picked = diff.line_sel.len();
                let can_pick = picked > 0 && !diff.editing;
                if ui
                    .add_enabled(
                        can_pick && !busy,
                        egui::Button::new(if picked == 0 {
                            "选中复制到右".to_string()
                        } else {
                            format!("选中 {picked} 行复制到右")
                        }),
                    )
                    .on_hover_text("只复制选中的行，不覆盖整份文件")
                    .clicked()
                {
                    line_copy = Some((sorted_lines(&diff.line_sel), true));
                }
                if ui
                    .add_enabled(
                        can_pick && !busy,
                        egui::Button::new(if picked == 0 {
                            "选中复制到左".to_string()
                        } else {
                            format!("选中 {picked} 行复制到左")
                        }),
                    )
                    .on_hover_text("只复制选中的行，不覆盖整份文件")
                    .clicked()
                {
                    line_copy = Some((sorted_lines(&diff.line_sel), false));
                }
                if can_edit && ui.button(edit_label).clicked() {
                    toggle_edit = true;
                }
                if ui
                    .add_enabled(!busy, egui::Button::new(left_label))
                    .clicked()
                {
                    save_left = true;
                }
                if ui
                    .add_enabled(!busy, egui::Button::new(right_label))
                    .clicked()
                {
                    save_right = true;
                }
            });
            let show_hash = diff.file.too_large || diff.file.left_binary || diff.file.right_binary;
            if diff.file.too_large {
                ui.label(
                    RichText::new("文件超过 8MB，只比较哈希，不打开文本。").color(theme::WARN),
                );
            } else if diff.file.left_binary || diff.file.right_binary {
                ui.label(RichText::new("二进制文件，不显示逐行差异。").color(theme::WARN));
            }
            if show_hash {
                if let (Some(left), Some(right)) = (&diff.file.left_hash, &diff.file.right_hash) {
                    let left_size = diff
                        .file
                        .left_meta
                        .as_ref()
                        .map(|m| format_size(m.size))
                        .unwrap_or_default();
                    let right_size = diff
                        .file
                        .right_meta
                        .as_ref()
                        .map(|m| format_size(m.size))
                        .unwrap_or_default();
                    ui.label(
                        RichText::new(format!(
                            "MD5 左 {left} ({left_size})    右 {right} ({right_size})"
                        ))
                        .color(theme::FAINT)
                        .size(12.0),
                    );
                }
            }
            if diff.editing {
                let path = diff.rel.clone();
                let left_edit = &mut diff.left_edit;
                let right_edit = &mut diff.right_edit;
                ui.columns(2, |cols| {
                    cols[0].label(RichText::new("左侧").color(theme::LEFT));
                    let left_h = (cols[0].available_height() - 28.0).max(120.0);
                    code_editor(&mut cols[0], left_edit, 0, &path, left_h);
                    cols[1].label(RichText::new("右侧").color(theme::RIGHT));
                    let right_h = (cols[1].available_height() - 28.0).max(120.0);
                    code_editor(&mut cols[1], right_edit, 1, &path, right_h);
                });
            } else if !diff.rows.is_empty() {
                let height = (ui.available_height() - 28.0).max(80.0);
                let len = diff.rows.len();
                let left_hl = &diff.left_hl;
                let right_hl = &diff.right_hl;
                let rows = &diff.rows;
                let selected = &diff.line_sel;
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(height)
                    .show_rows(ui, 24.0, len, |ui, range| {
                        for index in range {
                            let Some(row) = rows.get(index) else {
                                break;
                            };
                            if let Some(cmd) = paint_diff_row(
                                ui,
                                index,
                                row,
                                rows,
                                left_hl,
                                right_hl,
                                selected.contains(&index),
                                selected.len(),
                            ) {
                                let prefer = matches!(cmd, RowCmd::Drag { start: true, .. });
                                let keep_start =
                                    matches!(line_gesture, Some(RowCmd::Drag { start: true, .. }));
                                if prefer || !keep_start {
                                    line_gesture = Some(cmd);
                                }
                            }
                        }
                    });
            }
            if let Some(cmd) = line_gesture {
                match cmd {
                    RowCmd::Copy { indices, to_right } => line_copy = Some((indices, to_right)),
                    RowCmd::CopySelection { to_right } => {
                        line_copy = Some((sorted_lines(&diff.line_sel), to_right));
                    }
                    RowCmd::Select {
                        index,
                        shift,
                        toggle,
                        keep,
                    } => select_diff_lines(diff, index, shift, toggle, keep),
                    RowCmd::Drag { index, start } => {
                        if start || diff.line_anchor.is_none() {
                            diff.line_anchor = Some(index);
                            diff.line_sel.clear();
                            diff.line_sel.insert(index);
                        } else {
                            select_diff_lines(diff, index, true, false, false);
                        }
                    }
                }
                let picked = diff.line_sel.len();
                if picked > 0 && line_copy.is_none() {
                    pick_note = Some(format!(
                        "已选 {picked} 行。点「选中复制到右」只复制这些行，然后保存。"
                    ));
                }
            }
        }
        if let Some((indices, to_right)) = line_copy {
            self.apply_line_copy(&indices, to_right);
            return;
        }
        if let Some(note) = pick_note {
            self.status = note;
            self.status_err = false;
        }
        if back {
            self.diff = None;
            return;
        }
        if toggle_edit {
            if let Some(diff) = &mut self.diff {
                if diff.editing {
                    let aligned = diff::align(&diff.left_edit, &diff.right_edit);
                    diff.changed = aligned.changed;
                    diff.rows = aligned.rows;
                    diff.left_dirty = true;
                    diff.right_dirty = true;
                    diff.line_sel.clear();
                    diff.line_anchor = None;
                    colorize(diff);
                }
                diff.line_sel.clear();
                diff.line_anchor = None;
                diff.editing = !diff.editing;
            }
        }
        if save_left {
            self.save_side(Side::Left);
        }
        if save_right {
            self.save_side(Side::Right);
        }
        if overwrite_right {
            self.overwrite_side(true);
        }
        if overwrite_left {
            self.overwrite_side(false);
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let color = if self.status_err {
                theme::DANGER
            } else {
                theme::MUTED
            };
            ui.label(RichText::new(&self.status).color(color).size(12.0));
            let split = self.left_loc.is_some() && self.right_loc.is_some() && self.diff.is_none();
            let show_actions = !self.selected.is_empty() && self.diff.is_none();
            if show_actions {
                let busy = self.busy;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if split
                        && ui
                            .add_enabled(
                                !busy,
                                egui::Button::new("复制到右 →").fill(theme::PRIMARY),
                            )
                            .clicked()
                    {
                        self.copy_selected(true);
                    }
                    if split
                        && ui
                            .add_enabled(!busy, egui::Button::new("← 复制到左"))
                            .clicked()
                    {
                        self.copy_selected(false);
                    }
                    if ui.add_enabled(!busy, egui::Button::new("删除")).clicked() {
                        self.ask_delete_key();
                    }
                    if ui.add_enabled(!busy, egui::Button::new("属性")).clicked() {
                        if let Some(rel) = self.selected.iter().next().cloned() {
                            self.inspect_rel(&rel);
                        }
                    }
                });
            }
        });
    }

    fn overlays(&mut self, ctx: &egui::Context) {
        let dim = self.password.is_some() || self.delete_dialog.is_some() || self.props.is_some();
        if dim {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Middle,
                egui::Id::new("dim"),
            ));
            painter.rect_filled(ctx.content_rect(), 0.0, Color32::from_black_alpha(140));
        }
        self.password_window(ctx);
        self.delete_window(ctx);
        self.props_window(ctx);
    }

    fn password_window(&mut self, ctx: &egui::Context) {
        if self.password.is_none() {
            return;
        }
        let mut submit = false;
        let mut cancel = false;
        let mut open = true;
        egui::Window::new("SFTP 登录")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                let dlg = self.password.as_mut().unwrap();
                ui.label(format!("{}@{}:{}", dlg.username, dlg.host, dlg.port));
                ui.label(RichText::new(&dlg.path).color(theme::MUTED));
                if !dlg.keys_tried.is_empty() {
                    ui.label(
                        RichText::new("私钥未能登录，可输入私钥口令或账户密码")
                            .color(theme::MUTED)
                            .size(12.0),
                    );
                }
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut dlg.password)
                        .password(true)
                        .hint_text("密码")
                        .desired_width(280.0),
                );
                if dlg.focus {
                    resp.request_focus();
                    dlg.focus = false;
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submit = true;
                }
                ui.checkbox(&mut dlg.remember, "记住密码到系统钥匙串");
                ui.horizontal(|ui| {
                    if ui.button("连接").clicked() {
                        submit = true;
                    }
                    if ui.button("取消").clicked() {
                        cancel = true;
                    }
                });
            });
        if !open || cancel {
            self.password = None;
            self.status = "已取消登录".into();
            return;
        }
        if submit {
            let Some(dlg) = self.password.take() else {
                return;
            };
            self.secret = Some(Secret {
                host: dlg.host,
                port: dlg.port,
                username: dlg.username,
                password: dlg.password,
                remember: dlg.remember,
            });
            self.start_compare();
        }
    }

    fn delete_window(&mut self, ctx: &egui::Context) {
        if self.delete_dialog.is_none() {
            return;
        }
        let title = match self.delete_dialog {
            Some(DeleteDialog::ChooseSide) => "删除",
            _ => "确认删除",
        };
        let mut open = true;
        let mut action = DeleteChoice::None;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                match self.delete_dialog.as_ref() {
                    Some(DeleteDialog::ChooseSide) => {
                        ui.label("删除哪一侧的文件？");
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("删除左侧").clicked() {
                                action = DeleteChoice::Pick(Side::Left);
                            }
                            if ui.button("删除右侧").clicked() {
                                action = DeleteChoice::Pick(Side::Right);
                            }
                            if ui.button("取消").clicked() {
                                action = DeleteChoice::Cancel;
                            }
                        });
                    }
                    Some(DeleteDialog::Confirm { body, .. }) => {
                        ui.label(body);
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add(egui::Button::new("删除").fill(theme::DANGER))
                                .clicked()
                            {
                                action = DeleteChoice::Confirm;
                            }
                            if ui.button("取消").clicked() {
                                action = DeleteChoice::Cancel;
                            }
                        });
                    }
                    None => {}
                }
            });
        if !open {
            action = DeleteChoice::Cancel;
        }
        match action {
            DeleteChoice::None => {}
            DeleteChoice::Cancel => self.delete_dialog = None,
            DeleteChoice::Pick(side) => {
                self.delete_dialog = None;
                self.ask_delete(side);
            }
            DeleteChoice::Confirm => {
                if let Some(DeleteDialog::Confirm { locs, .. }) = self.delete_dialog.take() {
                    self.run_delete(locs);
                }
            }
        }
    }

    fn props_window(&mut self, ctx: &egui::Context) {
        if self.props.is_none() {
            return;
        }
        let mut open = true;
        let mut close = false;
        egui::Window::new("属性")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                let view = self.props.as_ref().unwrap();
                let name = view
                    .left
                    .as_ref()
                    .or(view.right.as_ref())
                    .map(|item| item.name.clone())
                    .unwrap_or_default();
                ui.label(RichText::new(name).strong().size(16.0));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if let Some(props) = &view.left {
                        prop_block(ui, "左侧", props);
                    }
                    if view.left.is_some() && view.right.is_some() {
                        ui.separator();
                    }
                    if let Some(props) = &view.right {
                        prop_block(ui, "右侧", props);
                    }
                });
                ui.add_space(8.0);
                if ui.button("关闭").clicked() {
                    close = true;
                }
            });
        if !open || close {
            self.props = None;
        }
    }
}

enum DeleteChoice {
    None,
    Cancel,
    Confirm,
    Pick(Side),
}

fn delete_label(split: bool, side: Side) -> &'static str {
    if !split {
        return "删除";
    }
    match side {
        Side::Left => "删除左侧",
        Side::Right => "删除右侧",
    }
}

fn unsafe_delete(path: &str) -> bool {
    let path = path.trim().trim_end_matches(['/', '\\']);
    path.is_empty() || path == "/" || path == "\\" || (path.len() <= 3 && path.ends_with(':'))
}

fn located(root: Option<&Location>, meta: Option<&FileMeta>) -> Option<Location> {
    let root = root?;
    let meta = meta?;
    if meta.path.is_empty() {
        return None;
    }
    Some(Location {
        kind: root.kind,
        path: meta.path.clone(),
        connection_id: root.connection_id.clone(),
    })
}

fn prop_block(ui: &mut egui::Ui, title: &str, props: &FileProps) {
    ui.vertical(|ui| {
        ui.set_min_width(260.0);
        ui.label(RichText::new(title).color(theme::ACCENT));
        prop_line(ui, "路径", &props.path);
        prop_line(
            ui,
            "类型",
            if props.ty == EntryType::Dir {
                "文件夹"
            } else {
                "文件"
            },
        );
        let size = if props.ty == EntryType::Dir {
            "—".to_string()
        } else {
            format_size(props.size)
        };
        prop_line(ui, "大小", &size);
        let time = format_time(props.mtime);
        prop_line(ui, "修改时间", if time.is_empty() { "—" } else { &time });
        let mode = props
            .mode
            .map(format_mode)
            .unwrap_or_else(|| "—".to_string());
        prop_line(ui, "权限", &mode);
    });
}

fn prop_line(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(theme::MUTED).size(12.0));
        ui.label(value);
    });
}

fn opening_status(left: &str, right: &str) -> String {
    if crate::url::looks_like_sftp(left) || crate::url::looks_like_sftp(right) {
        "正在连接…".into()
    } else {
        "正在读取…".into()
    }
}

fn diff_state(loaded: DiffLoaded) -> DiffState {
    let left_edit = loaded.file.left_text.clone().unwrap_or_default();
    let right_edit = loaded.file.right_text.clone().unwrap_or_default();
    let mut diff = DiffState {
        rel: loaded.rel,
        left: loaded.left,
        right: loaded.right,
        rows: loaded.rows,
        changed: loaded.changed,
        file: loaded.file,
        language: String::new(),
        left_hl: Vec::new(),
        right_hl: Vec::new(),
        editing: false,
        left_edit,
        right_edit,
        left_dirty: false,
        right_dirty: false,
        line_sel: HashSet::new(),
        line_anchor: None,
    };
    colorize(&mut diff);
    diff
}

fn colorize(diff: &mut DiffState) {
    let sample = if diff.left_edit.is_empty() {
        diff.right_edit.as_str()
    } else {
        diff.left_edit.as_str()
    };
    diff.language = highlight::language_label(&diff.rel, sample);
    diff.left_hl = highlight::color_lines(&diff.left_edit, &diff.rel);
    diff.right_hl = highlight::color_lines(&diff.right_edit, &diff.rel);
}

fn code_editor(ui: &mut egui::Ui, text: &mut String, slot: usize, path: &str, height: f32) {
    let chars = text
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let width = (chars as f32 * 8.0 + 32.0).max(ui.available_width());
    egui::ScrollArea::both()
        .id_salt(("code-editor", slot))
        .max_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                highlight::layout_galley(ui, slot, buf.as_str(), wrap, path)
            };
            ui.add(
                egui::TextEdit::multiline(text)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .desired_width(width)
                    .layouter(&mut layouter),
            );
        });
}

fn history_label(recent: &RecentSession) -> String {
    let left = short_place(&recent.left_display);
    let right = short_place(&recent.right_display);
    match (left.is_empty(), right.is_empty()) {
        (false, false) => format!("{left}  ↔  {right}"),
        (false, true) => left,
        (true, false) => right,
        _ => recent.title.clone(),
    }
}

fn history_tip(recent: &RecentSession) -> String {
    match (
        recent.left_display.is_empty(),
        recent.right_display.is_empty(),
    ) {
        (false, false) => format!("{}\n{}", recent.left_display, recent.right_display),
        (false, true) => recent.left_display.clone(),
        (true, false) => recent.right_display.clone(),
        _ => recent.title.clone(),
    }
}

fn short_place(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return String::new();
    }
    let mut parts: Vec<&str> = trimmed
        .rsplit(['/', '\\'])
        .filter(|part| !part.is_empty())
        .take(2)
        .collect();
    parts.reverse();
    if parts.is_empty() {
        trimmed.to_string()
    } else {
        parts.join("/")
    }
}

fn summary_status(split: bool, rows: usize, stats: &CompareStats, note: &str) -> String {
    let summary = if split {
        format!(
            "不同 {}   只在左 {}   只在右 {}",
            stats.different, stats.left_only, stats.right_only
        )
    } else {
        format!("{rows} 项")
    };
    let extra: Vec<&str> = note
        .split(" · ")
        .filter(|part| {
            !part.starts_with("已比较") && !part.starts_with("已列出") && !part.is_empty()
        })
        .collect();
    if extra.is_empty() {
        summary
    } else {
        format!("{summary}   {}", extra.join("   "))
    }
}

fn side_has(row: &FlatRow, side: Side) -> bool {
    match side {
        Side::Left => row.status != ItemStatus::RightOnly,
        Side::Right => row.status != ItemStatus::LeftOnly,
    }
}

fn paint_status_fill(ui: &egui::Ui, left: egui::Rect, right: egui::Rect, status: ItemStatus) {
    let (left_bg, right_bg) = match status {
        ItemStatus::Different => (theme::DIFF_BG, theme::DIFF_BG),
        ItemStatus::LeftOnly => (theme::LEFT_BG, Color32::TRANSPARENT),
        ItemStatus::RightOnly => (Color32::TRANSPARENT, theme::RIGHT_BG),
        _ => (Color32::TRANSPARENT, Color32::TRANSPARENT),
    };
    if left_bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(left.shrink(1.0), 6.0, left_bg);
    }
    if right_bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(right.shrink(1.0), 6.0, right_bg);
    }
}

fn paint_entry(ui: &egui::Ui, rect: egui::Rect, row: &FlatRow, side: Side) {
    let (size, time) = match side {
        Side::Left => (row.left_size, row.left_mtime),
        Side::Right => (row.right_size, row.right_mtime),
    };
    let arrow = if row.ty != EntryType::Dir {
        "   "
    } else if row.loading {
        "… "
    } else if row.open {
        "▾ "
    } else {
        "▸ "
    };
    let name = format!("{arrow}{}", row.name);
    let edge = 16.0;
    let time_w = if time > 0 && rect.width() > 420.0 {
        132.0
    } else {
        0.0
    };
    let size_w = if size.is_some() { 72.0 } else { 0.0 };
    let text_right = rect.max.x - edge - size_w - time_w;
    paint_clipped(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(rect.min.x + 8.0 + row.depth as f32 * 16.0, rect.min.y),
            egui::pos2(text_right, rect.max.y),
        ),
        &name,
        FontId::new(14.0, egui::FontFamily::Proportional),
        theme::TEXT,
    );
    let size_color = if row.status == ItemStatus::Different {
        theme::WARN
    } else {
        theme::MUTED
    };
    if let Some(size) = size {
        paint_clipped(
            ui,
            egui::Rect::from_min_max(
                egui::pos2(text_right, rect.min.y),
                egui::pos2(text_right + size_w - 6.0, rect.max.y),
            ),
            &format_size(size),
            FontId::new(12.0, egui::FontFamily::Monospace),
            size_color,
        );
    }
    if time_w > 0.0 {
        paint_clipped(
            ui,
            egui::Rect::from_min_max(
                egui::pos2(rect.max.x - edge - time_w, rect.min.y),
                egui::pos2(rect.max.x - edge, rect.max.y),
            ),
            &format_time(time),
            FontId::new(12.0, egui::FontFamily::Monospace),
            theme::FAINT,
        );
    }
}

fn sorted_lines(selected: &HashSet<usize>) -> Vec<usize> {
    let mut lines: Vec<usize> = selected.iter().copied().collect();
    lines.sort_unstable();
    lines
}

fn select_diff_lines(diff: &mut DiffState, index: usize, shift: bool, toggle: bool, keep: bool) {
    if keep && diff.line_sel.contains(&index) {
        return;
    }
    if shift {
        let anchor = diff.line_anchor.unwrap_or(index);
        let (start, end) = if anchor <= index {
            (anchor, index)
        } else {
            (index, anchor)
        };
        diff.line_sel = (start..=end).collect();
        diff.line_anchor = Some(anchor);
        return;
    }
    if toggle {
        if !diff.line_sel.remove(&index) {
            diff.line_sel.insert(index);
        }
        diff.line_anchor = Some(index);
        return;
    }
    diff.line_sel.clear();
    diff.line_sel.insert(index);
    diff.line_anchor = Some(index);
}

enum RowCmd {
    Copy {
        indices: Vec<usize>,
        to_right: bool,
    },
    CopySelection {
        to_right: bool,
    },
    Select {
        index: usize,
        shift: bool,
        toggle: bool,
        keep: bool,
    },
    Drag {
        index: usize,
        start: bool,
    },
}

fn paint_diff_row(
    ui: &mut egui::Ui,
    index: usize,
    row: &DiffRow,
    rows: &[DiffRow],
    left_hl: &[Vec<HlSpan>],
    right_hl: &[Vec<HlSpan>],
    picked: bool,
    selected_len: usize,
) -> Option<RowCmd> {
    let width = ui.available_width();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::click_and_drag());
    let gutter_w = 44.0;
    let mid = rect.center().x;
    let left_end = mid - gutter_w / 2.0;
    let right_start = mid + gutter_w / 2.0;
    let (left_bg, right_bg) = match row.tag {
        DiffTag::Same => (Color32::TRANSPARENT, Color32::TRANSPARENT),
        DiffTag::Changed => (theme::DIFF_BG, theme::DIFF_BG),
        DiffTag::LeftOnly => (theme::DELETE_BG, Color32::TRANSPARENT),
        DiffTag::RightOnly => (Color32::TRANSPARENT, theme::INSERT_BG),
    };
    if left_bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(rect.min, egui::pos2(left_end, rect.max.y)),
            0.0,
            left_bg,
        );
    }
    if right_bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(egui::pos2(right_start, rect.min.y), rect.max),
            0.0,
            right_bg,
        );
    }
    if picked {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + 3.0, rect.max.y)),
            0.0,
            theme::ACCENT,
        );
    }
    let font = FontId::new(13.0, egui::FontFamily::Monospace);
    let gutter = FontId::new(12.0, egui::FontFamily::Monospace);
    paint_half(
        ui,
        rect.min.x,
        left_end,
        rect,
        row.left_no,
        row.left.as_deref().unwrap_or(""),
        line_spans(left_hl, row.left_no),
        font.clone(),
        gutter.clone(),
    );
    paint_half(
        ui,
        right_start,
        rect.max.x,
        rect,
        row.right_no,
        row.right.as_deref().unwrap_or(""),
        line_spans(right_hl, row.right_no),
        font,
        gutter,
    );
    let arrow = copy_arrows(ui, index, row, left_end, right_start, rect);
    let mut cmd = arrow.map(|(to_right, _)| {
        if picked && selected_len > 1 {
            RowCmd::CopySelection { to_right }
        } else {
            RowCmd::Copy {
                indices: vec![index],
                to_right,
            }
        }
    });
    resp.context_menu(|ui| {
        if row.left.is_some() && ui.button("复制这一行到右侧").clicked() {
            cmd = Some(RowCmd::Copy {
                indices: vec![index],
                to_right: true,
            });
            ui.close();
        }
        if row.right.is_some() && ui.button("复制这一行到左侧").clicked() {
            cmd = Some(RowCmd::Copy {
                indices: vec![index],
                to_right: false,
            });
            ui.close();
        }
        if picked && selected_len > 1 && ui.button("把选中复制到右侧").clicked() {
            cmd = Some(RowCmd::CopySelection { to_right: true });
            ui.close();
        }
        if picked && selected_len > 1 && ui.button("把选中复制到左侧").clicked() {
            cmd = Some(RowCmd::CopySelection { to_right: false });
            ui.close();
        }
        if row.tag != DiffTag::Same && ui.button("复制这一段到右侧").clicked() {
            cmd = Some(RowCmd::Copy {
                indices: diff::hunk_indices(rows, index),
                to_right: true,
            });
            ui.close();
        }
        if row.tag != DiffTag::Same && ui.button("复制这一段到左侧").clicked() {
            cmd = Some(RowCmd::Copy {
                indices: diff::hunk_indices(rows, index),
                to_right: false,
            });
            ui.close();
        }
    });
    if cmd.is_some() {
        return cmd;
    }
    let shift = ui.input(|i| i.modifiers.shift);
    let toggle = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
    if resp.drag_started() {
        return Some(RowCmd::Drag { index, start: true });
    }
    let dragging = ui.input(|i| i.pointer.primary_down() && i.pointer.is_decidedly_dragging());
    let over = ui
        .input(|i| i.pointer.interact_pos())
        .is_some_and(|pos| rect.contains(pos));
    if dragging && over {
        return Some(RowCmd::Drag {
            index,
            start: false,
        });
    }
    if resp.clicked() {
        return Some(RowCmd::Select {
            index,
            shift,
            toggle,
            keep: false,
        });
    }
    if resp.secondary_clicked() {
        return Some(RowCmd::Select {
            index,
            shift: false,
            toggle: false,
            keep: true,
        });
    }
    None
}

fn copy_arrows(
    ui: &mut egui::Ui,
    index: usize,
    row: &DiffRow,
    left_end: f32,
    right_start: f32,
    rect: egui::Rect,
) -> Option<(bool, bool)> {
    if row.tag == DiffTag::Same {
        return None;
    }
    let to_right = row.left.is_some();
    let to_left = row.right.is_some();
    let mid = (left_end + right_start) / 2.0;
    let mut cmd = None;
    if to_right {
        let area = if to_left {
            egui::Rect::from_min_max(
                egui::pos2(left_end, rect.min.y),
                egui::pos2(mid, rect.max.y),
            )
        } else {
            egui::Rect::from_min_max(
                egui::pos2(left_end, rect.min.y),
                egui::pos2(right_start, rect.max.y),
            )
        };
        if arrow_button(
            ui,
            area,
            ("diff-to-right", index),
            "→",
            "把这一行复制到右侧",
        ) {
            cmd = Some((true, false));
        }
    }
    if to_left {
        let area = if to_right {
            egui::Rect::from_min_max(
                egui::pos2(mid, rect.min.y),
                egui::pos2(right_start, rect.max.y),
            )
        } else {
            egui::Rect::from_min_max(
                egui::pos2(left_end, rect.min.y),
                egui::pos2(right_start, rect.max.y),
            )
        };
        if arrow_button(ui, area, ("diff-to-left", index), "←", "把这一行复制到左侧") {
            cmd = Some((false, false));
        }
    }
    cmd
}

fn arrow_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    salt: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    tip: &str,
) -> bool {
    let resp = ui.interact(rect, ui.id().with(salt), egui::Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect.shrink(2.0), 4.0, theme::HOVER);
    }
    let font = FontId::new(16.0, egui::FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, theme::ACCENT);
    let pos = egui::pos2(
        rect.center().x - galley.size().x / 2.0,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter().galley(pos, galley, theme::ACCENT);
    resp.on_hover_text(tip).clicked()
}

fn line_spans(lines: &[Vec<HlSpan>], no: Option<usize>) -> &[HlSpan] {
    no.and_then(|n| n.checked_sub(1))
        .and_then(|index| lines.get(index))
        .map(|spans| spans.as_slice())
        .unwrap_or(&[])
}

fn paint_half(
    ui: &mut egui::Ui,
    x0: f32,
    x1: f32,
    rect: egui::Rect,
    no: Option<usize>,
    text: &str,
    spans: &[HlSpan],
    font: FontId,
    gutter: FontId,
) {
    let num = no
        .map(|n| format!("{n:>4}"))
        .unwrap_or_else(|| "    ".into());
    paint_clipped(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(x0, rect.min.y),
            egui::pos2(x0 + 42.0, rect.max.y),
        ),
        &num,
        gutter,
        theme::FAINT,
    );
    paint_code(
        ui,
        egui::Rect::from_min_max(
            egui::pos2(x0 + 46.0, rect.min.y),
            egui::pos2(x1, rect.max.y),
        ),
        text,
        spans,
        font,
    );
}

fn paint_code(ui: &mut egui::Ui, rect: egui::Rect, text: &str, spans: &[HlSpan], font: FontId) {
    if rect.width() < 4.0 || text.is_empty() {
        return;
    }
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    highlight::append_spans(&mut job, text, spans, font);
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let painter = ui.painter().with_clip_rect(rect);
    let y = rect.center().y - galley.size().y * 0.5;
    painter.galley(egui::pos2(rect.min.x, y), galley, theme::TEXT);
}

fn paint_clipped(ui: &egui::Ui, rect: egui::Rect, text: &str, font: FontId, color: Color32) {
    if rect.width() < 4.0 || text.is_empty() {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    let galley = painter.layout_no_wrap(text.to_owned(), font, color);
    let y = rect.center().y - galley.size().y * 0.5;
    painter.galley(egui::pos2(rect.min.x, y), galley, color);
}

fn flatten(
    nodes: &[CompareNode],
    expanded: &HashSet<String>,
    loading: &Option<String>,
    depth: u32,
    out: &mut Vec<FlatRow>,
) {
    for node in nodes {
        let open = node.ty == EntryType::Dir && expanded.contains(&node.rel_path);
        out.push(FlatRow {
            rel: node.rel_path.clone(),
            name: node.name.clone(),
            depth,
            open,
            loading: loading.as_deref() == Some(node.rel_path.as_str()),
            status: node.status,
            ty: node.ty,
            left_size: node
                .left
                .as_ref()
                .filter(|m| m.ty == EntryType::File)
                .map(|m| m.size),
            right_size: node
                .right
                .as_ref()
                .filter(|m| m.ty == EntryType::File)
                .map(|m| m.size),
            left_mtime: node.left.as_ref().map(|m| m.mtime).unwrap_or(0),
            right_mtime: node.right.as_ref().map(|m| m.mtime).unwrap_or(0),
        });
        if open {
            flatten(&node.children, expanded, loading, depth + 1, out);
        }
    }
}

fn prefix_rel(nodes: &mut [CompareNode], parent: &str) {
    for node in nodes {
        node.rel_path = format!("{parent}/{}", node.name);
    }
}

fn attach_children(nodes: &mut [CompareNode], rel: &str, children: Vec<CompareNode>) -> bool {
    if let Some(node) = nodes.iter_mut().find(|node| node.rel_path == rel) {
        node.children = children;
        refresh_dir_status(node);
        return true;
    }
    let Some(node) = nodes
        .iter_mut()
        .find(|node| rel.starts_with(&format!("{}/", node.rel_path)))
    else {
        return false;
    };
    if attach_children(&mut node.children, rel, children) {
        refresh_dir_status(node);
        true
    } else {
        false
    }
}

fn refresh_dir_status(node: &mut CompareNode) {
    if node.children.is_empty() {
        return;
    }
    let mut left_only = false;
    let mut right_only = false;
    let mut different = false;
    let mut same = false;
    for child in &node.children {
        match child.status {
            ItemStatus::Different => different = true,
            ItemStatus::LeftOnly => left_only = true,
            ItemStatus::RightOnly => right_only = true,
            ItemStatus::Same => same = true,
            ItemStatus::Unknown => {}
        }
    }
    node.status = if different || (left_only && right_only) || ((left_only || right_only) && same) {
        ItemStatus::Different
    } else if left_only {
        ItemStatus::LeftOnly
    } else if right_only {
        ItemStatus::RightOnly
    } else if same {
        ItemStatus::Same
    } else {
        node.status
    };
}

fn copy_items(
    nodes: &[CompareNode],
    to_right: bool,
    left: &Location,
    right: &Location,
) -> Vec<CopyItem> {
    nodes
        .iter()
        .filter_map(|node| {
            let meta = if to_right {
                node.left.as_ref()
            } else {
                node.right.as_ref()
            }?;
            let source_root = if to_right { left } else { right };
            let dest_root = if to_right { right } else { left };
            let parent = node.rel_path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
            Some(CopyItem {
                source: Location {
                    kind: source_root.kind,
                    path: meta.path.clone(),
                    connection_id: source_root.connection_id.clone(),
                },
                dest_dir: dest_root.join_rel(parent),
                is_dir: node.ty == EntryType::Dir,
            })
        })
        .collect()
}

fn file_location(root: Option<&Location>, meta: Option<&FileMeta>) -> Location {
    match (root, meta) {
        (Some(root), Some(meta)) => Location {
            kind: root.kind,
            path: meta.path.clone(),
            connection_id: root.connection_id.clone(),
        },
        _ => Location::local(String::new()),
    }
}
