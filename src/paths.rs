use std::path::{Component, Path, PathBuf};

pub fn home_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn ssh_dir() -> PathBuf {
    home_dir().join(".ssh")
}

pub fn file_name(path: &str) -> String {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

pub fn posix_join(base: &str, name: &str) -> String {
    if base.is_empty() || base == "/" {
        format!("/{name}")
    } else {
        format!("{}/{}", base.trim_end_matches('/'), name)
    }
}

pub fn parent_posix(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.rfind('/') {
        None => None,
        Some(0) => Some("/".to_string()),
        Some(i) => Some(trimmed[..i].to_string()),
    }
}

pub fn parent_local(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    if parent.as_os_str().is_empty() || parent == path {
        return None;
    }
    Some(parent.to_path_buf())
}

pub fn normalize_local(input: &str) -> PathBuf {
    let mut text = input.trim().to_string();
    if text.len() >= 2 {
        let bytes = text.as_bytes();
        let quote = bytes[0];
        if (quote == b'"' || quote == b'\'') && bytes[bytes.len() - 1] == quote {
            text = text[1..text.len() - 1].trim().to_string();
        }
    }

    if let Some(rest) = strip_prefix_ignore_ascii(&text, "file://") {
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        text = percent_decode(rest);
    }

    let home = home_dir();
    if text == "~" {
        text = home.display().to_string();
    } else if let Some(rest) = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\")) {
        text = home.join(rest).display().to_string();
    }

    text = expand_env(&text);

    let raw = PathBuf::from(&text);
    let absolute = if raw.is_absolute() {
        raw
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(raw)
    };
    lexical_normalize(&absolute)
}

pub fn parent_of_input(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if crate::url::looks_like_sftp(text) {
        let parsed = crate::url::parse(text)?;
        let parent = parent_posix(&parsed.path)?;
        let user = parsed.username.unwrap_or_default();
        return Some(crate::url::format(
            &user,
            &parsed.host,
            parsed.port.unwrap_or(22),
            &parent,
        ));
    }
    parent_local(&normalize_local(text)).map(|p| p.display().to_string())
}

fn strip_prefix_ignore_ascii<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    if text.len() < prefix.len() {
        return None;
    }
    if text[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&text[prefix.len()..])
    } else {
        None
    }
}

fn expand_env(text: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            if let Some(end) = chars[i + 1..].iter().position(|c| *c == '%') {
                let key: String = chars[i + 1..i + 1 + end].iter().collect();
                if let Ok(val) = std::env::var(&key) {
                    out.push_str(&val);
                    i += end + 2;
                    continue;
                }
            }
        }
        if chars[i] == '$' {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            if end > start {
                let key: String = chars[start..end].iter().collect();
                if let Ok(val) = std::env::var(&key) {
                    out.push_str(&val);
                    i = end;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_parent_and_join() {
        assert_eq!(posix_join("/var", "www"), "/var/www");
        assert_eq!(posix_join("/", "etc"), "/etc");
        assert_eq!(parent_posix("/var/www").as_deref(), Some("/var"));
        assert_eq!(parent_posix("/var").as_deref(), Some("/"));
        assert_eq!(parent_posix("/"), None);
    }

    #[test]
    fn expands_tilde() {
        let home = home_dir();
        let got = normalize_local("~/Documents");
        assert_eq!(got, lexical_normalize(&home.join("Documents")));
    }
}
