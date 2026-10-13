use serde::{Deserialize, Serialize};

pub const TEXT_MAX_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompareMode {
    Quick,
    Content,
}

impl CompareMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Quick => "时间 / 大小",
            Self::Content => "内容",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationKind {
    Local,
    Sftp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryType {
    File,
    Dir,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemStatus {
    Same,
    Different,
    LeftOnly,
    RightOnly,
    Unknown,
}

impl ItemStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Same => "相同",
            Self::Different => "不同",
            Self::LeftOnly => "仅左",
            Self::RightOnly => "仅右",
            Self::Unknown => "",
        }
    }

    pub fn mark(self) -> &'static str {
        match self {
            Self::Same => "=",
            Self::Different => "≠",
            Self::LeftOnly => "←",
            Self::RightOnly => "→",
            Self::Unknown => "·",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Clone, Debug)]
pub struct Location {
    pub kind: LocationKind,
    pub path: String,
    pub connection_id: Option<String>,
}

impl Location {
    pub fn local(path: impl Into<String>) -> Self {
        Self {
            kind: LocationKind::Local,
            path: path.into(),
            connection_id: None,
        }
    }

    pub fn remote(id: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            kind: LocationKind::Sftp,
            path: path.into(),
            connection_id: Some(id.into()),
        }
    }

    pub fn child(&self, name: &str) -> Self {
        let path = match self.kind {
            LocationKind::Local => std::path::Path::new(&self.path)
                .join(name)
                .display()
                .to_string(),
            LocationKind::Sftp => crate::paths::posix_join(&self.path, name),
        };
        Self {
            path,
            ..self.clone()
        }
    }

    pub fn join_rel(&self, rel: &str) -> Self {
        if rel.is_empty() {
            return self.clone();
        }
        let mut loc = self.clone();
        match self.kind {
            LocationKind::Local => {
                let mut path = std::path::PathBuf::from(&self.path);
                for part in rel.split('/').filter(|p| !p.is_empty()) {
                    path.push(part);
                }
                loc.path = path.display().to_string();
            }
            LocationKind::Sftp => {
                let base = if self.path == "/" {
                    String::new()
                } else {
                    self.path.trim_end_matches('/').to_string()
                };
                loc.path = format!("{base}/{rel}");
            }
        }
        loc
    }

    pub fn parent(&self) -> Option<Self> {
        let path = match self.kind {
            LocationKind::Local => crate::paths::parent_local(std::path::Path::new(&self.path))?
                .display()
                .to_string(),
            LocationKind::Sftp => crate::paths::parent_posix(&self.path)?,
        };
        Some(Self {
            path,
            ..self.clone()
        })
    }
}

#[derive(Clone, Debug)]
pub struct FileMeta {
    pub name: String,
    pub path: String,
    pub ty: EntryType,
    pub size: u64,
    pub mtime: i64,
}

#[derive(Clone, Debug)]
pub struct FileProps {
    pub name: String,
    pub path: String,
    pub ty: EntryType,
    pub size: u64,
    pub mtime: i64,
    pub mode: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct CompareNode {
    pub name: String,
    pub rel_path: String,
    pub ty: EntryType,
    pub status: ItemStatus,
    pub left: Option<FileMeta>,
    pub right: Option<FileMeta>,
    pub children: Vec<CompareNode>,
}

#[derive(Clone, Debug, Default)]
pub struct CompareStats {
    pub same: usize,
    pub different: usize,
    pub left_only: usize,
    pub right_only: usize,
    pub files: usize,
    pub dirs: usize,
}

#[derive(Clone, Debug)]
pub struct FolderCompare {
    pub tree: Vec<CompareNode>,
    pub stats: CompareStats,
}

#[derive(Clone, Debug)]
pub struct FileCompare {
    pub left_meta: Option<FileMeta>,
    pub right_meta: Option<FileMeta>,
    pub left_text: Option<String>,
    pub right_text: Option<String>,
    pub left_binary: bool,
    pub right_binary: bool,
    pub left_hash: Option<String>,
    pub right_hash: Option<String>,
    pub identical: bool,
    pub too_large: bool,
    pub left_encoding: String,
    pub right_encoding: String,
    pub left_newline: String,
    pub right_newline: String,
    pub left_trailing_nl: bool,
    pub right_trailing_nl: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpConnectionConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub default_path: String,
    pub created_at: i64,
    pub last_used_at: i64,
}

impl SftpConnectionConfig {
    #[allow(dead_code)]
    pub fn url(&self) -> String {
        crate::url::format(&self.username, &self.host, self.port, &self.default_path)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentSession {
    pub title: String,
    pub left_display: String,
    pub right_display: String,
    pub at: i64,
}

pub fn default_ignore() -> Vec<String> {
    [
        "node_modules",
        ".git",
        ".svn",
        "dist",
        "out",
        ".DS_Store",
        "Thumbs.db",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub fn parse_ignore(text: &str) -> Vec<String> {
    text.split([',', '\n', '，'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("{:x}{:x}", now_ms(), NEXT.fetch_add(1, Ordering::Relaxed))
}
