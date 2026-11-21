use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ignore::WalkBuilder;
use rayon::prelude::*;
use serde::Deserialize;

use crate::gitinfo;
use crate::guards::OpenIndex;
use crate::model::{FoundArtifact, FoundProject, Settings};
use crate::rules::{self, Rule, RuleSet};
use crate::size::allocated_tree_size_counted;
use crate::util::{now_secs, system_time_secs};

#[derive(Clone)]
pub struct DiscoverProgress {
    pub phase: String,
    pub root: String,
    pub current: String,
    pub completed: u32,
    pub total: u32,
    pub projects_found: u32,
    pub root_index: u32,
    pub root_count: u32,
    pub files_seen: u64,
}

const SKIP_DISCOVERY: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "node_modules",
    "appdata",
    "application data",
    "$recycle.bin",
    "system volume information",
    "windows",
    "winsxs",
    "program files",
    "program files (x86)",
    "programdata",
    "recovery",
    "perflogs",
];

#[derive(Debug, Default, Deserialize)]
struct MothFile {
    #[serde(default)]
    protect: bool,
    #[serde(default)]
    threshold_days: Option<u32>,
    #[serde(default)]
    extra: Vec<String>,
    #[serde(default)]
    ignore: Vec<String>,
}

pub fn discover_root(
    root: &Path,
    max_depth: u32,
    rules: &RuleSet,
    settings: &Settings,
    open: &OpenIndex,
    cancel: &AtomicBool,
    root_index: u32,
    root_count: u32,
    on_progress: Arc<dyn Fn(DiscoverProgress) + Send + Sync>,
) -> Vec<FoundProject> {
    let root_label = root.display().to_string();
    let files = Arc::new(AtomicU64::new(0));
    let latest = Arc::new(Mutex::new(DiscoverProgress {
        phase: "Looking".into(),
        root: root_label.clone(),
        current: root_label.clone(),
        completed: 0,
        total: 0,
        projects_found: 0,
        root_index,
        root_count,
        files_seen: 0,
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let heartbeat = {
        let latest = latest.clone();
        let files = files.clone();
        let on_progress = on_progress.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::park_timeout(Duration::from_millis(400));
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let tick = {
                    let Ok(mut tick) = latest.lock() else { continue };
                    tick.files_seen = files.load(Ordering::Relaxed);
                    tick.clone()
                };
                on_progress(tick);
            }
        })
    };
    let mut projects = Vec::new();
    let mut folders = 0u32;
    let mut stack = vec![(root.to_path_buf(), 0u32)];
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    while let Some((dir, depth)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if is_link(&dir) || is_placeholder(&dir) {
            continue;
        }
        folders += 1;
        let is_project = !rules.markers_in(&dir).is_empty();
        if is_project {
            projects.push(dir.clone());
        }
        let tick = progress_tick("Looking", &root_label, &dir.display().to_string(), folders, 0, projects.len() as u32, root_index, root_count, 0);
        if let Ok(mut slot) = latest.lock() {
            *slot = tick.clone();
        }
        if last_emit.elapsed() >= Duration::from_millis(180) {
            last_emit = Instant::now();
            on_progress(tick);
        }
        if is_project {
            continue;
        }
        if depth >= max_depth {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() || !kind.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if SKIP_DISCOVERY.contains(&name.as_str()) {
                continue;
            }
            stack.push((entry.path(), depth + 1));
        }
    }

    let total = projects.len() as u32;
    let mut analyzed = Vec::with_capacity(projects.len());
    for (index, path) in projects.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let current = path.display().to_string();
        let seen = files.load(Ordering::Relaxed);
        let tick = progress_tick("Measuring", &root_label, &current, index as u32, total, total, root_index, root_count, seen);
        if let Ok(mut slot) = latest.lock() {
            *slot = tick.clone();
        }
        on_progress(tick);
        let mut project = analyze(&path, rules, settings, open);
        measure_artifacts(&mut project, &files);
        analyzed.push(project);
    }
    if total > 0 {
        on_progress(progress_tick("Measuring", &root_label, &root_label, total, total, total, root_index, root_count, files.load(Ordering::Relaxed)));
    }
    stop.store(true, Ordering::Relaxed);
    heartbeat.thread().unpark();
    let _ = heartbeat.join();
    analyzed
}

fn progress_tick(
    phase: &str,
    root: &str,
    current: &str,
    completed: u32,
    total: u32,
    projects_found: u32,
    root_index: u32,
    root_count: u32,
    files_seen: u64,
) -> DiscoverProgress {
    DiscoverProgress {
        phase: phase.to_string(),
        root: root.to_string(),
        current: current.to_string(),
        completed,
        total,
        projects_found,
        root_index,
        root_count,
        files_seen,
    }
}

fn measure_artifacts(project: &mut FoundProject, files: &AtomicU64) {
    let root = project.path.clone();
    project.artifacts.par_iter_mut().for_each(|artifact| {
        let full = root.join(&artifact.rel_path);
        artifact.size_bytes = if artifact.is_link {
            files.fetch_add(1, Ordering::Relaxed);
            fs::symlink_metadata(&full).map(|meta| meta.len()).unwrap_or(0)
        } else {
            allocated_tree_size_counted(&full, Some(files))
        };
    });
}

