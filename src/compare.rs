use crate::fsops::{FsOps, ReadOutcome};
use crate::model::*;
use crate::textutil::{decode_text, hash_bytes, ignored, is_probably_binary};
use anyhow::Result;
#[cfg(test)]
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
pub fn compare_folders(
    fs: &dyn FsOps,
    left: &Location,
    right: &Location,
    mode: CompareMode,
    ignore: &[String],
    progress: &mut dyn FnMut(usize, &str),
    cancel: &AtomicBool,
) -> Result<FolderCompare> {
    let mut stats = CompareStats::default();
    let mut scanned = 0usize;
    let tree = walk(
        fs,
        left,
        right,
        "",
        mode,
        ignore,
        &mut scanned,
        progress,
        cancel,
        &mut stats,
    )?;
    Ok(FolderCompare { tree, stats })
}

#[cfg(test)]
fn walk(
    fs: &dyn FsOps,
    left: &Location,
    right: &Location,
    rel: &str,
    mode: CompareMode,
    ignore: &[String],
    scanned: &mut usize,
    progress: &mut dyn FnMut(usize, &str),
    cancel: &AtomicBool,
    stats: &mut CompareStats,
) -> Result<Vec<CompareNode>> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("已取消");
    }
    let left_list = match fs.list(left) {
        Ok(items) => items,
        Err(err) if rel.is_empty() => return Err(err),
        Err(_) => Vec::new(),
    };
    let right_list = match fs.list(right) {
        Ok(items) => items,
        Err(err) if rel.is_empty() => return Err(err),
        Err(_) => Vec::new(),
    };

    let mut left_map = std::collections::HashMap::<String, FileMeta>::new();
    let mut right_map = std::collections::HashMap::<String, FileMeta>::new();
    let mut names = Vec::new();
    for entry in left_list {
        if ignored(&entry.name, ignore) {
            continue;
        }
        if !left_map.contains_key(&entry.name) {
            names.push(entry.name.clone());
        }
        left_map.insert(entry.name.clone(), entry);
    }
    for entry in right_list {
        if ignored(&entry.name, ignore) {
            continue;
        }
        if !left_map.contains_key(&entry.name) && !right_map.contains_key(&entry.name) {
            names.push(entry.name.clone());
        }
        right_map.insert(entry.name.clone(), entry);
    }
    names.sort_by(|a, b| {
        let ar = rank(left_map.get(a).or_else(|| right_map.get(a)));
        let br = rank(left_map.get(b).or_else(|| right_map.get(b)));
        ar.cmp(&br)
            .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
    });

    let mut nodes = Vec::new();
    for name in names {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!("已取消");
        }
        let left_meta = left_map.remove(&name);
        let right_meta = right_map.remove(&name);
        let rel_path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        *scanned += 1;
        if *scanned == 1 || *scanned % 10 == 0 {
            progress(*scanned, &rel_path);
        }

        let ty = entry_type(left_meta.as_ref(), right_meta.as_ref());
        let mut node = CompareNode {
            name,
            rel_path,
            ty,
            status: ItemStatus::Unknown,
            left: left_meta,
            right: right_meta,
            children: Vec::new(),
        };

        if let (Some(l), Some(r)) = (&node.left, &node.right) {
            if l.ty != r.ty {
                node.status = ItemStatus::Different;
                count(&node, stats);
                nodes.push(node);
                continue;
            }
        }

        if node.ty == EntryType::Dir {
            if node.left.is_some() && node.right.is_some() {
                node.children = walk(
                    fs,
                    &left.child(&node.name),
                    &right.child(&node.name),
                    &node.rel_path,
                    mode,
                    ignore,
                    scanned,
                    progress,
                    cancel,
                    stats,
                )?;
                if node.children.is_empty() {
                    node.status = ItemStatus::Same;
                } else {
                    rollup(&mut node);
                }
            } else if node.left.is_some() {
                node.children = walk_only(
                    fs,
                    &left.child(&node.name),
                    &node.rel_path,
                    ItemStatus::LeftOnly,
                    true,
                    ignore,
                    scanned,
                    progress,
                    cancel,
                    stats,
                )?;
                if node.children.is_empty() {
                    node.status = ItemStatus::LeftOnly;
                } else {
                    rollup(&mut node);
                }
            } else {
                node.children = walk_only(
                    fs,
                    &right.child(&node.name),
                    &node.rel_path,
                    ItemStatus::RightOnly,
                    false,
                    ignore,
                    scanned,
                    progress,
                    cancel,
                    stats,
                )?;
                if node.children.is_empty() {
                    node.status = ItemStatus::RightOnly;
                } else {
                    rollup(&mut node);
                }
            }
        } else {
            node.status = file_status(
                fs,
                node.left.as_ref(),
                node.right.as_ref(),
                mode,
                left,
                right,
            )?;
        }
        count(&node, stats);
        nodes.push(node);
    }
    Ok(nodes)
}

