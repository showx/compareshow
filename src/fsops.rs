use crate::model::{EntryType, FileMeta, FileProps, Location, LocationKind};
use crate::paths::normalize_local;
use crate::remote::{ConnectOutcome, Dial, Pool};
use crate::textutil::hash_reader;
use anyhow::{anyhow, Result};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

pub enum ReadOutcome {
    Missing,
    TooLarge { size: u64, meta: FileMeta },
    Data { bytes: Vec<u8>, meta: FileMeta },
}

pub trait FsOps: Send + Sync {
    fn list(&self, loc: &Location) -> Result<Vec<FileMeta>>;
    fn hash_file(&self, loc: &Location) -> Result<String>;
    fn read_limited(&self, loc: &Location, max: u64) -> Result<ReadOutcome>;
    fn write_bytes(&self, loc: &Location, data: &[u8]) -> Result<()>;
    fn copy_file(&self, src: &Location, dest: &Location) -> Result<()>;
    fn mkdir_p(&self, loc: &Location) -> Result<()>;
    fn delete(&self, loc: &Location) -> Result<()>;
    fn stat(&self, loc: &Location) -> Result<FileProps>;
}

pub struct RealFs {
    pool: Pool,
}

impl RealFs {
    pub fn new() -> Self {
        Self { pool: Pool::new() }
    }

    pub fn connect(&self, dial: Dial) -> Result<ConnectOutcome> {
        self.pool.connect(dial)
    }
}

impl Default for RealFs {
    fn default() -> Self {
        Self::new()
    }
}

impl FsOps for RealFs {
    fn list(&self, loc: &Location) -> Result<Vec<FileMeta>> {
        match loc.kind {
            LocationKind::Local => list_local(Path::new(&loc.path)),
            LocationKind::Sftp => {
                let id = need_id(loc)?;
                self.pool.list(id, &loc.path)
            }
        }
    }

    fn hash_file(&self, loc: &Location) -> Result<String> {
        match loc.kind {
            LocationKind::Local => {
                let file = File::open(&loc.path)?;
                hash_reader(file).map_err(|err| friendly(&loc.path, err))
            }
            LocationKind::Sftp => self.pool.hash_file(need_id(loc)?, &loc.path),
        }
    }

    fn read_limited(&self, loc: &Location, max: u64) -> Result<ReadOutcome> {
        match loc.kind {
            LocationKind::Local => read_local(Path::new(&loc.path), max),
            LocationKind::Sftp => {
                let id = need_id(loc)?;
                match self.pool.read_limited(id, &loc.path, max)? {
                    None => Ok(ReadOutcome::Missing),
                    Some((bytes, size, mtime)) if bytes.is_empty() && size > max => {
                        Ok(ReadOutcome::TooLarge {
                            size,
                            meta: remote_meta(&loc.path, size, mtime, false),
                        })
                    }
                    Some((bytes, size, mtime)) => Ok(ReadOutcome::Data {
                        meta: remote_meta(&loc.path, size.max(bytes.len() as u64), mtime, false),
                        bytes,
                    }),
                }
            }
        }
    }

    fn write_bytes(&self, loc: &Location, data: &[u8]) -> Result<()> {
        match loc.kind {
            LocationKind::Local => {
                let path = Path::new(&loc.path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, data).map_err(|err| friendly(&loc.path, err))
            }
            LocationKind::Sftp => self.pool.write_bytes(need_id(loc)?, &loc.path, data),
        }
    }

    fn copy_file(&self, src: &Location, dest: &Location) -> Result<()> {
        match (src.kind, dest.kind) {
            (LocationKind::Local, LocationKind::Local) => {
                copy_local(Path::new(&src.path), Path::new(&dest.path))
            }
            (LocationKind::Local, LocationKind::Sftp) => {
                self.pool
                    .upload(need_id(dest)?, Path::new(&src.path), &dest.path)
            }
            (LocationKind::Sftp, LocationKind::Local) => {
                self.pool
                    .download(need_id(src)?, &src.path, Path::new(&dest.path))
            }
            (LocationKind::Sftp, LocationKind::Sftp) => {
                let src_id = need_id(src)?;
                let dest_id = need_id(dest)?;
                if src_id == dest_id {
                    self.pool.copy_same(src_id, &src.path, &dest.path)
                } else {
                    let tmp = tempfile::NamedTempFile::new()?;
                    self.pool.download(src_id, &src.path, tmp.path())?;
                    self.pool.upload(dest_id, tmp.path(), &dest.path)
                }
            }
        }
    }

    fn mkdir_p(&self, loc: &Location) -> Result<()> {
        match loc.kind {
            LocationKind::Local => fs::create_dir_all(normalize_keep(&loc.path))
                .map_err(|err| friendly(&loc.path, err)),
            LocationKind::Sftp => self.pool.mkdir_p(need_id(loc)?, &loc.path),
        }
    }