fn analyze(path: &Path, rules: &RuleSet, settings: &Settings, open: &OpenIndex) -> FoundProject {
    let moth = if settings.respect_mothball_files {
        read_moth(&path.join(".mothball"))
    } else {
        MothFile::default()
    };
    let mut artifacts = Vec::new();
    let mut eco_ids = BTreeSet::new();
    let mut workspaces = Vec::new();
    let mut lockfiles: Vec<(String, String)> = Vec::new();
    walk(
        path,
        "",
        &[],
        rules,
        settings,
        &moth,
        &mut artifacts,
        &mut eco_ids,
        &mut workspaces,
        &mut lockfiles,
        0,
    );

    let mut hits = Vec::new();
    for id in &eco_ids {
        if let Some(rule) = rules.get(id) {
            let lock = lockfiles.iter().find(|(rule_id, _)| rule_id == id).map(|(_, file)| file.as_str());
            hits.push(rules::display_for(rule, lock));
        }
    }
    hits.sort_by(|a, b| a.label.cmp(&b.label));
    let ecosystem_label = if hits.is_empty() {
        "Unknown".to_string()
    } else {
        hits.iter().map(|hit| hit.label.clone()).collect::<Vec<_>>().join(" · ")
    };
    let lockfile = lockfiles.first().map(|(_, file)| file.clone());
    let touched = last_touched(path, rules);
    FoundProject {
        name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string()),
        path: path.to_path_buf(),
        ecosystems: hits,
        ecosystem_label,
        workspaces,
        artifacts,
        last_touched: touched.0,
        git_dirty: touched.1,
        protected: moth.protect,
        threshold_days: moth.threshold_days,
        lockfile,
        open_reason: if settings.skip_open_projects { open.reason_for(path) } else { None },
    }
}

fn walk(
    dir: &Path,
    rel: &str,
    applicable: &[String],
    rules: &RuleSet,
    settings: &Settings,
    moth: &MothFile,
    artifacts: &mut Vec<FoundArtifact>,
    eco_ids: &mut BTreeSet<String>,
    workspaces: &mut Vec<String>,
    lockfiles: &mut Vec<(String, String)>,
    depth: u32,
) {
    if depth > 48 {
        return;
    }
    let here = rules.markers_in(dir);
    if !rel.is_empty() && !here.is_empty() {
        workspaces.push(rel.to_string());
    }
    for id in &here {
        eco_ids.insert(id.clone());
        if let Some(rule) = rules.get(id) {
            if let Some(file) = find_lockfile(dir, rule) {
                if !lockfiles.iter().any(|(rule_id, name)| rule_id == id && name == &file) {
                    lockfiles.push((id.clone(), file));
                }
            }
        }
    }
    let mut next_rules = applicable.to_vec();
    for id in &here {
        if !next_rules.iter().any(|existing| existing == id) {
            next_rules.push(id.clone());
        }
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() && !kind.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".git" {
            continue;
        }
        let child_rel = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        let is_link = kind.is_symlink();
        if let Some(mut artifact) = match_artifact(&child_rel, &next_rules, rules, settings, moth, dir) {
            artifact.is_link = is_link;
            artifacts.push(artifact);
            continue;
        }
        if is_link || is_placeholder(&entry.path()) {
            continue;
        }
        walk(
            &entry.path(),
            &child_rel,
            &next_rules,
            rules,
            settings,
            moth,
            artifacts,
            eco_ids,
            workspaces,
            lockfiles,
            depth + 1,
        );
    }
}

fn match_artifact(
    rel: &str,
    applicable: &[String],
    rules: &RuleSet,
    settings: &Settings,
    moth: &MothFile,
    dir_parent: &Path,
) -> Option<FoundArtifact> {
    let ignored = moth.ignore.iter().any(|pattern| rules::pattern_match(rel, pattern));
    if moth.extra.iter().any(|pattern| rules::pattern_match(rel, pattern)) {
        return Some(FoundArtifact {
            rel_path: rel.to_string(),
            rule_id: "custom".into(),
            pattern: rel.to_string(),
            size_bytes: 0,
            kind: "extra".into(),
            ignored,
            lock_ok: true,
            is_link: false,
        });
    }

    let mut opt_in_hit = None;
    for rule_id in applicable {
        let Some(rule) = rules.get(rule_id) else { continue };
        let enabled = settings
            .ecosystems
            .get(rule_id)
            .map(|setting| setting.enabled)
            .unwrap_or(rule.enabled_by_default);
        if !enabled {
            continue;
        }
        for pattern in &rule.safe_artifacts {
            if rules::pattern_match(rel, pattern) {
                return Some(make_artifact(rel, rule, pattern, "safe", ignored, dir_parent));
            }
        }
        for pattern in &rule.opt_in_artifacts {
            if rules::pattern_match(rel, pattern) && opt_in_hit.is_none() {
                opt_in_hit = Some(make_artifact(rel, rule, pattern, "opt_in", ignored, dir_parent));
            }
        }
    }
    opt_in_hit
}

