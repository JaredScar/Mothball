use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util::system_time_secs;

#[derive(Debug, Default)]
pub struct GitInfo {
    pub index_mtime: Option<i64>,
    pub reflog_time: Option<i64>,
    pub dirty: bool,
}

pub fn inspect(project: &Path) -> GitInfo {
    let Some(git_dir) = git_dir(project) else {
        return GitInfo::default();
    };
    let index_mtime = fs::metadata(git_dir.join("index"))
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(system_time_secs);
    let reflog_time = newest_reflog(&git_dir.join("logs").join("HEAD"));
    GitInfo {
        index_mtime,
        reflog_time,
        dirty: dirty(project),
    }
}

fn git_dir(project: &Path) -> Option<PathBuf> {
    let dot = project.join(".git");
    let meta = fs::symlink_metadata(&dot).ok()?;
    if meta.is_dir() {
        return Some(dot);
    }
    if !meta.is_file() {
        return None;
    }
    let text = fs::read_to_string(&dot).ok()?;
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("gitdir:") else {
            continue;
        };
        let raw = PathBuf::from(rest.trim());
        if raw.is_absolute() {
            return Some(raw);
        }
        return Some(project.join(raw));
    }
    None
}

pub fn newest_reflog(path: &Path) -> Option<i64> {
    let text = fs::read_to_string(path).ok()?;
    let mut best = None;
    for line in text.lines() {
        if let Some(ts) = reflog_timestamp(line) {
            best = Some(best.map(|current: i64| current.max(ts)).unwrap_or(ts));
        }
    }
    best
}

fn reflog_timestamp(line: &str) -> Option<i64> {
    let head = line.split('\t').next().unwrap_or(line);
    let parts: Vec<&str> = head.split_whitespace().collect();
    for window in parts.windows(2) {
        let stamp = window[0];
        let zone = window[1];
        if stamp.len() >= 9
            && stamp.chars().all(|ch| ch.is_ascii_digit())
            && zone.len() == 5
            && (zone.starts_with('+') || zone.starts_with('-'))
            && zone[1..].chars().all(|ch| ch.is_ascii_digit())
        {
            return stamp.parse().ok();
        }
    }
    None
}

fn dirty(project: &Path) -> bool {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(project)
        .arg("--no-optional-locks")
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("GIT_OPTIONAL_LOCKS", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    match command.output() {
        Ok(output) if output.status.success() => {
            output.stdout.iter().any(|byte| !byte.is_ascii_whitespace())
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_reflog_timestamp() {
        let line = "0000000000000000000000000000000000000000 abcdefabcdefabcdefabcdefabcdefabcdefabcd Ada <ada@example.com> 1700000000 -0500\tcommit: init";
        assert_eq!(reflog_timestamp(line), Some(1_700_000_000));
    }
}
