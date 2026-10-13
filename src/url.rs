#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSftp {
    pub username: Option<String>,
    pub password: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
}

pub fn looks_like_sftp(raw: &str) -> bool {
    let text = raw.trim();
    if text.is_empty() {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("sftp://") || lower.starts_with("scp://") || lower.starts_with("ssh://") {
        return true;
    }
    if looks_like_local(text) {
        return false;
    }
    looks_like_user_at_host(text)
}

pub fn looks_like_local(text: &str) -> bool {
    let text = text.trim().trim_matches(|c| c == '"' || c == '\'');
    if text.is_empty() {
        return false;
    }
    if text.len() >= 5 && text[..5].eq_ignore_ascii_case("file:") {
        return true;
    }
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return true;
    }
    if text.starts_with('\\')
        || text.starts_with('/')
        || text == "~"
        || text.starts_with("~/")
        || text.starts_with("~\\")
    {
        return true;
    }
    text.starts_with('%')
}

fn looks_like_user_at_host(text: &str) -> bool {
    let Some((user, host)) = text.split_once('@') else {
        return false;
    };
    if user.is_empty()
        || user
            .chars()
            .any(|c| c.is_whitespace() || c == '/' || c == '\\')
    {
        return false;
    }
    let host = host.split(['/', '\\']).next().unwrap_or("");
    !host.is_empty() && !host.chars().any(char::is_whitespace) && !host.contains('@')
}

pub fn parse(raw: &str) -> Option<ParsedSftp> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    let (scheme_len, matched) = if lower.starts_with("sftp://") {
        (7, true)
    } else if lower.starts_with("scp://") || lower.starts_with("ssh://") {
        (6, true)
    } else {
        (0, false)
    };
    if !matched && !looks_like_sftp(text) {
        return None;
    }
    let rest = &text[scheme_len..];
    let (userinfo, hostportpath) = match rest.rfind('@') {
        Some(at) => (Some(&rest[..at]), &rest[at + 1..]),
        None => (None, rest),
    };
    let (username, password) = match userinfo {
        Some(info) => match info.split_once(':') {
            Some((user, pass)) => (decode(user), Some(decode(pass))),
            None => (decode(info), None),
        },
        None => (String::new(), None),
    };

    let (hostport, mut path) = split_host_path(hostportpath);
    let (host, port) = split_host_port(hostport)?;
    if !path.starts_with('/') {
        path = format!("/{path}");
    }
    if path.len() > 1 {
        path = path.trim_end_matches('/').to_string();
    }
    if path.is_empty() {
        path = "/".to_string();
    }
    Some(ParsedSftp {
        username: if username.is_empty() {
            None
        } else {
            Some(username)
        },
        password,
        host,
        port,
        path,
    })
}

pub fn format(username: &str, host: &str, port: u16, path: &str) -> String {
    let user = if username.is_empty() {
        String::new()
    } else {
        format!("{username}@")
    };
    let port_text = if port > 0 && port != 22 {
        format!(":{port}")
    } else {
        String::new()
    };
    let path = if path.is_empty() {
        "/".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    format!("sftp://{user}{host}{port_text}{path}")
}

fn split_host_path(hostportpath: &str) -> (&str, String) {
    if let Some(slash) = hostportpath.find('/') {
        let path = if slash + 1 >= hostportpath.len() && slash == 0 {
            "/".to_string()
        } else {
            hostportpath[slash..].to_string()
        };
        return (
            &hostportpath[..slash],
            if path.is_empty() { "/".into() } else { path },
        );
    }
    if let Some((host, rest)) = hostportpath.split_once(':') {
        if !rest.is_empty() && !rest.chars().all(|c| c.is_ascii_digit()) {
            let path = if rest.starts_with('/') {
                rest.to_string()
            } else {
                format!("/{rest}")
            };
            return (host, path);
        }
    }
    (hostportpath, "/".to_string())
}

fn split_host_port(hostport: &str) -> Option<(String, Option<u16>)> {
    let (host, port) = if let Some((host, port)) = hostport.rsplit_once(':') {
        if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() {
            (host, port.parse::<u16>().ok())
        } else {
            (hostport, None)
        }
    } else {
        (hostport, None)
    };
    let host = host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    if host.is_empty() {
        None
    } else {
        Some((host.to_string(), port))
    }
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_forms() {
        let a = parse("sftp://root@10.0.0.8/var/www").unwrap();
        assert_eq!(a.username.as_deref(), Some("root"));
        assert_eq!(a.host, "10.0.0.8");
        assert_eq!(a.port, None);
        assert_eq!(a.path, "/var/www");

        let b = parse("ssh://user@host:2222/etc").unwrap();
        assert_eq!(b.port, Some(2222));
        assert_eq!(b.path, "/etc");

        let c = parse("alice@example.com").unwrap();
        assert_eq!(c.path, "/");
        assert_eq!(c.host, "example.com");

        let d = parse("user@host:22").unwrap();
        assert_eq!(d.port, Some(22));
        assert_eq!(d.path, "/");

        let e = parse("user:p%40ss@host/tmp").unwrap();
        assert_eq!(e.password.as_deref(), Some("p@ss"));
        assert_eq!(e.path, "/tmp");
    }

    #[test]
    fn local_paths_are_not_sftp() {
        assert!(!looks_like_sftp("/Users/me"));
        assert!(!looks_like_sftp("~/code"));
        assert!(!looks_like_sftp(r"C:\work"));
        assert!(looks_like_sftp("sftp://host/var"));
    }

    #[test]
    fn format_omits_default_port() {
        assert_eq!(format("root", "h", 22, "/var"), "sftp://root@h/var");
        assert_eq!(format("root", "h", 2200, "/"), "sftp://root@h:2200/");
    }
}
