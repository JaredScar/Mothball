use std::path::Path;

use sysinfo::Disks;

use crate::model::{DiskView, HostConditions, RootConfig};
use crate::util::{format_gb_int, path_key};

pub fn conditions() -> HostConditions {
    let idle = idle_seconds();
    HostConditions {
        idle_seconds: idle.unwrap_or(u64::MAX),
        idle_known: idle.is_some(),
        on_ac: on_ac_power().unwrap_or(true),
        fullscreen: foreground_is_fullscreen().unwrap_or(false),
    }
}

pub fn disk_for(roots: &[RootConfig], reclaimable: u64) -> DiskView {
    let disks = Disks::new_with_refreshed_list();
    let preferred = roots.iter().find(|root| root.enabled).map(|root| path_key(Path::new(&root.path)));
    let mut best: Option<(u64, String, String, u64, u64)> = None;
    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy().to_string();
        let total = disk.total_space();
        let free = disk.available_space();
        if total == 0 {
            continue;
        }
        let mount_key = path_key(Path::new(&mount));
        let matches = preferred.as_ref().map(|root| mount_holds(&mount_key, root)).unwrap_or(false);
        let score = if matches { u64::MAX - 1 } else { total };
        if best.as_ref().map(|(score_best, ..)| score > *score_best).unwrap_or(true) {
            let name = disk_name(&mount, &disk.name().to_string_lossy());
            best = Some((score, name, mount, free, total));
        }
    }
    let Some((_, name, mount, free, total)) = best else {
        return empty_disk();
    };
    view(name, mount, free, total, reclaimable)
}

fn mount_holds(mount_key: &str, path: &str) -> bool {
    if mount_key.is_empty() {
        return false;
    }
    if path == mount_key {
        return true;
    }
    let sep = std::path::MAIN_SEPARATOR;
    path.starts_with(mount_key) && path[mount_key.len()..].starts_with(sep)
}

fn disk_name(mount: &str, volume: &str) -> String {
    let letter = mount.trim_end_matches(['\\', '/']);
    let volume = volume.trim();
    if !volume.is_empty() && !volume.eq_ignore_ascii_case(letter) {
        format!("{volume} ({letter})")
    } else if cfg!(windows) {
        format!("Local Disk ({letter})")
    } else {
        letter.to_string()
    }
}

fn view(name: String, mount: String, free: u64, total: u64, reclaimable: u64) -> DiskView {
    let used = total.saturating_sub(free);
    let reclaimable = reclaimable.min(used);
    let total_f = total.max(1) as f64;
    DiskView {
        name,
        mount,
        free_bytes: free,
        total_bytes: total,
        free_label: format_gb_int(free),
        total_label: format_gb_int(total),
        used_ratio: (used.saturating_sub(reclaimable) as f64) / total_f,
        reclaimable_ratio: (reclaimable as f64) / total_f,
        free_percent: (free as f64) / total_f * 100.0,
    }
}

fn empty_disk() -> DiskView {
    DiskView {
        name: "Disk".into(),
        mount: String::new(),
        free_bytes: 0,
        total_bytes: 0,
        free_label: "0 B".into(),
        total_label: "0 B".into(),
        used_ratio: 0.0,
        reclaimable_ratio: 0.0,
        free_percent: 100.0,
    }
}

fn idle_seconds() -> Option<u64> {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct LastInputInfo {
            cb_size: u32,
            dw_time: u32,
        }
        #[link(name = "user32")]
        extern "system" {
            fn GetLastInputInfo(info: *mut LastInputInfo) -> i32;
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetTickCount() -> u32;
        }
        let mut info = LastInputInfo {
            cb_size: std::mem::size_of::<LastInputInfo>() as u32,
            dw_time: 0,
        };
        if unsafe { GetLastInputInfo(&mut info) } == 0 {
            return None;
        }
        let idle_ms = unsafe { GetTickCount() }.wrapping_sub(info.dw_time);
        return Some(u64::from(idle_ms) / 1000);
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn on_ac_power() -> Option<bool> {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct PowerStatus {
            ac_line_status: u8,
            battery_flag: u8,
            battery_life_percent: u8,
            system_status_flag: u8,
            battery_life_time: u32,
            battery_full_life_time: u32,
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetSystemPowerStatus(status: *mut PowerStatus) -> i32;
        }
        let mut status = PowerStatus {
            ac_line_status: 255,
            battery_flag: 0,
            battery_life_percent: 255,
            system_status_flag: 0,
            battery_life_time: 0,
            battery_full_life_time: 0,
        };
        if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
            return None;
        }
        return match status.ac_line_status {
            1 => Some(true),
            0 => Some(false),
            _ => None,
        };
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn foreground_is_fullscreen() -> Option<bool> {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct Rect {
            left: i32,
            top: i32,
            right: i32,
            bottom: i32,
        }
        #[repr(C)]
        struct MonitorInfo {
            cb_size: u32,
            monitor: Rect,
            work: Rect,
            flags: u32,
        }
        #[link(name = "user32")]
        extern "system" {
            fn GetForegroundWindow() -> isize;
            fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
            fn MonitorFromWindow(hwnd: isize, flags: u32) -> isize;
            fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfo) -> i32;
        }
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd == 0 {
            return None;
        }
        let mut window = Rect { left: 0, top: 0, right: 0, bottom: 0 };
        if unsafe { GetWindowRect(hwnd, &mut window) } == 0 {
            return None;
        }
        let monitor = unsafe { MonitorFromWindow(hwnd, 1) };
        if monitor == 0 {
            return None;
        }
        let mut info = MonitorInfo {
            cb_size: std::mem::size_of::<MonitorInfo>() as u32,
            monitor: Rect { left: 0, top: 0, right: 0, bottom: 0 },
            work: Rect { left: 0, top: 0, right: 0, bottom: 0 },
            flags: 0,
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return None;
        }
        let covers = window.left <= info.monitor.left + 2
            && window.top <= info.monitor.top + 2
            && window.right >= info.monitor.right - 2
            && window.bottom >= info.monitor.bottom - 2;
        return Some(covers);
    }
    #[cfg(not(windows))]
    {
        None
    }
}
