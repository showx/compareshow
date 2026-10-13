use crate::compare::{compare_file, compare_level};
use crate::diff::{self, DiffRow};
use crate::fsops::{FsOps, RealFs};
use crate::model::*;
use crate::paths::{file_name, normalize_local};
use crate::remote::Dial;
use crate::session::SessionStore;
use crate::sshcfg;
use crate::transfer::{self, CopyItem};
use crate::url;
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;

pub struct Runtime {
    pub fs: RealFs,
    pub store: SessionStore,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            fs: RealFs::new(),
            store: SessionStore::open(),
        }
    }
}

#[derive(Clone)]
pub struct Secret {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub remember: bool,
}

pub enum Job {
    Compare {
        left_text: String,
        right_text: String,
        mode: CompareMode,
        ignore: Vec<String>,
        secret: Option<Secret>,
    },
    Copy {
        items: Vec<CopyItem>,
        left_text: String,
        right_text: String,
        mode: CompareMode,
        ignore: Vec<String>,
        secret: Option<Secret>,
    },
    Diff {
        left: Location,
        right: Location,
        rel: String,
    },
    Save {
        loc: Location,
        text: String,
        encoding: String,
        reload: Option<ReloadDiff>,
    },
    Overwrite {
        src: Location,
        dest: Location,
        reload: ReloadDiff,
    },
    Expand {
        rel: String,
        left: Option<Location>,
        right: Option<Location>,
        mode: CompareMode,
        ignore: Vec<String>,
    },
    Delete {
        locs: Vec<Location>,
        left_text: String,
        right_text: String,
        mode: CompareMode,
        ignore: Vec<String>,
        secret: Option<Secret>,
    },
    Inspect {
        left: Option<Location>,
        right: Option<Location>,
    },
}

#[derive(Clone)]
pub struct ReloadDiff {
    pub left: Location,
    pub right: Location,
    pub rel: String,
}

pub enum JobEvent {
    Progress { scanned: usize, path: String },
    Finished(std::result::Result<JobDone, String>),
}

pub enum JobDone {
    Compared(Compared),
    NeedPassword(NeedPassword),
    Diff(DiffLoaded),
    Noted(String),
    Expanded {
        rel: String,
        nodes: Vec<CompareNode>,
    },
    Inspected {
        left: Option<FileProps>,
        right: Option<FileProps>,
    },
}

pub struct Compared {
    pub left: Option<Location>,
    pub right: Option<Location>,
    pub left_display: String,
    pub right_display: String,
    pub result: FolderCompare,
    pub note: String,
}

pub struct NeedPassword {
    pub username: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub keys_tried: Vec<String>,
}

pub struct DiffLoaded {
    pub rel: String,
    pub left: Location,
    pub right: Location,
    pub file: FileCompare,
    pub rows: Vec<DiffRow>,
    pub changed: usize,
}