#[cfg(test)]
fn walk_only(
    fs: &dyn FsOps,
    loc: &Location,
    rel: &str,
    status: ItemStatus,
    left_side: bool,
    ignore: &[String],
    scanned: &mut usize,
    progress: &mut dyn FnMut(usize, &str),
    cancel: &AtomicBool,
    stats: &mut CompareStats,
) -> Result<Vec<CompareNode>> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("已取消");
    }
    let mut list = fs.list(loc).unwrap_or_default();
    list.retain(|e| !ignored(&e.name, ignore));
    list.sort_by(|a, b| {
        rank(Some(a))
            .cmp(&rank(Some(b)))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let mut nodes = Vec::new();
    for entry in list {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!("已取消");
        }
        let rel_path = if rel.is_empty() {
            entry.name.clone()
        } else {
            format!("{rel}/{}", entry.name)
        };
        *scanned += 1;
        if *scanned % 10 == 0 {
            progress(*scanned, &rel_path);
        }
        let mut node = CompareNode {
            name: entry.name.clone(),
            rel_path,
            ty: entry.ty,
            status,
            left: if left_side { Some(entry.clone()) } else { None },
            right: if left_side { None } else { Some(entry.clone()) },
            children: Vec::new(),
        };
        if entry.ty == EntryType::Dir {
            node.children = walk_only(
                fs,
                &loc.child(&entry.name),
                &node.rel_path,
                status,
                left_side,
                ignore,
                scanned,
                progress,
                cancel,
                stats,
            )?;
            if !node.children.is_empty() {
                rollup(&mut node);
            }
        }
        count(&node, stats);
        nodes.push(node);
    }
    Ok(nodes)
}

/// 只比较当前这一层。空的一侧当作没有文件，不会整棵树递归下去。
pub fn compare_level(
    fs: &dyn FsOps,
    left: Option<&Location>,
    right: Option<&Location>,
    mode: CompareMode,
    ignore: &[String],
    progress: &mut dyn FnMut(usize, &str),
    cancel: &AtomicBool,
) -> Result<FolderCompare> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("已取消");
    }
    let left_list = match left {
        Some(loc) => fs.list(loc)?,
        None => Vec::new(),
    };
    let right_list = match right {
        Some(loc) => fs.list(loc)?,
        None => Vec::new(),
    };

    let mut left_map = std::collections::HashMap::<String, FileMeta>::new();
    let mut right_map = std::collections::HashMap::<String, FileMeta>::new();
    let mut names = Vec::new();
    for entry in left_list {
        if ignored(&entry.name, ignore) {
            continue;
        }
        if !left_map.contains_key(&entry.name) {
            names.push(entry.name.clone());
        }
        left_map.insert(entry.name.clone(), entry);
    }
    for entry in right_list {
        if ignored(&entry.name, ignore) {
            continue;
        }
        if !left_map.contains_key(&entry.name) && !right_map.contains_key(&entry.name) {
            names.push(entry.name.clone());
        }
        right_map.insert(entry.name.clone(), entry);
    }
    names.sort_by(|a, b| {
        let ar = rank(left_map.get(a).or_else(|| right_map.get(a)));
        let br = rank(left_map.get(b).or_else(|| right_map.get(b)));
        ar.cmp(&br)
            .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
    });

    let left_root = left
        .cloned()
        .unwrap_or_else(|| Location::local(String::new()));
    let right_root = right
        .cloned()
        .unwrap_or_else(|| Location::local(String::new()));
    let mut stats = CompareStats::default();
    let mut nodes = Vec::new();
    for (index, name) in names.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!("已取消");
        }
        let left_meta = left_map.remove(&name);
        let right_meta = right_map.remove(&name);
        if index == 0 || (index + 1) % 10 == 0 {
            progress(index + 1, &name);
        }
        let ty = entry_type(left_meta.as_ref(), right_meta.as_ref());
        let status = match (
            left_meta.as_ref().map(|meta| meta.ty),
            right_meta.as_ref().map(|meta| meta.ty),
        ) {
            (Some(a), Some(b)) if a != b => ItemStatus::Different,
            (Some(EntryType::Dir), Some(EntryType::Dir)) => ItemStatus::Unknown,
            (Some(EntryType::Dir), None) => ItemStatus::LeftOnly,
            (None, Some(EntryType::Dir)) => ItemStatus::RightOnly,
            _ => file_status(
                fs,
                left_meta.as_ref(),
                right_meta.as_ref(),
                mode,
                &left_root,
                &right_root,
            )?,
        };
        let node = CompareNode {
            rel_path: name.clone(),
            name,
            ty,
            status,
            left: left_meta,
            right: right_meta,
            children: Vec::new(),
        };
        count(&node, &mut stats);
        nodes.push(node);
    }
    Ok(FolderCompare { tree: nodes, stats })
}

