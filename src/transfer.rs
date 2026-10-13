use crate::fsops::FsOps;
use crate::model::EntryType;
use crate::model::Location;
use crate::paths::file_name;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug)]
pub struct CopyItem {
    pub source: Location,
    pub dest_dir: Location,
    pub is_dir: bool,
}

pub fn copy_items(fs: &dyn FsOps, items: &[CopyItem], cancel: &AtomicBool) -> Result<usize> {
    for item in items {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!("已取消");
        }
        let name = file_name(&item.source.path);
        let dest = item.dest_dir.child(&name);
        copy_entry(fs, &item.source, &dest, item.is_dir, cancel)?;
    }
    Ok(items.len())
}

fn copy_entry(
    fs: &dyn FsOps,
    source: &Location,
    dest: &Location,
    is_dir: bool,
    cancel: &AtomicBool,
) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("已取消");
    }
    if is_dir {
        fs.mkdir_p(dest)?;
        for child in fs.list(source)? {
            let next_src = source.child(&child.name);
            let next_dest = dest.child(&child.name);
            copy_entry(
                fs,
                &next_src,
                &next_dest,
                child.ty == EntryType::Dir,
                cancel,
            )?;
        }
        Ok(())
    } else if let Some(parent) = dest.parent() {
        fs.mkdir_p(&parent)?;
        fs.copy_file(source, dest)
    } else {
        fs.copy_file(source, dest)
    }
}
