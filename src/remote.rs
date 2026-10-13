use crate::model::{EntryType, FileMeta, FileProps};
use crate::paths::ssh_dir;
use crate::textutil::hash_reader;
use anyhow::{anyhow, Context, Result};
use ssh2::{CheckResult, KnownHostFileKind, Session};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

pub struct Dial {
    pub connection_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub identity_files: Vec<PathBuf>,
}

pub enum ConnectOutcome {
    Ready {
        connection_id: String,
        note: Option<String>,
    },
    NeedPassword {
        keys_tried: Vec<String>,
    },
}

struct Conn {
    session: Session,
}

struct PoolInner {
    by_id: HashMap<String, Arc<Mutex<Conn>>>,
    by_endpoint: HashMap<String, String>,
}

pub struct Pool {
    inner: Mutex<PoolInner>,
}

impl Pool {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(PoolInner {
                by_id: HashMap::new(),
                by_endpoint: HashMap::new(),
            }),
        }
    }

    pub fn connect(&self, dial: Dial) -> Result<ConnectOutcome> {
        let endpoint = endpoint_key(&dial.username, &dial.host, dial.port);
        if let Some(id) = self.live_id(&endpoint) {
            return Ok(ConnectOutcome::Ready {
                connection_id: id,
                note: None,
            });
        }

        let session = handshake(&dial.host, dial.port)?;
        let note = verify_host(&session, &dial.host, dial.port)?;
        let auth = authenticate(
            &session,
            &dial.username,
            &dial.identity_files,
            dial.password.as_deref(),
        )?;
        match auth {
            Auth::NeedPassword { keys_tried } => Ok(ConnectOutcome::NeedPassword { keys_tried }),
            Auth::Ready => {
                session.set_timeout(120_000);
                let id = dial.connection_id;
                let conn = Arc::new(Mutex::new(Conn { session }));
                let mut guard = self.lock();
                guard.by_endpoint.insert(endpoint, id.clone());
                guard.by_id.insert(id.clone(), conn);
                Ok(ConnectOutcome::Ready {
                    connection_id: id,
                    note,
                })
            }
        }
    }

    pub fn list(&self, id: &str, path: &str) -> Result<Vec<FileMeta>> {
        let path = if path.is_empty() { "/" } else { path };
        self.with_sftp(id, |sftp| {
            let entries = sftp
                .readdir(Path::new(path))
                .with_context(|| format!("列出 {path} 失败"))?;
            let mut items = Vec::new();
            for (entry_path, stat) in entries {
                let name = entry_path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if name.is_empty() || name == "." || name == ".." {
                    continue;
                }
                let full = crate::paths::posix_join(path, &name);
                let followed = if stat.file_type().is_symlink() {
                    sftp.stat(Path::new(&full)).unwrap_or(stat)
                } else {
                    stat
                };
                let ty = if followed.is_dir() {
                    EntryType::Dir
                } else {
                    EntryType::File
                };
                items.push(FileMeta {
                    name,
                    path: full,
                    ty,
                    size: if ty == EntryType::Dir {
                        0
                    } else {
                        followed.size.unwrap_or(0)
                    },
                    mtime: followed.mtime.map(|s| s as i64 * 1000).unwrap_or(0),
                });
            }
            items.sort_by(|a, b| {
                let ar = if a.ty == EntryType::Dir { 0 } else { 1 };
                let br = if b.ty == EntryType::Dir { 0 } else { 1 };
                ar.cmp(&br)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            Ok(items)
        })
    }

    pub fn hash_file(&self, id: &str, path: &str) -> Result<String> {
        self.with_sftp(id, |sftp| {
            let mut file = sftp.open(Path::new(path))?;
            hash_reader(&mut file).with_context(|| format!("计算哈希失败：{path}"))
        })
    }

    pub fn read_limited(
        &self,
        id: &str,
        path: &str,
        max: u64,
    ) -> Result<Option<(Vec<u8>, u64, i64)>> {
        self.with_sftp(id, |sftp| {
            let stat = match sftp.stat(Path::new(path)) {
                Ok(stat) => stat,
                Err(err) if is_missing(&err) => return Ok(None),
                Err(err) => return Err(friendly(path, err)),
            };
            if stat.is_dir() {
                anyhow::bail!("不是文件：{path}");
            }
            let size = stat.size.unwrap_or(0);
            let mtime = stat.mtime.map(|s| s as i64 * 1000).unwrap_or(0);
            if size > max {
                return Ok(Some((Vec::new(), size, mtime)));
            }
            let file = sftp.open(Path::new(path))?;
            let mut buf = Vec::new();
            file.take(max + 1).read_to_end(&mut buf)?;
            if buf.len() as u64 > max {
                return Ok(Some((Vec::new(), buf.len() as u64, mtime)));
            }
            Ok(Some((buf, size, mtime)))
        })
    }

    pub fn write_bytes(&self, id: &str, path: &str, data: &[u8]) -> Result<()> {
        self.with_sftp(id, |sftp| {
            if let Some(parent) = crate::paths::parent_posix(path) {
                mkdir_p(sftp, &parent)?;
            }
            let mut file = sftp.create(Path::new(path))?;
            file.write_all(data)?;
            Ok(())
        })
    }

    pub fn mkdir_p(&self, id: &str, path: &str) -> Result<()> {
        self.with_sftp(id, |sftp| mkdir_p(sftp, path))
    }

    pub fn delete_path(&self, id: &str, path: &str) -> Result<()> {
        self.with_sftp(id, |sftp| delete_remote(sftp, path))
    }

    pub fn stat(&self, id: &str, path: &str) -> Result<FileProps> {
        self.with_sftp(id, |sftp| {
            let stat = sftp
                .stat(Path::new(path))
                .or_else(|_| sftp.lstat(Path::new(path)))
                .with_context(|| format!("读取属性失败：{path}"))?;
            let ty = if stat.is_dir() {
                EntryType::Dir
            } else {
                EntryType::File
            };
            Ok(FileProps {
                name: crate::paths::file_name(path),
                path: path.to_string(),
                ty,
                size: if ty == EntryType::Dir {
                    0
                } else {
                    stat.size.unwrap_or(0)
                },
                mtime: stat.mtime.map(|s| s as i64 * 1000).unwrap_or(0),
                mode: stat.perm.map(|perm| perm & 0o777),
            })
        })
    }

    pub fn download(&self, id: &str, remote: &str, local: &Path) -> Result<()> {
        if let Some(parent) = local.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mtime = self.with_sftp(id, |sftp| {
            let mut src = sftp.open(Path::new(remote))?;
            let mut dst = File::create(local)?;
            std::io::copy(&mut src, &mut dst)?;
            let mtime = sftp
                .stat(Path::new(remote))
                .ok()
                .and_then(|s| s.mtime)
                .map(|s| s as i64 * 1000)
                .unwrap_or(0);
            Ok(mtime)
        })?;
        set_local_mtime(local, mtime)
    }

    pub fn upload(&self, id: &str, local: &Path, remote: &str) -> Result<()> {
        let mtime = File::open(local)?
            .metadata()?
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        self.with_sftp(id, |sftp| {
            if let Some(parent) = crate::paths::parent_posix(remote) {
                mkdir_p(sftp, &parent)?;
            }
            let mut src = File::open(local)?;
            let mut dst = sftp.create(Path::new(remote))?;
            std::io::copy(&mut src, &mut dst)?;
            set_remote_mtime(sftp, Path::new(remote), mtime)
        })
    }

    pub fn copy_same(&self, id: &str, src: &str, dest: &str) -> Result<()> {
        self.with_sftp(id, |sftp| {
            if let Some(parent) = crate::paths::parent_posix(dest) {
                mkdir_p(sftp, &parent)?;
            }
            let mtime = sftp
                .stat(Path::new(src))
                .ok()
                .and_then(|s| s.mtime)
                .map(|s| s as i64 * 1000)
                .unwrap_or(0);
            let mut from = sftp.open(Path::new(src))?;
            let mut to = sftp.create(Path::new(dest))?;
            std::io::copy(&mut from, &mut to)?;
            drop(from);
            drop(to);
            set_remote_mtime(sftp, Path::new(dest), mtime)
        })
    }

    fn live_id(&self, endpoint: &str) -> Option<String> {
        let id = {
            let guard = self.lock();
            guard.by_endpoint.get(endpoint).cloned()?
        };
        let alive = self
            .lookup(&id)
            .ok()
            .and_then(|conn| conn.lock().ok().map(|c| c.session.authenticated()))
            .unwrap_or(false);
        if alive {
            Some(id)
        } else {
            self.remove(&id);
            None
        }
    }

    fn with_sftp<T>(&self, id: &str, f: impl FnOnce(&ssh2::Sftp) -> Result<T>) -> Result<T> {
        let conn = self.lookup(id)?;
        let guard = conn.lock().unwrap_or_else(|err| err.into_inner());
        let result = (|| {
            let sftp = guard.session.sftp()?;
            f(&sftp)
        })();
        drop(guard);
        if let Err(err) = &result {
            if is_disconnect(err) {
                self.remove(id);
            }
        }
        result
    }

    fn lookup(&self, id: &str) -> Result<Arc<Mutex<Conn>>> {
        self.lock()
            .by_id
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("SFTP 连接已断开，请重新比较"))
    }

    fn remove(&self, id: &str) {
        let mut guard = self.lock();
        guard.by_id.remove(id);
        guard.by_endpoint.retain(|_, existing| existing != id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, PoolInner> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }
}

fn endpoint_key(user: &str, host: &str, port: u16) -> String {
    format!("{}@{}:{}", user.to_lowercase(), host.to_lowercase(), port)
}

fn handshake(host: &str, port: u16) -> Result<Session> {
    let addr = (host, port)
        .to_socket_addrs()
        .with_context(|| format!("无法解析主机 {host}"))?
        .next()
        .ok_or_else(|| anyhow!("无法解析主机 {host}"))?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(15))
        .with_context(|| format!("无法连接 {host}:{port}"))?;
    let mut session = Session::new()?;
    session.set_timeout(20_000);
    session.set_tcp_stream(tcp);
    session
        .handshake()
        .with_context(|| format!("SSH 握手失败 {host}:{port}"))?;
    Ok(session)
}

