use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::model::DeleteMode;
use crate::util::extended_path;

pub fn remove_artifact(path: &Path, mode: DeleteMode) -> io::Result<()> {
    remove_artifact_tracked(path, mode, &AtomicU64::new(0), &AtomicBool::new(false))
}

pub fn remove_artifact_tracked(path: &Path, mode: DeleteMode, files: &AtomicU64, cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(cancelled());
    }
    let (path, meta) = metadata_of(path)?;
    if is_reparse(&meta) {
        unlink_only(&path)?;
        files.fetch_add(1, Ordering::Relaxed);
        return Ok(());
    }
    match mode {
        DeleteMode::Trash => {
            trash::delete(&path).map_err(|err| io::Error::other(err.to_string()))?;
            files.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        DeleteMode::Permanent if meta.is_file() => remove_file_retry(&path, files),
        DeleteMode::Permanent if meta.is_dir() => remove_dir_progress(&path, files, cancel, 0),
        DeleteMode::Permanent => remove_file_retry(&path, files),
    }
}

fn remove_dir_progress(path: &Path, files: &AtomicU64, cancel: &AtomicBool, depth: u32) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(cancelled());
    }
    if depth > 48 {
        remove_dir_all::remove_dir_all(path)?;
        files.fetch_add(1, Ordering::Relaxed);
        return Ok(());
    }
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => {
            remove_dir_all::remove_dir_all(path)?;
            files.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
    };
    let mut first_error = None;
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        let Ok(entry) = entry else { continue };
        let child = entry.path();
        if let Err(err) = remove_one(&child, files, cancel, depth + 1) {
            if err.kind() == io::ErrorKind::Interrupted {
                return Err(err);
            }
            if first_error.is_none() {
                first_error = Some(err);
            }
        }
    }
    let _ = clear_readonly(path);
    match fs::remove_dir(path) {
        Ok(()) => {
            files.fetch_add(1, Ordering::Relaxed);
        }
        Err(err) => {
            if first_error.is_none() {
                first_error = Some(err);
            }
        }
    }
    match first_error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn remove_one(path: &Path, files: &AtomicU64, cancel: &AtomicBool, depth: u32) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(cancelled());
    }
    let (path, meta) = metadata_of(path)?;
    if is_reparse(&meta) {
        unlink_only(&path)?;
        files.fetch_add(1, Ordering::Relaxed);
        return Ok(());
    }
    if meta.is_dir() {
        remove_dir_progress(&path, files, cancel, depth)
    } else {
        remove_file_retry(&path, files)
    }
}

fn remove_file_retry(path: &Path, files: &AtomicU64) -> io::Result<()> {
    let _ = clear_readonly(path);
    match fs::remove_file(path) {
        Ok(()) => {
            files.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
        Err(err) => {
            let _ = clear_readonly(path);
            match fs::remove_file(path) {
                Ok(()) => {
                    files.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
                Err(_) => Err(err),
            }
        }
    }
}

fn metadata_of(path: &Path) -> io::Result<(PathBuf, fs::Metadata)> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        return Ok((path.to_path_buf(), meta));
    }
    let extended = extended_path(path);
    let meta = fs::symlink_metadata(&extended)?;
    Ok((extended, meta))
}

fn is_reparse(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        return meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

fn unlink_only(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(first) => fs::remove_dir(path).or(Err(first)),
    }
}

fn clear_readonly(path: &Path) -> io::Result<()> {
    let mut perms = fs::symlink_metadata(path)?.permissions();
    if perms.readonly() {
        perms.set_readonly(false);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

pub fn in_use(err: &io::Error) -> bool {
    if err.kind() == io::ErrorKind::PermissionDenied {
        return true;
    }
    let text = err.to_string().to_ascii_lowercase();
    text.contains("being used")
        || text.contains("access is denied")
        || text.contains("in use")
        || text.contains("error 32")
        || text.contains("sharing violation")
}
