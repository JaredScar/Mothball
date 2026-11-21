use std::path::Path;
use std::process::Stdio;

use crate::error::{Error, Result};
use crate::model::{CacheView, Settings};
use crate::util::hidden_command;

struct Spec {
    id: &'static str,
    name: &'static str,
    detail: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    dangerous: bool,
}

const SPECS: &[Spec] = &[
    Spec { id: "npm", name: "npm cache", detail: "Verifies and drops unneeded npm cache entries.", command: "npm", args: &["cache", "verify"], dangerous: false },
    Spec { id: "pnpm", name: "pnpm store", detail: "Removes packages that no project links anymore.", command: "pnpm", args: &["store", "prune"], dangerous: false },
    Spec { id: "yarn", name: "Yarn cache", detail: "Clears the Yarn download cache.", command: "yarn", args: &["cache", "clean"], dangerous: false },
    Spec { id: "nuget", name: "NuGet", detail: "Clears the local NuGet HTTP and global packages caches.", command: "dotnet", args: &["nuget", "locals", "all", "--clear"], dangerous: false },
    Spec { id: "gradle", name: "Gradle", detail: "Removes unused dependency caches under your home folder.", command: "gradle", args: &["--stop"], dangerous: false },
    Spec { id: "cargo", name: "Cargo", detail: "Runs cargo-cache --autoclean when that tool is installed.", command: "cargo", args: &["cache", "--autoclean"], dangerous: false },
    Spec { id: "pip", name: "pip", detail: "Purges the pip download cache.", command: "pip", args: &["cache", "purge"], dangerous: false },
    Spec { id: "uv", name: "uv", detail: "Prunes unused uv cache entries.", command: "uv", args: &["cache", "prune"], dangerous: false },
    Spec { id: "docker", name: "Docker", detail: "docker system prune. Off until you confirm it. This removes unused images and build cache.", command: "docker", args: &["system", "prune", "-f"], dangerous: true },
    Spec { id: "wsl", name: "Compact WSL disk", detail: "Marks WSL distros sparse so the next shutdown can return space to Windows. Deleting files inside WSL does not shrink the VHDX by itself.", command: "wsl", args: &["--manage"], dangerous: true },
];

pub fn ids() -> impl Iterator<Item = &'static str> {
    SPECS.iter().map(|spec| spec.id)
}

pub fn list(settings: &Settings) -> Vec<CacheView> {
    SPECS
        .iter()
        .map(|spec| CacheView {
            id: spec.id.to_string(),
            name: spec.name.to_string(),
            detail: spec.detail.to_string(),
            enabled: settings.cache_toggles.get(spec.id).copied().unwrap_or(false),
            available: program_exists(spec.command),
            command: format!("{} {}", spec.command, spec.args.join(" ")),
            dangerous: spec.dangerous,
        })
        .collect()
}

pub fn prune(id: &str, confirm: bool) -> Result<String> {
    let spec = SPECS.iter().find(|spec| spec.id == id).ok_or_else(|| Error::msg("Unknown cache."))?;
    if spec.dangerous && !confirm {
        return Err(Error::msg("This cache needs an explicit confirmation."));
    }
    if !program_exists(spec.command) {
        return Err(Error::msg(format!("{} is not on PATH.", spec.command)));
    }
    if spec.id == "wsl" {
        return compact_wsl();
    }
    if spec.id == "gradle" {
        return prune_gradle();
    }
    run(spec.command, spec.args, None)
}

fn prune_gradle() -> Result<String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from);
    let mut notes = Vec::new();
    if let Some(home) = home {
        let caches = home.join(".gradle").join("caches");
        if caches.is_dir() {
            notes.push(format!("Gradle caches live at {}", caches.display()));
        }
    }
    if program_exists("gradle") {
        notes.push(run("gradle", &["--stop"], None)?);
    }
    if notes.is_empty() {
        return Err(Error::msg("No Gradle cache was found."));
    }
    Ok(notes.join("\n"))
}

fn compact_wsl() -> Result<String> {
    let output = hidden_command("wsl")
        .arg("-l")
        .arg("-q")
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(Error::msg("wsl -l failed."));
    }
    let text = decode_wsl(&output.stdout);
    let mut notes = Vec::new();
    for line in text.lines() {
        let name = line.trim().trim_matches('\u{0}').trim();
        if name.is_empty() || name.starts_with("Windows Subsystem") {
            continue;
        }
        let name = name.trim_end_matches("(Default)").trim();
        let result = hidden_command("wsl")
            .args(["--manage", name, "--set-sparse", "true"])
            .output();
        match result {
            Ok(out) if out.status.success() => notes.push(format!("{name}: marked sparse")),
            Ok(out) => notes.push(format!(
                "{name}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )),
            Err(err) => notes.push(format!("{name}: {err}")),
        }
    }
    if notes.is_empty() {
        return Err(Error::msg("No WSL distros were listed."));
    }
    Ok(notes.join("\n"))
}

fn decode_wsl(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[1] == 0 {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn run(program: &str, args: &[&str], dir: Option<&Path>) -> Result<String> {
    let mut command = hidden_command(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let output = command.output()?;
    let mut text = String::new();
    let out = String::from_utf8_lossy(&output.stdout);
    let err = String::from_utf8_lossy(&output.stderr);
    if !out.trim().is_empty() {
        text.push_str(out.trim());
    }
    if !err.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(err.trim());
    }
    if !output.status.success() {
        return Err(Error::msg(if text.is_empty() {
            format!("{program} exited with {}", output.status)
        } else {
            text
        }));
    }
    if text.is_empty() {
        text = format!("{program} finished.");
    }
    Ok(text)
}

fn program_exists(name: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    #[cfg(windows)]
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into())
        .split(';')
        .map(|ext| ext.to_ascii_lowercase())
        .collect();
    for dir in std::env::split_paths(&path) {
        if dir.join(name).is_file() {
            return true;
        }
        #[cfg(windows)]
        for ext in &exts {
            if dir.join(format!("{name}{ext}")).is_file() {
                return true;
            }
        }
    }
    false
}