pub fn execute(
    rt: &Runtime,
    job: Job,
    tx: &Sender<JobEvent>,
    cancel: &AtomicBool,
) -> std::result::Result<JobDone, String> {
    match job {
        Job::Compare {
            left_text,
            right_text,
            mode,
            ignore,
            secret,
        } => compare_texts(
            rt,
            &left_text,
            &right_text,
            mode,
            &ignore,
            secret.as_ref(),
            tx,
            cancel,
        )
        .map_err(|e| e.to_string()),
        Job::Copy {
            items,
            left_text,
            right_text,
            mode,
            ignore,
            secret,
        } => {
            transfer::copy_items(&rt.fs, &items, cancel).map_err(|e| e.to_string())?;
            compare_texts(
                rt,
                &left_text,
                &right_text,
                mode,
                &ignore,
                secret.as_ref(),
                tx,
                cancel,
            )
            .map_err(|e| e.to_string())
        }
        Job::Diff { left, right, rel } => {
            load_diff(&rt.fs, left, right, rel).map_err(|e| e.to_string())
        }
        Job::Save {
            loc,
            text,
            encoding,
            reload,
        } => {
            let bytes = crate::textutil::encode_text(&text, &encoding);
            rt.fs.write_bytes(&loc, &bytes).map_err(|e| e.to_string())?;
            if let Some(reload) = reload {
                load_diff(&rt.fs, reload.left, reload.right, reload.rel).map_err(|e| e.to_string())
            } else {
                Ok(JobDone::Noted("已保存".into()))
            }
        }
        Job::Overwrite { src, dest, reload } => {
            rt.fs.copy_file(&src, &dest).map_err(|e| e.to_string())?;
            load_diff(&rt.fs, reload.left, reload.right, reload.rel).map_err(|e| e.to_string())
        }
        Job::Expand {
            rel,
            left,
            right,
            mode,
            ignore,
        } => {
            let listed = compare_level(
                &rt.fs,
                left.as_ref(),
                right.as_ref(),
                mode,
                &ignore,
                &mut |_, _| {},
                cancel,
            )
            .map_err(|e| e.to_string())?;
            Ok(JobDone::Expanded {
                rel,
                nodes: listed.tree,
            })
        }
        Job::Delete {
            locs,
            left_text,
            right_text,
            mode,
            ignore,
            secret,
        } => delete_then_compare(
            rt,
            locs,
            &left_text,
            &right_text,
            mode,
            &ignore,
            secret.as_ref(),
            tx,
            cancel,
        ),
        Job::Inspect { left, right } => inspect(&rt.fs, left, right).map_err(|e| e.to_string()),
    }
}

fn delete_then_compare(
    rt: &Runtime,
    locs: Vec<Location>,
    left_text: &str,
    right_text: &str,
    mode: CompareMode,
    ignore: &[String],
    secret: Option<&Secret>,
    tx: &Sender<JobEvent>,
    cancel: &AtomicBool,
) -> std::result::Result<JobDone, String> {
    let total = locs.len();
    let mut failed = 0usize;
    for loc in &locs {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("已取消".into());
        }
        if rt.fs.delete(loc).is_err() {
            failed += 1;
        }
    }
    let mut done = compare_texts(rt, left_text, right_text, mode, ignore, secret, tx, cancel)
        .map_err(|e| e.to_string())?;
    if let JobDone::Compared(compared) = &mut done {
        compared.note = if failed == 0 {
            format!("已删除 {total} 项")
        } else {
            format!("删除失败 {failed} 项")
        };
    }
    Ok(done)
}

fn inspect(fs: &RealFs, left: Option<Location>, right: Option<Location>) -> Result<JobDone> {
    let read = |loc: Option<Location>| -> Option<FileProps> {
        let loc = loc.filter(|item| !item.path.is_empty())?;
        fs.stat(&loc).ok()
    };
    let left = read(left);
    let right = read(right);
    if left.is_none() && right.is_none() {
        anyhow::bail!("读不到文件属性");
    }
    Ok(JobDone::Inspected { left, right })
}

fn report(tx: &Sender<JobEvent>, message: impl Into<String>) {
    let _ = tx.send(JobEvent::Progress {
        scanned: 0,
        path: message.into(),
    });
}

fn opening_message(left: &str, right: &str) -> String {
    if url::looks_like_sftp(left) || url::looks_like_sftp(right) {
        "正在连接…".into()
    } else {
        "正在读取…".into()
    }
}