    fn delete(&self, loc: &Location) -> Result<()> {
        if refuse_delete(loc) {
            anyhow::bail!("不能删除这个位置");
        }
        match loc.kind {
            LocationKind::Local => delete_local(Path::new(&loc.path)),
            LocationKind::Sftp => self.pool.delete_path(need_id(loc)?, &loc.path),
        }
    }

    fn stat(&self, loc: &Location) -> Result<FileProps> {
        match loc.kind {
            LocationKind::Local => stat_local(Path::new(&loc.path)),
            LocationKind::Sftp => self.pool.stat(need_id(loc)?, &loc.path),
        }
    }
}

fn need_id(loc: &Location) -> Result<&str> {
    loc.connection_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow!("缺少 SFTP 连接"))
}

fn list_local(dir: &Path) -> Result<Vec<FileMeta>> {
    let entries = fs::read_dir(dir).map_err(|err| friendly(&dir.display().to_string(), err))?;
    let mut items = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        let path = entry.path();
        let meta = fs::metadata(&path).or_else(|_| fs::symlink_metadata(&path));
        let Ok(meta) = meta else { continue };
        let is_dir = meta.is_dir();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        items.push(FileMeta {
            name,
            path: path.display().to_string(),
            ty: if is_dir {
                EntryType::Dir
            } else {
                EntryType::File
            },
            size: if is_dir { 0 } else { meta.len() },
            mtime,
        });
    }
    items.sort_by(|a, b| {
        let ar = if a.ty == EntryType::Dir { 0 } else { 1 };
        let br = if b.ty == EntryType::Dir { 0 } else { 1 };
        ar.cmp(&br)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(items)
}

fn read_local(path: &Path, max: u64) -> Result<ReadOutcome> {
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(ReadOutcome::Missing),
        Err(err) => return Err(friendly(&path.display().to_string(), err)),
    };
    if meta.is_dir() {
        anyhow::bail!("不是文件：{}", path.display());
    }
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let file_meta = FileMeta {
        name: path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.display().to_string(),
        ty: EntryType::File,
        size: meta.len(),
        mtime,
    };
    if meta.len() > max {
        return Ok(ReadOutcome::TooLarge {
            size: meta.len(),
            meta: file_meta,
        });
    }
    let file = File::open(path).map_err(|err| friendly(&path.display().to_string(), err))?;
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Ok(ReadOutcome::TooLarge {
            size: bytes.len() as u64,
            meta: file_meta,
        });
    }
    Ok(ReadOutcome::Data {
        bytes,
        meta: file_meta,
    })
}

fn copy_local(src: &Path, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let modified = fs::metadata(src).and_then(|m| m.modified()).ok();
    fs::copy(src, dest).map_err(|err| friendly(&dest.display().to_string(), err))?;
    if let Some(modified) = modified {
        let _ = File::open(dest).and_then(|f| f.set_modified(modified));
    }
    Ok(())
}

fn remote_meta(path: &str, size: u64, mtime: i64, is_dir: bool) -> FileMeta {
    FileMeta {
        name: crate::paths::file_name(path),
        path: path.to_string(),
        ty: if is_dir {
            EntryType::Dir
        } else {
            EntryType::File
        },
        size,
        mtime,
    }
}

fn normalize_keep(path: &str) -> std::path::PathBuf {
    let normalized = normalize_local(path);
    if normalized.as_os_str().is_empty() {
        std::path::PathBuf::from(path)
    } else {
        normalized
    }
}

fn refuse_delete(loc: &Location) -> bool {
    let path = loc.path.trim().trim_end_matches(['/', '\\']);
    path.is_empty() || path == "/" || path == "\\" || (path.len() <= 3 && path.ends_with(':'))
}

fn delete_local(path: &Path) -> Result<()> {
    let meta =
        fs::symlink_metadata(path).map_err(|err| friendly(&path.display().to_string(), err))?;
    let result = if meta.file_type().is_symlink() || !meta.is_dir() {
        fs::remove_file(path)
    } else {
        fs::remove_dir_all(path)
    };
    result.map_err(|err| friendly(&path.display().to_string(), err))
}

fn stat_local(path: &Path) -> Result<FileProps> {
    let meta = fs::metadata(path)
        .or_else(|_| fs::symlink_metadata(path))
        .map_err(|err| friendly(&path.display().to_string(), err))?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        Some(meta.permissions().mode() & 0o777)
    };
    #[cfg(not(unix))]
    let mode = None;
    Ok(FileProps {
        name: path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.display().to_string(),
        ty: if meta.is_dir() {
            EntryType::Dir
        } else {
            EntryType::File
        },
        size: if meta.is_dir() { 0 } else { meta.len() },
        mtime,
        mode,
    })
}

fn friendly(path: &str, err: impl std::fmt::Display) -> anyhow::Error {
    let msg = err.to_string();
    let lower = msg.to_lowercase();
    if lower.contains("not found") || lower.contains("no such") {
        anyhow!("找不到路径：{path}")
    } else if lower.contains("permission") || lower.contains("denied") {
        anyhow!("没有权限：{path}")
    } else {
        anyhow!("{path}：{msg}")
    }
}
