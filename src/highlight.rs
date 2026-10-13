use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, Color32, FontId, Galley};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

const MAX_LINES: usize = 12_000;

#[derive(Clone, Debug)]
pub struct HlSpan {
    pub start: usize,
    pub end: usize,
    pub color: Color32,
}

pub fn preload() {
    let _ = assets();
}

pub fn language_label(path: &str, sample: &str) -> String {
    let (set, _) = assets();
    let syntax = detect(set, path, sample);
    if syntax.name == "Plain Text" {
        "纯文本".into()
    } else {
        syntax.name.clone()
    }
}

pub fn color_lines(text: &str, path: &str) -> Vec<Vec<HlSpan>> {
    if text.is_empty() {
        return Vec::new();
    }
    let (set, theme) = assets();
    let syntax = detect(set, path, text);
    if syntax.name == "Plain Text" {
        return Vec::new();
    }
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut out = Vec::new();
    for line in LinesWithEndings::from(text) {
        if out.len() >= MAX_LINES {
            break;
        }
        let ranges = highlighter.highlight_line(line, set).unwrap_or_default();
        out.push(spans_of(&ranges));
    }
    out
}

pub fn layout_galley(
    ui: &egui::Ui,
    slot: usize,
    text: &str,
    wrap: f32,
    path: &str,
) -> std::sync::Arc<Galley> {
    let wrap_key = wrap.to_bits();
    let key = cache_key(path, text, wrap_key);
    let slot = slot.min(1);
    if let Some(hit) = cache_get(slot, key, wrap_key) {
        return hit;
    }
    let font = FontId::monospace(13.0);
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let colored = color_lines(text, path);
    if colored.is_empty() {
        job.append(text, 0.0, text_format(font, crate::theme::TEXT));
    } else {
        for (line_i, line) in text.split_inclusive('\n').enumerate() {
            if line_i >= colored.len() {
                job.append(line, 0.0, text_format(font.clone(), crate::theme::TEXT));
                continue;
            }
            let body = line.trim_end_matches(['\n', '\r']);
            append_spans(&mut job, body, &colored[line_i], font.clone());
            if line.len() > body.len() {
                job.append(
                    &line[body.len()..],
                    0.0,
                    text_format(font.clone(), crate::theme::TEXT),
                );
            }
        }
    }
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    cache_put(slot, key, wrap_key, galley.clone());
    galley
}

pub fn append_spans(job: &mut LayoutJob, text: &str, spans: &[HlSpan], font: FontId) {
    if text.is_empty() {
        return;
    }
    if spans.is_empty() {
        job.append(text, 0.0, text_format(font, crate::theme::TEXT));
        return;
    }
    let mut cursor = 0usize;
    for span in spans {
        let start = span.start.min(text.len());
        let end = span.end.min(text.len()).max(start);
        if start > cursor {
            job.append(
                &text[cursor..start],
                0.0,
                text_format(font.clone(), crate::theme::TEXT),
            );
        }
        if end > start {
            job.append(
                &text[start..end],
                0.0,
                text_format(font.clone(), span.color),
            );
        }
        cursor = end;
    }
    if cursor < text.len() {
        job.append(&text[cursor..], 0.0, text_format(font, crate::theme::TEXT));
    }
}

fn spans_of(ranges: &[(syntect::highlighting::Style, &str)]) -> Vec<HlSpan> {
    let mut spans = Vec::new();
    let mut pos = 0usize;
    for (style, piece) in ranges {
        let piece = piece.trim_end_matches(['\n', '\r']);
        if piece.is_empty() {
            continue;
        }
        let start = pos;
        pos += piece.len();
        spans.push(HlSpan {
            start,
            end: pos,
            color: color_of(style.foreground),
        });
    }
    spans
}

fn color_of(color: syntect::highlighting::Color) -> Color32 {
    if color.a == 0 {
        crate::theme::TEXT
    } else {
        Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
    }
}

fn text_format(font: FontId, color: Color32) -> TextFormat {
    TextFormat {
        font_id: font,
        color,
        ..Default::default()
    }
}

fn detect<'a>(set: &'a SyntaxSet, path: &str, sample: &str) -> &'a SyntaxReference {
    let file = crate::paths::file_name(path);
    let ext = file
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .unwrap_or("")
        .to_ascii_lowercase();
    if !ext.is_empty() {
        if let Some(syntax) = set.find_syntax_by_extension(&ext) {
            return syntax;
        }
    }
    let lower = file.to_ascii_lowercase();
    for name in special_names(&lower) {
        if let Some(syntax) = set.find_syntax_by_name(name) {
            return syntax;
        }
    }
    let first = sample.lines().next().unwrap_or("");
    if let Some(syntax) = set.find_syntax_by_first_line(first) {
        return syntax;
    }
    set.find_syntax_plain_text()
}

fn special_names(file: &str) -> &'static [&'static str] {
    match file {
        "dockerfile" => &["Dockerfile"],
        "makefile" | "gnumakefile" => &["Makefile"],
        "cargo.lock" => &["TOML"],
        _ => &[],
    }
}

fn assets() -> &'static (SyntaxSet, Theme) {
    static ASSETS: OnceLock<(SyntaxSet, Theme)> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let set = SyntaxSet::load_defaults_newlines();
        let themes = ThemeSet::load_defaults();
        let theme = themes
            .themes
            .get("base16-ocean.dark")
            .cloned()
            .unwrap_or_else(|| {
                themes
                    .themes
                    .into_values()
                    .next()
                    .unwrap_or_else(Theme::default)
            });
        (set, theme)
    })
}

fn cache_key(path: &str, text: &str, wrap: u32) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    text.hash(&mut hasher);
    wrap.hash(&mut hasher);
    hasher.finish()
}

struct CachedGalley {
    key: u64,
    wrap: u32,
    galley: std::sync::Arc<Galley>,
}

fn cache() -> &'static Mutex<[Option<CachedGalley>; 2]> {
    static CACHE: OnceLock<Mutex<[Option<CachedGalley>; 2]>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new([None, None]))
}

fn cache_get(slot: usize, key: u64, wrap: u32) -> Option<std::sync::Arc<Galley>> {
    let guard = cache().lock().ok()?;
    guard[slot]
        .as_ref()
        .and_then(|item| (item.key == key && item.wrap == wrap).then(|| item.galley.clone()))
}

fn cache_put(slot: usize, key: u64, wrap: u32, galley: std::sync::Arc<Galley>) {
    if let Ok(mut guard) = cache().lock() {
        guard[slot] = Some(CachedGalley { key, wrap, galley });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_source_uses_more_than_one_color() {
        let lines = color_lines("fn main() {\n    let x = 1;\n}\n", "src/main.rs");
        assert_eq!(lines.len(), 3);
        let colors: Vec<_> = lines[0].iter().map(|span| span.color).collect();
        assert!(colors.len() > 1);
        assert!(colors.windows(2).any(|pair| pair[0] != pair[1]));
        assert_eq!(language_label("src/main.rs", ""), "Rust");
    }

    #[test]
    fn plain_text_stays_uncolored() {
        assert!(color_lines("hello\n", "notes.txt").is_empty());
        assert_eq!(language_label("notes.txt", "hello"), "纯文本");
    }
}