fn file_status(
    fs: &dyn FsOps,
    left: Option<&FileMeta>,
    right: Option<&FileMeta>,
    mode: CompareMode,
    left_root: &Location,
    right_root: &Location,
) -> Result<ItemStatus> {
    match (left, right) {
        (Some(_), None) => return Ok(ItemStatus::LeftOnly),
        (None, Some(_)) => return Ok(ItemStatus::RightOnly),
        (None, None) => return Ok(ItemStatus::Unknown),
        (Some(l), Some(r)) => {
            if l.ty != r.ty || l.size != r.size {
                return Ok(ItemStatus::Different);
            }
            if mode == CompareMode::Quick {
                return Ok(if l.mtime / 1000 != r.mtime / 1000 {
                    ItemStatus::Different
                } else {
                    ItemStatus::Same
                });
            }
            let left_loc = side_file(left_root, l);
            let right_loc = side_file(right_root, r);
            match (fs.hash_file(&left_loc), fs.hash_file(&right_loc)) {
                (Ok(lh), Ok(rh)) if lh == rh => Ok(ItemStatus::Same),
                _ => Ok(ItemStatus::Different),
            }
        }
    }
}

fn side_file(root: &Location, meta: &FileMeta) -> Location {
    Location {
        kind: root.kind,
        path: meta.path.clone(),
        connection_id: root.connection_id.clone(),
    }
}

pub fn compare_file(fs: &dyn FsOps, left: &Location, right: &Location) -> Result<FileCompare> {
    let l = read_side(fs, left)?;
    let r = read_side(fs, right)?;
    let too_large =
        matches!(l, SideRead::TooLarge { .. }) || matches!(r, SideRead::TooLarge { .. });
    let mut result = FileCompare {
        left_meta: meta_of(&l),
        right_meta: meta_of(&r),
        left_text: None,
        right_text: None,
        left_binary: too_large,
        right_binary: too_large,
        left_hash: None,
        right_hash: None,
        identical: false,
        too_large,
        left_encoding: "utf-8".into(),
        right_encoding: "utf-8".into(),
        left_newline: "\n".into(),
        right_newline: "\n".into(),
        left_trailing_nl: false,
        right_trailing_nl: false,
    };
    if too_large {
        if left.path_exists_for_hash(&l) {
            result.left_hash = fs.hash_file(left).ok();
        }
        if right.path_exists_for_hash(&r) {
            result.right_hash = fs.hash_file(right).ok();
        }
        result.identical = hashes_equal(&result.left_hash, &result.right_hash);
        return Ok(result);
    }
    if let SideRead::Data { bytes, .. } = &l {
        result.left_binary = is_probably_binary(bytes);
        result.left_hash = Some(hash_bytes(bytes));
        if !result.left_binary {
            let decoded = decode_text(bytes);
            result.left_encoding = decoded.encoding;
            result.left_newline = decoded.newline;
            result.left_trailing_nl = decoded.trailing_newline;
            result.left_text = Some(decoded.text);
        }
    }
    if let SideRead::Data { bytes, .. } = &r {
        result.right_binary = is_probably_binary(bytes);
        result.right_hash = Some(hash_bytes(bytes));
        if !result.right_binary {
            let decoded = decode_text(bytes);
            result.right_encoding = decoded.encoding;
            result.right_newline = decoded.newline;
            result.right_trailing_nl = decoded.trailing_newline;
            result.right_text = Some(decoded.text);
        }
    }
    result.identical = hashes_equal(&result.left_hash, &result.right_hash);
    Ok(result)
}