fn compare_texts(
    rt: &Runtime,
    left_text: &str,
    right_text: &str,
    mode: CompareMode,
    ignore: &[String],
    secret: Option<&Secret>,
    tx: &Sender<JobEvent>,
    cancel: &AtomicBool,
) -> Result<JobDone> {
    rt.store.set_prefs(mode, ignore.to_vec());
    if left_text.trim().is_empty() && right_text.trim().is_empty() {
        anyhow::bail!("先填一个路径");
    }
    report(tx, opening_message(left_text, right_text));
    let left = match resolve_optional(rt, left_text, secret, tx)? {
        None => None,
        Some(Resolved::Need(need)) => return Ok(JobDone::NeedPassword(need)),
        Some(Resolved::Ready(ready)) => Some(ready),
    };
    let right = match resolve_optional(rt, right_text, secret, tx)? {
        None => None,
        Some(Resolved::Need(need)) => return Ok(JobDone::NeedPassword(need)),
        Some(Resolved::Ready(ready)) => Some(ready),
    };
    let mut notes = Vec::new();
    if let Some(note) = left.as_ref().and_then(|side| side.note.clone()) {
        notes.push(note);
    }
    if let Some(note) = right.as_ref().and_then(|side| side.note.clone()) {
        notes.push(note);
    }
    report(tx, "正在读取目录");
    let result = compare_level(
        &rt.fs,
        left.as_ref().map(|side| &side.location),
        right.as_ref().map(|side| &side.location),
        mode,
        ignore,
        &mut |scanned, path| {
            let _ = tx.send(JobEvent::Progress {
                scanned,
                path: path.to_string(),
            });
        },
        cancel,
    )?;
    let left_display = left
        .as_ref()
        .map(|side| side.display.clone())
        .unwrap_or_default();
    let right_display = right
        .as_ref()
        .map(|side| side.display.clone())
        .unwrap_or_default();
    let title = match (left.as_ref(), right.as_ref()) {
        (Some(left), Some(right)) => {
            format!(
                "{} ↔ {}",
                file_name(&left.display),
                file_name(&right.display)
            )
        }
        (Some(side), None) | (None, Some(side)) => file_name(&side.display),
        (None, None) => "未命名".to_string(),
    };
    rt.store.push_recent(RecentSession {
        title,
        left_display: left_display.clone(),
        right_display: right_display.clone(),
        at: now_ms(),
    });
    let mode_note = if left.is_some() && right.is_some() {
        match mode {
            CompareMode::Quick => "已比较当前目录 · 大小与时间",
            CompareMode::Content => "已比较当前目录 · 文件内容",
        }
    } else {
        "已列出当前目录"
    };
    notes.insert(0, mode_note.to_string());
    Ok(JobDone::Compared(Compared {
        left: left.map(|side| side.location),
        right: right.map(|side| side.location),
        left_display,
        right_display,
        result,
        note: notes.join(" · "),
    }))
}

struct ReadySide {
    location: Location,
    display: String,
    note: Option<String>,
}

enum Resolved {
    Ready(ReadySide),
    Need(NeedPassword),
}

fn resolve_optional(
    rt: &Runtime,
    text: &str,
    secret: Option<&Secret>,
    tx: &Sender<JobEvent>,
) -> Result<Option<Resolved>> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    resolve_side(rt, text, secret, tx).map(Some)
}

fn resolve_side(
    rt: &Runtime,
    text: &str,
    secret: Option<&Secret>,
    tx: &Sender<JobEvent>,
) -> Result<Resolved> {
    let text = text.trim();
    if url::looks_like_sftp(text) {
        resolve_sftp(rt, text, secret, tx)
    } else {
        resolve_local(text).map(Resolved::Ready)
    }
}

fn resolve_local(text: &str) -> Result<ReadySide> {
    let path = normalize_local(text);
    let (dir, note) = if path.is_dir() {
        (path, None)
    } else if path.is_file() {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("找不到文件夹：{}", path.display()))?;
        (parent, Some("已打开文件所在文件夹".to_string()))
    } else {
        anyhow::bail!("找不到文件夹：{}", path.display());
    };
    let display = dir.display().to_string();
    Ok(ReadySide {
        location: Location::local(display.clone()),
        display,
        note,
    })
}