fn verify_host(session: &Session, host: &str, port: u16) -> Result<Option<String>> {
    let (key, key_type) = session
        .host_key()
        .ok_or_else(|| anyhow!("服务器没有返回主机密钥"))?;
    let mut combined = session.known_hosts()?;
    let user_file = ssh_dir().join("known_hosts");
    let our_file = ssh_dir().join("compareshow_known_hosts");
    if user_file.is_file() {
        let _ = combined.read_file(&user_file, KnownHostFileKind::OpenSSH);
    }
    if our_file.is_file() {
        let _ = combined.read_file(&our_file, KnownHostFileKind::OpenSSH);
    }
    let checked = if port == 22 {
        combined.check(host, key)
    } else {
        combined.check_port(host, port, key)
    };
    match checked {
        CheckResult::Match => Ok(None),
        CheckResult::Mismatch => {
            anyhow::bail!("主机密钥与 known_hosts 不一致，已拒绝连接 {host}:{port}")
        }
        CheckResult::Failure => Ok(Some("未能校验主机密钥，本次仍继续连接".into())),
        CheckResult::NotFound => {
            let mut ours = session.known_hosts()?;
            if our_file.is_file() {
                let _ = ours.read_file(&our_file, KnownHostFileKind::OpenSSH);
            }
            let entry = if port == 22 {
                host.to_string()
            } else {
                format!("[{host}]:{port}")
            };
            ours.add(&entry, key, "compareshow", key_type.into())?;
            if let Some(parent) = our_file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            ours.write_file(&our_file, KnownHostFileKind::OpenSSH)?;
            Ok(Some(
                "已信任新主机，写入 ~/.ssh/compareshow_known_hosts".into(),
            ))
        }
    }
}