fn make_artifact(
    rel: &str,
    rule: &Rule,
    pattern: &str,
    kind: &str,
    ignored: bool,
    parent: &Path,
) -> FoundArtifact {
    FoundArtifact {
        rel_path: rel.to_string(),
        rule_id: rule.id.clone(),
        pattern: pattern.to_string(),
        size_bytes: 0,
        kind: kind.to_string(),
        ignored,
        lock_ok: lock_ok(parent, rel, rule),
        is_link: false,
    }
}

fn rel_parts(rel: &str) -> Vec<&str> {
    rel.split(['/', '\\']).filter(|part| !part.is_empty()).collect()
}

fn lock_ok(artifact_parent: &Path, rel: &str, rule: &Rule) -> bool {
    if !rule.lockfile_required || rule.lockfiles.is_empty() {
        return true;
    }
    let parts = rel_parts(rel);
    let mut project = artifact_parent.to_path_buf();
    // `artifact_parent` already contains every component except the artifact itself.
    for _ in 1..parts.len() {
        project.pop();
    }
    let mut dir = artifact_parent.to_path_buf();
    let project_key = crate::util::path_key(&project);
    loop {
        if find_lockfile(&dir, rule).is_some() {
            return true;
        }
        if crate::util::path_key(&dir) == project_key || !dir.pop() {
            break;
        }
    }
    false
}

fn find_lockfile(dir: &Path, rule: &Rule) -> Option<String> {
    for lock in &rule.lockfiles {
        let candidate = dir.join(lock);
        if fs::symlink_metadata(&candidate).map(|meta| meta.is_file()).unwrap_or(false) {
            return Some(lock.clone());
        }
    }
    None
}

fn last_touched(project: &Path, rules: &RuleSet) -> (Option<i64>, bool) {
    let mut newest = 0i64;
    let mut seen = false;
    let mut files = 0u32;
    let artifacts = artifact_dir_names(rules);
    let mut builder = WalkBuilder::new(project);
    builder
        .follow_links(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .hidden(false)
        .parents(false)
        .filter_entry(move |entry| {
            if entry.depth() == 0 {
                return true;
            }
            if entry.file_type().map(|kind| kind.is_symlink()).unwrap_or(false) {
                return false;
            }
            let name = entry.file_name().to_string_lossy();
            !is_heavy(&name) && !artifacts.contains(name.as_ref())
        });
    for entry in builder.build().flatten() {
        if !entry.file_type().map(|kind| kind.is_file()).unwrap_or(false) {
            continue;
        }
        files += 1;
        if files > 200_000 {
            break;
        }
        if files > 20_000 && files % 25 != 0 {
            continue;
        }
        if let Some(secs) = entry.metadata().ok().and_then(|meta| meta.modified().ok()).and_then(system_time_secs) {
            seen = true;
            newest = newest.max(secs);
        }
    }
    let git = gitinfo::inspect(project);
    if let Some(ts) = git.index_mtime {
        seen = true;
        newest = newest.max(ts);
    }
    if let Some(ts) = git.reflog_time {
        seen = true;
        newest = newest.max(ts);
    }
    (if seen { Some(newest) } else { None }, git.dirty)
}

fn artifact_dir_names(rules: &RuleSet) -> BTreeSet<String> {
    rules
        .iter()
        .flat_map(|rule| rule.safe_artifacts.iter().chain(rule.opt_in_artifacts.iter()))
        .filter_map(|pattern| {
            let name = pattern.rsplit(['/', '\\']).next().unwrap_or(pattern);
            if name.is_empty() || name.contains('*') {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}

fn is_heavy(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | "Pods"
            | ".gradle"
            | ".dart_tool"
            | "Library"
            | "__pycache__"
            | ".next"
            | ".nuxt"
            | ".turbo"
            | ".vite"
            | ".parcel-cache"
            | ".expo"
            | ".tox"
            | ".pytest_cache"
            | ".mypy_cache"
            | ".ruff_cache"
            | ".terraform"
            | ".svelte-kit"
            | "coverage"
            | ".angular"
            | "vendor"
            | ".venv"
            | "venv"
    )
}

fn read_moth(path: &Path) -> MothFile {
    let Ok(text) = fs::read_to_string(path) else {
        return MothFile::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).map(|meta| meta.file_type().is_symlink()).unwrap_or(false)
}

fn is_placeholder(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        const RECALL_ON_OPEN: u32 = 0x0004_0000;
        const OFFLINE: u32 = 0x0000_1000;
        if let Ok(meta) = fs::symlink_metadata(path) {
            let attrs = meta.file_attributes();
            return attrs & (RECALL_ON_DATA_ACCESS | RECALL_ON_OPEN | OFFLINE) != 0;
        }
    }
    let _ = path;
    false
}

pub fn touch_stamp() -> i64 {
    now_secs()
}
