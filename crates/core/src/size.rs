use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use rayon::prelude::*;

pub fn allocated_tree_size(path: &Path) -> u64 {
    allocated_tree_size_counted(path, None)
}

pub fn allocated_tree_size_counted(path: &Path, files: Option<&AtomicU64>) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if let Some(files) = files {
        files.fetch_add(1, Ordering::Relaxed);
    }
    if meta.file_type().is_symlink() {
        return meta.len();
    }
    if meta.is_file() {
        return file_allocated(&meta, path);
    }
    if !meta.is_dir() {
        return 0;
    }
    let mut total = file_allocated(&meta, path);
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries.filter_map(|entry| entry.ok()).map(|entry| entry.path()).collect::<Vec<_>>(),
        Err(_) => return total,
    };
    if entries.len() > 24 {
        total += entries.par_iter().map(|entry| allocated_tree_size_counted(entry, files)).sum::<u64>();
    } else {
        for entry in entries {
            total += allocated_tree_size_counted(&entry, files);
        }
    }
    total
}

fn file_allocated(meta: &fs::Metadata, path: &Path) -> u64 {
    #[cfg(windows)]
    {
        if let Some(size) = windows_allocated(path) {
            return size;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let blocks = meta.blocks();
        if blocks > 0 {
            return blocks.saturating_mul(512);
        }
    }
    let _ = meta;
    meta_len(meta)
}

fn meta_len(meta: &fs::Metadata) -> u64 {
    meta.len()
}

#[cfg(windows)]
fn windows_allocated(path: &Path) -> Option<u64> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const FILE_SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_STANDARD_INFO: i32 = 1;

    #[repr(C)]
    struct StandardInfo {
        allocation_size: i64,
        end_of_file: i64,
        number_of_links: u32,
        delete_pending: u8,
        directory: u8,
        _pad: [u8; 2],
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut c_void,
            disposition: u32,
            flags: u32,
            template: *mut c_void,
        ) -> *mut c_void;
        fn GetFileInformationByHandleEx(
            file: *mut c_void,
            class: i32,
            info: *mut c_void,
            size: u32,
        ) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    let extended = crate::util::extended_path(path);
    let mut wide: Vec<u16> = extended.as_os_str().encode_wide().collect();
    wide.push(0);
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_ALL,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle.is_null() || handle == (-1isize as *mut c_void) {
        return None;
    }
    let mut info = StandardInfo {
        allocation_size: 0,
        end_of_file: 0,
        number_of_links: 0,
        delete_pending: 0,
        directory: 0,
        _pad: [0, 0],
    };
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FILE_STANDARD_INFO,
            &mut info as *mut StandardInfo as *mut c_void,
            std::mem::size_of::<StandardInfo>() as u32,
        )
    };
    unsafe {
        CloseHandle(handle);
    }
    if ok == 0 {
        None
    } else {
        Some(info.allocation_size.max(0) as u64)
    }
}