enum Auth {
    Ready,
    NeedPassword { keys_tried: Vec<String> },
}

fn authenticate(
    session: &Session,
    username: &str,
    keys: &[PathBuf],
    password: Option<&str>,
) -> Result<Auth> {
    if std::env::var_os("SSH_AUTH_SOCK").is_some() {
        let _ = session.userauth_agent(username);
    }
    if session.authenticated() {
        return Ok(Auth::Ready);
    }

    let mut tried = Vec::new();
    for key in keys {
        tried.push(key.display().to_string());
        if let Some(password) = password {
            let _ = session.userauth_pubkey_file(username, None, key, Some(password));
            if session.authenticated() {
                return Ok(Auth::Ready);
            }
        }
        let err = session.userauth_pubkey_file(username, None, key, None);
        if session.authenticated() {
            return Ok(Auth::Ready);
        }
        if let Err(err) = err {
            if is_disconnect_msg(&err.to_string()) {
                return Err(anyhow!("连接在认证时中断：{err}"));
            }
        }
    }
    if session.authenticated() {
        return Ok(Auth::Ready);
    }
    if let Some(password) = password {
        let password_err = session.userauth_password(username, password).err();
        if session.authenticated() {
            return Ok(Auth::Ready);
        }
        let mut prompter = PasswordPrompt { password };
        let interactive_err = session
            .userauth_keyboard_interactive(username, &mut prompter)
            .err();
        if session.authenticated() {
            return Ok(Auth::Ready);
        }
        let detail = interactive_err.or(password_err);
        let message = detail
            .map(|err| err.to_string())
            .unwrap_or_else(|| "认证失败".into());
        if is_disconnect_msg(&message) {
            return Err(anyhow!("连接在认证时中断：{message}"));
        }
        return Err(anyhow!("认证失败，请检查用户名或密码"));
    }
    Ok(Auth::NeedPassword { keys_tried: tried })
}