fn resolve_sftp(
    rt: &Runtime,
    text: &str,
    secret: Option<&Secret>,
    tx: &Sender<JobEvent>,
) -> Result<Resolved> {
    let parsed = url::parse(text).context("无法解析 SFTP 地址")?;
    let ssh = sshcfg::resolve(&parsed.host, parsed.username.as_deref(), parsed.port);
    report(
        tx,
        format!("正在连接 {}@{}:{}", ssh.username, ssh.host, ssh.port),
    );
    let mut notes = Vec::new();
    let existing = rt
        .store
        .find_by_endpoint(&ssh.username, &ssh.host, ssh.port);
    let mut connection_id = existing
        .as_ref()
        .map(|c| c.id.clone())
        .unwrap_or_else(new_id);
    let user_supplied = parsed.password.is_some()
        || secret.is_some_and(|item| secret_matches(item, &ssh.username, &ssh.host, ssh.port));
    let mut remember = false;
    let mut password = parsed.password.clone();
    if let Some(item) = secret {
        if secret_matches(item, &ssh.username, &ssh.host, ssh.port) {
            password = Some(item.password.clone());
            remember = item.remember;
        }
    }
    if password.is_none() {
        if let Some(saved) = existing.as_ref().and_then(|c| rt.store.get_password(&c.id)) {
            password = Some(saved);
        }
    }
    let outcome = rt.fs.connect(Dial {
        connection_id: connection_id.clone(),
        host: ssh.host.clone(),
        port: ssh.port,
        username: ssh.username.clone(),
        password,
        identity_files: ssh.identity_files,
    })?;
    match outcome {
        crate::remote::ConnectOutcome::NeedPassword { keys_tried } => {
            Ok(Resolved::Need(NeedPassword {
                username: ssh.username,
                host: ssh.host,
                port: ssh.port,
                path: parsed.path,
                keys_tried,
            }))
        }
        crate::remote::ConnectOutcome::Ready {
            connection_id: id,
            note,
        } => {
            connection_id = id;
            if let Some(note) = note {
                notes.push(note);
            }
            let created_at = existing
                .as_ref()
                .map(|c| c.created_at)
                .unwrap_or_else(now_ms);
            rt.store.upsert(SftpConnectionConfig {
                id: connection_id.clone(),
                name: format!("{}@{}", ssh.username, ssh.host),
                host: ssh.host.clone(),
                port: ssh.port,
                username: ssh.username.clone(),
                default_path: parsed.path.clone(),
                created_at,
                last_used_at: now_ms(),
            });
            if user_supplied {
                if let Some(item) =
                    secret.filter(|item| secret_matches(item, &ssh.username, &ssh.host, ssh.port))
                {
                    if remember {
                        if let Err(err) = rt.store.set_password(&connection_id, &item.password) {
                            notes.push(err.to_string());
                        }
                    } else {
                        rt.store.delete_password(&connection_id);
                    }
                } else if parsed.password.is_some() {
                    rt.store.delete_password(&connection_id);
                }
            }
            let display = url::format(&ssh.username, &ssh.host, ssh.port, &parsed.path);
            Ok(Resolved::Ready(ReadySide {
                location: Location::remote(connection_id, parsed.path),
                display,
                note: if notes.is_empty() {
                    None
                } else {
                    Some(notes.join(" · "))
                },
            }))
        }
    }
}

fn secret_matches(secret: &Secret, username: &str, host: &str, port: u16) -> bool {
    secret.username.eq_ignore_ascii_case(username)
        && secret.host.eq_ignore_ascii_case(host)
        && secret.port == port
}

fn load_diff(fs: &RealFs, left: Location, right: Location, rel: String) -> Result<JobDone> {
    let file = compare_file(fs, &left, &right)?;
    let left_text = file.left_text.clone().unwrap_or_default();
    let right_text = file.right_text.clone().unwrap_or_default();
    let aligned = if file.left_text.is_some() && file.right_text.is_some() && !file.too_large {
        diff::align(&left_text, &right_text)
    } else if file.left_text.is_some() && !file.right_binary && !file.too_large {
        diff::align(&left_text, "")
    } else if file.right_text.is_some() && !file.left_binary && !file.too_large {
        diff::align("", &right_text)
    } else {
        diff::Aligned {
            rows: Vec::new(),
            changed: 0,
        }
    };
    Ok(JobDone::Diff(DiffLoaded {
        rel,
        left,
        right,
        changed: aligned.changed,
        rows: aligned.rows,
        file,
    }))
}