enum SideRead {
    Missing,
    TooLarge { meta: FileMeta },
    Data { meta: FileMeta, bytes: Vec<u8> },
}

fn meta_of(side: &SideRead) -> Option<FileMeta> {
    match side {
        SideRead::Missing => None,
        SideRead::TooLarge { meta } | SideRead::Data { meta, .. } => Some(meta.clone()),
    }
}

fn hashes_equal(left: &Option<String>, right: &Option<String>) -> bool {
    matches!((left, right), (Some(l), Some(r)) if l == r)
}

trait HashableSide {
    fn path_exists_for_hash(&self, side: &SideRead) -> bool;
}

impl HashableSide for Location {
    fn path_exists_for_hash(&self, side: &SideRead) -> bool {
        !self.path.is_empty() && !matches!(side, SideRead::Missing)
    }
}

fn read_side(fs: &dyn FsOps, loc: &Location) -> Result<SideRead> {
    if loc.path.is_empty() {
        return Ok(SideRead::Missing);
    }
    match fs.read_limited(loc, TEXT_MAX_BYTES)? {
        ReadOutcome::Missing => Ok(SideRead::Missing),
        ReadOutcome::TooLarge { size, meta } => {
            let mut meta = meta;
            if meta.size == 0 {
                meta.size = size;
            }
            Ok(SideRead::TooLarge { meta })
        }
        ReadOutcome::Data { bytes, meta } => Ok(SideRead::Data { meta, bytes }),
    }
}

fn rank(meta: Option<&FileMeta>) -> u8 {
    if meta.is_some_and(|m| m.ty == EntryType::Dir) {
        0
    } else {
        1
    }
}

fn entry_type(left: Option<&FileMeta>, right: Option<&FileMeta>) -> EntryType {
    left.or(right).map(|m| m.ty).unwrap_or(EntryType::File)
}

#[cfg(test)]
fn rollup(node: &mut CompareNode) -> ItemStatus {
    if node.ty == EntryType::File || node.children.is_empty() {
        return node.status;
    }
    let mut set = HashSet::new();
    for child in &mut node.children {
        let status = if child.ty == EntryType::Dir {
            rollup(child)
        } else {
            child.status
        };
        set.insert(status);
    }
    node.status = if set.contains(&ItemStatus::Different)
        || (set.contains(&ItemStatus::LeftOnly) && set.contains(&ItemStatus::RightOnly))
    {
        ItemStatus::Different
    } else if set.len() == 1 {
        set.into_iter().next().unwrap_or(ItemStatus::Unknown)
    } else if set.contains(&ItemStatus::LeftOnly) {
        ItemStatus::LeftOnly
    } else if set.contains(&ItemStatus::RightOnly) {
        ItemStatus::RightOnly
    } else {
        ItemStatus::Different
    };
    node.status
}

fn count(node: &CompareNode, stats: &mut CompareStats) {
    if node.ty == EntryType::Dir {
        stats.dirs += 1;
        return;
    }
    stats.files += 1;
    match node.status {
        ItemStatus::Same => stats.same += 1,
        ItemStatus::Different => stats.different += 1,
        ItemStatus::LeftOnly => stats.left_only += 1,
        ItemStatus::RightOnly => stats.right_only += 1,
        ItemStatus::Unknown => {}
    }
}

