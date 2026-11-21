use std::path::Path;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::util::{is_strict_child, path_key, paths_equal};

#[derive(Debug, Clone)]
struct Hit {
    path_key: String,
    reason: String,
    ide: bool,
}

#[derive(Debug, Clone, Default)]
pub struct OpenIndex {
    hits: Vec<Hit>,
}

impl OpenIndex {
    pub fn capture() -> Self {
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .without_tasks()
                .with_cmd(UpdateKind::Always)
                .with_cwd(UpdateKind::Always)
                .with_exe(UpdateKind::Always),
        );
        let mut hits = Vec::new();
        for process in system.processes().values() {
            let name = process.name().to_string_lossy().to_ascii_lowercase();
            let exe = process
                .exe()
                .map(|path| path.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let ide = is_ide(&name) || is_ide(&exe);
            let reason = if ide {
                format!("Open in {}", ide_label(&name, &exe))
            } else {
                "A running process is using it".to_string()
            };
            if let Some(cwd) = process.cwd() {
                let key = path_key(cwd);
                if !is_rootish(&key) {
                    hits.push(Hit {
                        path_key: key,
                        reason: reason.clone(),
                        ide: false,
                    });
                }
            }
            if ide {
                let cmd = process
                    .cmd()
                    .iter()
                    .map(|arg| arg.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                for path in path_like_args(&cmd) {
                    hits.push(Hit {
                        path_key: path_key(Path::new(&path)),
                        reason: reason.clone(),
                        ide: true,
                    });
                }
            }
        }
        Self { hits }
    }

    pub fn reason_for(&self, project: &Path) -> Option<String> {
        let project_key = path_key(project);
        for hit in &self.hits {
            if hit.path_key == project_key {
                return Some(hit.reason.clone());
            }
            if hit.ide && is_key_within(&hit.path_key, &project_key) {
                return Some(hit.reason.clone());
            }
            if is_key_within(&project_key, &hit.path_key) {
                return Some(hit.reason.clone());
            }
        }
        let _ = (paths_equal, is_strict_child);
        None
    }
}

fn is_key_within(parent: &str, child: &str) -> bool {
    if child.len() <= parent.len() {
        return false;
    }
    let sep = std::path::MAIN_SEPARATOR;
    child.starts_with(parent) && child[parent.len()..].starts_with(sep)
}

fn is_rootish(key: &str) -> bool {
    #[cfg(windows)]
    {
        key.len() <= 3
    }
    #[cfg(not(windows))]
    {
        key == "/" || key.is_empty()
    }
}

fn is_ide(name: &str) -> bool {
    const NAMES: &[&str] = &[
        "code", "code.exe", "cursor", "cursor.exe", "code - oss", "codium", "codium.exe",
        "idea", "idea64", "idea64.exe", "webstorm", "webstorm64", "webstorm64.exe", "pycharm64",
        "pycharm64.exe", "clion64", "clion64.exe", "rider64", "rider64.exe", "goland64",
        "goland64.exe", "rustrover64", "rustrover64.exe", "phpstorm64", "phpstorm64.exe",
        "devenv", "devenv.exe", "unity", "unity.exe", "studio64", "studio64.exe",
    ];
    let file = name.rsplit(['\\', '/']).next().unwrap_or(name);
    NAMES.iter().any(|candidate| file == *candidate || file.starts_with(&format!("{candidate}.")))
}

fn ide_label(name: &str, exe: &str) -> &'static str {
    let hay = format!("{name} {exe}");
    if hay.contains("cursor") {
        "Cursor"
    } else if hay.contains("code") || hay.contains("codium") {
        "VS Code"
    } else if hay.contains("devenv") || hay.contains("studio64") {
        "Visual Studio"
    } else if hay.contains("unity") {
        "Unity"
    } else if hay.contains("idea")
        || hay.contains("webstorm")
        || hay.contains("pycharm")
        || hay.contains("clion")
        || hay.contains("rider")
        || hay.contains("goland")
        || hay.contains("rustrover")
        || hay.contains("phpstorm")
    {
        "JetBrains"
    } else {
        "an editor"
    }
}

fn path_like_args(cmd: &str) -> Vec<String> {
    let mut found = Vec::new();
    for token in split_args(cmd) {
        let trimmed = token.trim_matches('"').trim();
        if looks_like_path(trimmed) {
            found.push(trimmed.to_string());
        }
    }
    found
}

fn looks_like_path(token: &str) -> bool {
    if token.len() < 3 {
        return false;
    }
    #[cfg(windows)]
    {
        let bytes = token.as_bytes();
        bytes.len() > 2 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/')
            || token.starts_with(r"\\")
    }
    #[cfg(not(windows))]
    {
        token.starts_with('/')
    }
}

fn split_args(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for ch in cmd.chars() {
        match (quote, ch) {
            (None, '"' | '\'') => quote = Some(ch),
            (Some(q), c) if c == q => quote = None,
            (None, ' ') => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            (_, c) => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
