use crate::paths::{home_dir, ssh_dir};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ResolvedSsh {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub identity_files: Vec<PathBuf>,
}

pub fn resolve(host: &str, username: Option<&str>, port: Option<u16>) -> ResolvedSsh {
    let home = home_dir();
    let text = std::fs::read_to_string(ssh_dir().join("config")).unwrap_or_default();
    let defaults = default_identity_files(&home);
    resolve_from(&text, host, username, port, &home, &defaults)
}

pub fn resolve_from(
    config: &str,
    host: &str,
    username: Option<&str>,
    port: Option<u16>,
    home: &Path,
    default_keys: &[PathBuf],
) -> ResolvedSsh {
    let blocks = parse_config(config);
    let matched: Vec<&HostBlock> = blocks
        .iter()
        .filter(|b| block_matches(&b.patterns, host))
        .collect();

    let resolved_host = matched
        .iter()
        .find_map(|b| b.host_name.clone())
        .unwrap_or_else(|| host.to_string());
    let resolved_user = username
        .map(str::to_string)
        .or_else(|| matched.iter().find_map(|b| b.user.clone()))
        .or_else(|| std::env::var("USER").ok())
        .or_else(|| std::env::var("USERNAME").ok())
        .unwrap_or_else(|| "root".to_string());
    let resolved_port = port
        .or_else(|| matched.iter().find_map(|b| b.port))
        .unwrap_or(22);

    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for path in matched.iter().flat_map(|b| b.identity_files.iter()) {
        let expanded = expand_path(path, home);
        if seen.insert(expanded.clone()) {
            files.push(expanded);
        }
    }
    for path in default_keys {
        if seen.insert(path.clone()) {
            files.push(path.clone());
        }
    }
    let identity_files = files.into_iter().filter(|p| p.is_file()).collect();

    ResolvedSsh {
        host: resolved_host,
        port: resolved_port,
        username: resolved_user,
        identity_files,
    }
}

fn default_identity_files(home: &Path) -> Vec<PathBuf> {
    ["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"]
        .into_iter()
        .map(|name| home.join(".ssh").join(name))
        .filter(|p| p.is_file())
        .collect()
}

struct HostBlock {
    patterns: Vec<String>,
    host_name: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_files: Vec<String>,
}

fn parse_config(text: &str) -> Vec<HostBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<HostBlock> = None;
    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = split_kv(line) else {
            continue;
        };
        let key = key.to_ascii_lowercase();
        if key == "host" {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            current = Some(HostBlock {
                patterns: value.split_whitespace().map(str::to_string).collect(),
                host_name: None,
                user: None,
                port: None,
                identity_files: Vec::new(),
            });
            continue;
        }
        let Some(block) = current.as_mut() else {
            continue;
        };
        match key.as_str() {
            "hostname" => block.host_name = Some(value.to_string()),
            "user" => block.user = Some(value.to_string()),
            "port" => block.port = value.parse().ok(),
            "identityfile" => block.identity_files.push(value.to_string()),
            _ => {}
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    blocks
}

fn split_kv(line: &str) -> Option<(&str, &str)> {
    let idx = line.find([' ', '\t', '='])?;
    let key = line[..idx].trim();
    let value = line[idx + 1..].trim().trim_matches('"');
    if key.is_empty() || value.is_empty() {
        None
    } else {
        Some((key, value))
    }
}

fn strip_comment(line: &str) -> &str {
    let mut quote = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quote = !quote,
            '#' if !quote => return &line[..i],
            _ => {}
        }
    }
    line
}

fn block_matches(patterns: &[String], host: &str) -> bool {
    let mut any_positive = false;
    for pattern in patterns {
        if let Some(negated) = pattern.strip_prefix('!') {
            if glob_match(negated, host) {
                return false;
            }
        } else if glob_match(pattern, host) {
            any_positive = true;
        }
    }
    any_positive
}

fn glob_match(pattern: &str, host: &str) -> bool {
    fn rec(pattern: &[char], host: &[char]) -> bool {
        if pattern.is_empty() {
            return host.is_empty();
        }
        if pattern[0] == '*' {
            if rec(&pattern[1..], host) {
                return true;
            }
            return !host.is_empty() && rec(pattern, &host[1..]);
        }
        if host.is_empty() {
            return false;
        }
        let ok = pattern[0] == '?' || pattern[0] == host[0];
        ok && rec(&pattern[1..], &host[1..])
    }
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let host: Vec<char> = host.to_lowercase().chars().collect();
    rec(&pattern, &host)
}

fn expand_path(path: &str, home: &Path) -> PathBuf {
    if path == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        return home.join(rest);
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specific_host_overrides_star() {
        let config = r#"
            Host web
                HostName 10.1.2.3
                User deploy
                Port 2201
                IdentityFile ~/.ssh/web_key
            Host *
                User fallback
                Port 22
        "#;
        let home = Path::new("/Users/me");
        let resolved = resolve_from(config, "web", None, None, home, &[]);
        assert_eq!(resolved.host, "10.1.2.3");
        assert_eq!(resolved.username, "deploy");
        assert_eq!(resolved.port, 2201);
    }

    #[test]
    fn explicit_user_wins_and_negation_excludes() {
        let config = "Host * !skip\n    User alice\n";
        let home = Path::new("/tmp");
        let matched = resolve_from(config, "box", Some("bob"), Some(22), home, &[]);
        assert_eq!(matched.username, "bob");
        let skipped = resolve_from(config, "skip", None, None, home, &[]);
        assert_eq!(skipped.username, whoami_or_root());
    }

    fn whoami_or_root() -> String {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "root".into())
    }
}
