use similar::{ChangeTag, TextDiff};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffTag {
    Same,
    Changed,
    LeftOnly,
    RightOnly,
}

#[derive(Clone, Debug)]
pub struct DiffRow {
    pub left_no: Option<usize>,
    pub right_no: Option<usize>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub tag: DiffTag,
}

#[derive(Clone, Debug)]
pub struct Aligned {
    pub rows: Vec<DiffRow>,
    pub changed: usize,
}

pub fn align(left: &str, right: &str) -> Aligned {
    let diff = TextDiff::from_lines(left, right);
    let mut rows = Vec::new();
    let mut left_no = 1usize;
    let mut right_no = 1usize;
    let mut dels: Vec<(usize, String)> = Vec::new();
    let mut ins: Vec<(usize, String)> = Vec::new();

    let flush = |rows: &mut Vec<DiffRow>,
                 dels: &mut Vec<(usize, String)>,
                 ins: &mut Vec<(usize, String)>| {
        let n = dels.len().max(ins.len());
        for i in 0..n {
            let left = dels.get(i);
            let right = ins.get(i);
            let tag = match (left, right) {
                (Some(_), Some(_)) => DiffTag::Changed,
                (Some(_), None) => DiffTag::LeftOnly,
                (None, Some(_)) => DiffTag::RightOnly,
                (None, None) => continue,
            };
            rows.push(DiffRow {
                left_no: left.map(|(n, _)| *n),
                right_no: right.map(|(n, _)| *n),
                left: left.map(|(_, t)| t.clone()),
                right: right.map(|(_, t)| t.clone()),
                tag,
            });
        }
        dels.clear();
        ins.clear();
    };

    for change in diff.iter_all_changes() {
        let text = change.value().trim_end_matches(['\n', '\r']).to_string();
        match change.tag() {
            ChangeTag::Equal => {
                flush(&mut rows, &mut dels, &mut ins);
                rows.push(DiffRow {
                    left_no: Some(left_no),
                    right_no: Some(right_no),
                    left: Some(text.clone()),
                    right: Some(text),
                    tag: DiffTag::Same,
                });
                left_no += 1;
                right_no += 1;
            }
            ChangeTag::Delete => {
                dels.push((left_no, text));
                left_no += 1;
            }
            ChangeTag::Insert => {
                ins.push((right_no, text));
                right_no += 1;
            }
        }
    }
    flush(&mut rows, &mut dels, &mut ins);
    let changed = rows.iter().filter(|r| r.tag != DiffTag::Same).count();
    Aligned { rows, changed }
}

#[cfg(test)]
fn apply_copy(
    rows: &[DiffRow],
    index: usize,
    to_right: bool,
    hunk: bool,
    left_newline: &str,
    right_newline: &str,
    left_trailing: bool,
    right_trailing: bool,
) -> Option<(String, String)> {
    if index >= rows.len() {
        return None;
    }
    let indices: Vec<usize> = if hunk {
        hunk_indices(rows, index)
    } else {
        vec![index]
    };
    apply_copy_indices(
        rows,
        &indices,
        to_right,
        left_newline,
        right_newline,
        left_trailing,
        right_trailing,
    )
}

pub fn apply_copy_indices(
    rows: &[DiffRow],
    indices: &[usize],
    to_right: bool,
    left_newline: &str,
    right_newline: &str,
    left_trailing: bool,
    right_trailing: bool,
) -> Option<(String, String)> {
    let selected: HashSet<usize> = indices
        .iter()
        .copied()
        .filter(|i| *i < rows.len())
        .collect();
    if selected.is_empty() {
        return None;
    }
    let (left_lines, right_lines) = rebuild_selected(rows, &selected, to_right);
    Some((
        join_lines(&left_lines, left_newline, left_trailing),
        join_lines(&right_lines, right_newline, right_trailing),
    ))
}

pub fn hunk_indices(rows: &[DiffRow], index: usize) -> Vec<usize> {
    if index >= rows.len() {
        return Vec::new();
    }
    let (start, end) = hunk_bounds(rows, index);
    (start..end).collect()
}

fn hunk_bounds(rows: &[DiffRow], index: usize) -> (usize, usize) {
    if rows[index].tag == DiffTag::Same {
        return (index, index + 1);
    }
    let mut start = index;
    while start > 0 && rows[start - 1].tag != DiffTag::Same {
        start -= 1;
    }
    let mut end = index + 1;
    while end < rows.len() && rows[end].tag != DiffTag::Same {
        end += 1;
    }
    (start, end)
}

fn rebuild_selected(
    rows: &[DiffRow],
    selected: &HashSet<usize>,
    to_right: bool,
) -> (Vec<String>, Vec<String>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let picked = selected.contains(&i);
        if to_right {
            if let Some(text) = &row.left {
                left.push(text.clone());
            }
            if picked {
                if let Some(text) = &row.left {
                    right.push(text.clone());
                }
            } else if let Some(text) = &row.right {
                right.push(text.clone());
            }
        } else {
            if let Some(text) = &row.right {
                right.push(text.clone());
            }
            if picked {
                if let Some(text) = &row.right {
                    left.push(text.clone());
                }
            } else if let Some(text) = &row.left {
                left.push(text.clone());
            }
        }
    }
    (left, right)
}

pub fn join_lines(lines: &[String], newline: &str, trailing: bool) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut text = lines.join(newline);
    if trailing {
        text.push_str(newline);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_changes_and_copies_hunk() {
        let aligned = align("a\nb\nc\n", "a\nx\nc\n");
        assert_eq!(aligned.changed, 1);
        assert_eq!(aligned.rows[1].tag, DiffTag::Changed);
        assert_eq!(aligned.rows[1].left.as_deref(), Some("b"));
        assert_eq!(aligned.rows[1].right.as_deref(), Some("x"));

        let (left, right) =
            apply_copy(&aligned.rows, 1, true, true, "\n", "\n", true, true).unwrap();
        assert_eq!(left, "a\nb\nc\n");
        assert_eq!(right, "a\nb\nc\n");
    }

    #[test]
    fn copy_line_inserts_left_only_text() {
        let aligned = align("keep\nextra\n", "keep\n");
        let idx = aligned
            .rows
            .iter()
            .position(|r| r.tag == DiffTag::LeftOnly)
            .unwrap();
        let (_, right) =
            apply_copy(&aligned.rows, idx, true, false, "\n", "\n", true, true).unwrap();
        assert_eq!(right, "keep\nextra\n");
    }

    #[test]
    fn copies_only_the_selected_lines() {
        let aligned = align("a\nb\nc\nd\n", "a\nB\nC\nd\n");
        let (_, right) =
            apply_copy_indices(&aligned.rows, &[1], true, "\n", "\n", true, true).unwrap();
        assert_eq!(right, "a\nb\nC\nd\n");
        let (_, right) =
            apply_copy_indices(&aligned.rows, &[1, 2], true, "\n", "\n", true, true).unwrap();
        assert_eq!(right, "a\nb\nc\nd\n");
    }
}