#[allow(dead_code)]
pub fn collect_sync(nodes: &[CompareNode], to_right: bool, out: &mut Vec<CompareNode>) {
    for node in nodes {
        let take = if to_right {
            matches!(node.status, ItemStatus::LeftOnly | ItemStatus::Different)
                && node.left.is_some()
        } else {
            matches!(node.status, ItemStatus::RightOnly | ItemStatus::Different)
                && node.right.is_some()
        };
        if take
            && (node.ty == EntryType::File
                || matches!(node.status, ItemStatus::LeftOnly | ItemStatus::RightOnly))
        {
            out.push(node.clone());
            continue;
        }
        collect_sync(&node.children, to_right, out);
    }
}

pub fn find_node<'a>(nodes: &'a [CompareNode], rel: &str) -> Option<&'a CompareNode> {
    for node in nodes {
        if node.rel_path == rel {
            return Some(node);
        }
        if let Some(found) = find_node(&node.children, rel) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::time::{Duration, SystemTime};

    #[test]
    fn content_mode_sees_same_size_edits_and_skips_ignored_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left");
        let right = dir.path().join("right");
        std::fs::create_dir_all(left.join("sub")).unwrap();
        std::fs::create_dir_all(right.join("sub")).unwrap();
        std::fs::create_dir_all(left.join("node_modules")).unwrap();
        write(&left.join("same.txt"), "hello");
        write(&right.join("same.txt"), "hello");
        write(&left.join("diff.txt"), "aaa");
        write(&right.join("diff.txt"), "bbb");
        write(&left.join("only.txt"), "x");
        write(&right.join("sub/nested.txt"), "n");
        write(&left.join("node_modules/skip.js"), "nope");
        let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        for path in [
            left.join("same.txt"),
            right.join("same.txt"),
            left.join("diff.txt"),
            right.join("diff.txt"),
        ] {
            File::open(path).unwrap().set_modified(stamp).unwrap();
        }

        let fs = crate::fsops::RealFs::new();
        let ignore = default_ignore();
        let cancel = AtomicBool::new(false);
        let mut notes = Vec::new();
        let quick = compare_folders(
            &fs,
            &Location::local(left.display().to_string()),
            &Location::local(right.display().to_string()),
            CompareMode::Quick,
            &ignore,
            &mut |_, path| notes.push(path.to_string()),
            &cancel,
        )
        .unwrap();
        let diff = find_node(&quick.tree, "diff.txt").unwrap();
        assert_eq!(diff.status, ItemStatus::Same);
        assert!(find_node(&quick.tree, "node_modules").is_none());
        assert_eq!(
            find_node(&quick.tree, "only.txt").unwrap().status,
            ItemStatus::LeftOnly
        );
        assert_eq!(
            find_node(&quick.tree, "sub").unwrap().status,
            ItemStatus::RightOnly
        );

        let content = compare_folders(
            &fs,
            &Location::local(left.display().to_string()),
            &Location::local(right.display().to_string()),
            CompareMode::Content,
            &ignore,
            &mut |_, _| {},
            &cancel,
        )
        .unwrap();
        assert_eq!(
            find_node(&content.tree, "diff.txt").unwrap().status,
            ItemStatus::Different
        );
        assert_eq!(
            find_node(&content.tree, "same.txt").unwrap().status,
            ItemStatus::Same
        );
        assert!(!notes.is_empty());
    }

    #[test]
    fn one_side_lists_only_the_current_folder() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left");
        std::fs::create_dir_all(left.join("sub")).unwrap();
        write(&left.join("only.txt"), "x");
        write(&left.join("sub/nested.txt"), "n");
        let fs = crate::fsops::RealFs::new();
        let cancel = AtomicBool::new(false);
        let listed = compare_level(
            &fs,
            Some(&Location::local(left.display().to_string())),
            None,
            CompareMode::Quick,
            &default_ignore(),
            &mut |_, _| {},
            &cancel,
        )
        .unwrap();
        assert!(find_node(&listed.tree, "nested.txt").is_none());
        assert_eq!(
            find_node(&listed.tree, "only.txt").unwrap().status,
            ItemStatus::LeftOnly
        );
        assert_eq!(
            find_node(&listed.tree, "sub").unwrap().status,
            ItemStatus::LeftOnly
        );
        assert!(find_node(&listed.tree, "sub").unwrap().children.is_empty());
    }

    fn write(path: &std::path::Path, text: &str) {
        let mut file = File::create(path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }
}
