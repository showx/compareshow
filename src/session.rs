use crate::model::{default_ignore, now_ms, CompareMode, RecentSession, SftpConnectionConfig};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreData {
    connections: Vec<SftpConnectionConfig>,
    recents: Vec<RecentSession>,
    ignore: Vec<String>,
    mode: CompareMode,
}

impl Default for StoreData {
    fn default() -> Self {
        Self {
            connections: Vec::new(),
            recents: Vec::new(),
            ignore: default_ignore(),
            mode: CompareMode::Quick,
        }
    }
}

pub struct SessionStore {
    path: PathBuf,
    inner: Mutex<StoreData>,
}

impl SessionStore {
    pub fn open() -> Self {
        let path = store_path();
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self {
            path,
            inner: Mutex::new(data),
        }
    }

    #[allow(dead_code)]
    pub fn connections(&self) -> Vec<SftpConnectionConfig> {
        self.lock().connections.clone()
    }

    pub fn recents(&self) -> Vec<RecentSession> {
        self.lock().recents.clone()
    }

    pub fn ignore(&self) -> Vec<String> {
        let ignore = self.lock().ignore.clone();
        if ignore.is_empty() {
            default_ignore()
        } else {
            ignore
        }
    }

    pub fn mode(&self) -> CompareMode {
        self.lock().mode
    }

    pub fn set_prefs(&self, mode: CompareMode, ignore: Vec<String>) {
        let mut data = self.lock();
        data.mode = mode;
        if !ignore.is_empty() {
            data.ignore = ignore;
        }
        self.save(&data);
    }

    pub fn find_by_endpoint(
        &self,
        username: &str,
        host: &str,
        port: u16,
    ) -> Option<SftpConnectionConfig> {
        let port = if port == 0 { 22 } else { port };
        self.lock()
            .connections
            .iter()
            .find(|c| {
                c.username.eq_ignore_ascii_case(username)
                    && c.host.eq_ignore_ascii_case(host)
                    && (if c.port == 0 { 22 } else { c.port }) == port
            })
            .cloned()
    }

    pub fn upsert(&self, mut config: SftpConnectionConfig) {
        let mut data = self.lock();
        if let Some(existing) = data.connections.iter().find(|c| c.id == config.id) {
            if config.created_at == 0 {
                config.created_at = existing.created_at;
            }
        }
        if config.created_at == 0 {
            config.created_at = now_ms();
        }
        config.last_used_at = now_ms();
        if let Some(slot) = data.connections.iter_mut().find(|c| c.id == config.id) {
            *slot = config;
        } else {
            data.connections.insert(0, config);
        }
        self.save(&data);
    }

    pub fn push_recent(&self, recent: RecentSession) {
        let mut data = self.lock();
        data.recents.retain(|item| {
            item.left_display != recent.left_display || item.right_display != recent.right_display
        });
        data.recents.insert(0, recent);
        data.recents.truncate(24);
        self.save(&data);
    }

    pub fn get_password(&self, id: &str) -> Option<String> {
        let entry = keyring::Entry::new(SERVICE, &password_account(id)).ok()?;
        entry.get_password().ok()
    }

    pub fn set_password(&self, id: &str, password: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, &password_account(id))
            .map_err(|err| anyhow!("无法打开钥匙串：{err}"))?;
        entry
            .set_password(password)
            .map_err(|err| anyhow!("无法写入钥匙串：{err}"))
    }

    pub fn delete_password(&self, id: &str) {
        if let Ok(entry) = keyring::Entry::new(SERVICE, &password_account(id)) {
            let _ = entry.delete_credential();
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StoreData> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn save(&self, data: &StoreData) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let Ok(text) = serde_json::to_string_pretty(data) else {
            return;
        };
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        if let Ok(mut file) = options.open(&self.path) {
            let _ = file.write_all(text.as_bytes());
        }
    }
}

const SERVICE: &str = "CompareShow2";

fn password_account(id: &str) -> String {
    format!("pw:{id}")
}

fn store_path() -> PathBuf {
    directories::ProjectDirs::from("com.compareshow", "CompareShow", "CompareShow2")
        .map(|dirs| dirs.data_dir().join("sessions.json"))
        .unwrap_or_else(|| {
            crate::paths::home_dir()
                .join(".compareshow2")
                .join("sessions.json")
        })
}