struct PasswordPrompt<'a> {
    password: &'a str,
}

impl ssh2::KeyboardInteractivePrompt for PasswordPrompt<'_> {
    fn prompt(&mut self, _: &str, _: &str, prompts: &[ssh2::Prompt<'_>]) -> Vec<String> {
        prompts.iter().map(|_| self.password.to_string()).collect()
    }
}

fn mkdir_p(sftp: &ssh2::Sftp, path: &str) -> Result<()> {
    if path.is_empty() || path == "/" {
        return Ok(());
    }
    let mut current = String::new();
    for part in path.split('/').filter(|s| !s.is_empty()) {
        current.push('/');
        current.push_str(part);
        if sftp.stat(Path::new(&current)).is_ok() {
            continue;
        }
        if let Err(err) = sftp.mkdir(Path::new(&current), 0o755) {
            if sftp.stat(Path::new(&current)).is_err() {
                return Err(friendly(&current, err));
            }
        }
    }
    Ok(())
}

fn delete_remote(sftp: &ssh2::Sftp, path: &str) -> Result<()> {
    if path.is_empty() || path == "/" {
        anyhow::bail!("不能删除这个位置");
    }
    let stat = sftp
        .lstat(Path::new(path))
        .or_else(|_| sftp.stat(Path::new(path)))
        .with_context(|| format!("找不到 {path}"))?;
    if stat.file_type().is_symlink() || !stat.is_dir() {
        return sftp
            .unlink(Path::new(path))
            .with_context(|| format!("删除失败：{path}"));
    }
    let entries = sftp
        .readdir(Path::new(path))
        .with_context(|| format!("无法列出 {path}"))?;
    for (entry_path, _) in entries {
        let name = entry_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        delete_remote(sftp, &crate::paths::posix_join(path, &name))?;
    }
    sftp.rmdir(Path::new(path))
        .with_context(|| format!("删除文件夹失败：{path}"))
}

fn set_remote_mtime(sftp: &ssh2::Sftp, path: &Path, mtime_ms: i64) -> Result<()> {
    if mtime_ms <= 0 {
        return Ok(());
    }
    let secs = (mtime_ms / 1000) as u64;
    sftp.setstat(
        path,
        ssh2::FileStat {
            size: None,
            uid: None,
            gid: None,
            perm: None,
            atime: Some(secs),
            mtime: Some(secs),
        },
    )?;
    Ok(())
}

fn set_local_mtime(path: &Path, mtime_ms: i64) -> Result<()> {
    if mtime_ms <= 0 {
        return Ok(());
    }
    let when = UNIX_EPOCH + Duration::from_millis(mtime_ms as u64);
    File::open(path)?.set_modified(when)?;
    Ok(())
}

fn is_missing(err: &ssh2::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("no such file") || msg.contains("not found")
}

fn is_disconnect(err: &anyhow::Error) -> bool {
    is_disconnect_msg(&err.to_string())
}

fn is_disconnect_msg(msg: &str) -> bool {
    let msg = msg.to_lowercase();
    msg.contains("socket")
        || msg.contains("closed")
        || msg.contains("eof")
        || msg.contains("timed out")
        || msg.contains("timeout")
        || msg.contains("connection reset")
}

fn friendly(path: &str, err: impl std::fmt::Display) -> anyhow::Error {
    let msg = err.to_string();
    let lower = msg.to_lowercase();
    if lower.contains("no such") || lower.contains("not found") {
        anyhow!("找不到路径：{path}")
    } else if lower.contains("permission") || lower.contains("denied") {
        anyhow!("没有权限：{path}")
    } else {
        anyhow!("{path}：{msg}")
    }
}

impl Default for Pool {
    fn default() -> Self {
        Self::new()
    }
}
