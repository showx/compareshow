use md5::{Digest, Md5};
use std::io::Read;

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut n = bytes as f64;
    let mut i = 0;
    while n >= 1024.0 && i < UNITS.len() - 1 {
        n /= 1024.0;
        i += 1;
    }
    if i > 0 && n < 10.0 {
        format!("{n:.1} {}", UNITS[i])
    } else {
        format!("{} {}", n.round() as u64, UNITS[i])
    }
}

pub fn format_mode(mode: u32) -> String {
    let flags = ['r', 'w', 'x'];
    let mut text = String::with_capacity(9);
    for i in 0..9 {
        let bit = 1 << (8 - i);
        text.push(if mode & bit != 0 { flags[i % 3] } else { '-' });
    }
    format!("{text} ({:03o})", mode & 0o777)
}

pub fn format_time(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let Some(dt) = chrono::DateTime::from_timestamp_millis(ms) else {
        return String::new();
    };
    dt.with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

pub fn hash_bytes(data: &[u8]) -> String {
    format!("{:x}", Md5::digest(data))
}

pub fn hash_reader(mut reader: impl Read) -> anyhow::Result<String> {
    let mut hasher = Md5::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn is_probably_binary(buf: &[u8]) -> bool {
    let sample = if buf.len() > 8192 { &buf[..8192] } else { buf };
    if sample.contains(&0) {
        return true;
    }
    let weird = sample
        .iter()
        .filter(|b| **b < 7 || (**b > 14 && **b < 32 && **b != 9 && **b != 10 && **b != 13))
        .count();
    !sample.is_empty() && (weird as f64) / (sample.len() as f64) > 0.3
}

pub struct Decoded {
    pub text: String,
    pub encoding: String,
    pub newline: String,
    pub trailing_newline: bool,
}

pub fn decode_text(buf: &[u8]) -> Decoded {
    if buf.len() >= 2 && buf[0] == 0xFF && buf[1] == 0xFE {
        let units: Vec<u16> = buf[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let text = String::from_utf16_lossy(&units);
        return finish(text, "utf-16le");
    }
    if buf.len() >= 3 && buf[0] == 0xEF && buf[1] == 0xBB && buf[2] == 0xBF {
        let text = String::from_utf8_lossy(&buf[3..]).into_owned();
        return finish(text, "utf-8");
    }
    finish(String::from_utf8_lossy(buf).into_owned(), "utf-8")
}

fn finish(text: String, encoding: &str) -> Decoded {
    let newline = if text.contains("\r\n") {
        "\r\n".to_string()
    } else {
        "\n".to_string()
    };
    let trailing_newline = text.ends_with('\n');
    Decoded {
        text,
        encoding: encoding.to_string(),
        newline,
        trailing_newline,
    }
}

pub fn encode_text(text: &str, encoding: &str) -> Vec<u8> {
    if encoding.eq_ignore_ascii_case("utf-16le") {
        let mut out = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out
    } else {
        text.as_bytes().to_vec()
    }
}

pub fn ignored(name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        let token = pattern.trim();
        if token.is_empty() {
            return false;
        }
        if token.contains('*') || token.contains('?') {
            glob_match(token, name)
        } else {
            token.to_lowercase() == name.to_lowercase()
        }
    })
}

fn glob_match(pattern: &str, text: &str) -> bool {
    fn rec(pattern: &[char], text: &[char]) -> bool {
        if pattern.is_empty() {
            return text.is_empty();
        }
        if pattern[0] == '*' {
            if rec(&pattern[1..], text) {
                return true;
            }
            return !text.is_empty() && rec(pattern, &text[1..]);
        }
        if text.is_empty() {
            return false;
        }
        (pattern[0] == '?' || pattern[0] == text[0]) && rec(&pattern[1..], &text[1..])
    }
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    rec(&pattern, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_detects_nul_and_keeps_utf8() {
        assert!(is_probably_binary(b"a\0b"));
        assert!(!is_probably_binary("你好\nfn main() {}".as_bytes()));
    }

    #[test]
    fn ignore_glob() {
        let patterns = vec!["*.log".into(), ".git".into()];
        assert!(ignored("app.LOG", &patterns));
        assert!(ignored(".git", &patterns));
        assert!(!ignored("src", &patterns));
    }
}
