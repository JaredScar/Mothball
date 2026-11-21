use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::db::Database;
use crate::delete::{self, in_use};
use crate::error::Result;
use crate::model::{
    DeleteMode, ExcludeItem, PreviewGroup, PreviewItem, Settings, StoredProject, SweepOutcome, SweepPreview, SweepProgress, SweepQuery,
};
use crate::policy::{self, Choice};
use crate::rules::{self, RuleSet};
use crate::util::{age_short, format_bytes, now_secs, path_key, safe_join};

#[derive(Clone)]
struct Planned {
    project_path: String,
    project_name: String,
    rel_path: String,
    pattern: String,
    size: u64,
    age: String,
    lockfile: Option<String>,
    lock_ok: bool,
    checked: bool,
}

pub fn preview(
    projects: &[StoredProject],
    settings: &Settings,
    rules: &RuleSet,
    query: &SweepQuery,
    excluded: &HashSet<(String, String)>,
    now: i64,
) -> SweepPreview {
    let planned = plan(projects, settings, rules, query, excluded, now);
    build_preview(planned, settings, query)
}

pub fn run<F: FnMut(SweepProgress) + Send + 'static>(
    db: &Database,
    projects: &[StoredProject],
    settings: &Settings,
    rules: &RuleSet,
    query: &SweepQuery,
    excluded: &HashSet<(String, String)>,
    cancel: &AtomicBool,
    progress: F,
) -> Result<SweepOutcome> {
    let planned: Vec<Planned> = plan(projects, settings, rules, query, excluded, now_secs())
        .into_iter()
        .filter(|item| item.checked)
        .collect();
    let mode = query.delete_mode.unwrap_or(settings.delete_mode);
    let total = planned.len() as u32;
    let bytes_total = planned.iter().map(|item| item.size).sum();
    let mut bytes = 0u64;
    let mut errors = 0u32;
    let mut skipped = 0u32;
    let mut in_use_count = 0u32;
    let mut cleaned_projects = HashSet::new();
    let mut items = Vec::new();
    let started = now_secs();
    let files = Arc::new(AtomicU64::new(0));
    let latest = Arc::new(Mutex::new(empty_progress(bytes_total)));
    let callback = Arc::new(Mutex::new(progress));
    let stop = Arc::new(AtomicBool::new(false));
    let heartbeat = {
        let latest = latest.clone();
        let files = files.clone();
        let callback = callback.clone();
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::park_timeout(Duration::from_millis(400));
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let tick = {
                    let Ok(mut tick) = latest.lock() else { continue };
                    tick.files_removed = files.load(Ordering::Relaxed);
                    tick.clone()
                };
                if let Ok(mut progress) = callback.lock() {
                    progress(tick);
                }
            }
        })
    };
    let publish = |tick: SweepProgress| {
        if let Ok(mut slot) = latest.lock() {
            *slot = tick.clone();
        }
        if let Ok(mut progress) = callback.lock() {
            progress(tick);
        }
    };

    for (index, item) in planned.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let full_path = Path::new(&item.project_path).join(&item.rel_path);
        publish(SweepProgress {
            project: item.project_name.clone(),
            path: full_path.display().to_string(),
            rel_path: item.rel_path.clone(),
            index: index as u32 + 1,
            total,
            bytes_freed: bytes,
            bytes_total,
            files_removed: files.load(Ordering::Relaxed),
        });
        if query.dry_run {
            bytes += item.size;
            cleaned_projects.insert(item.project_path.clone());
            continue;
        }
        let Some(full) = safe_join(Path::new(&item.project_path), &item.rel_path) else {
            errors += 1;
            items.push((
                item.project_path.clone(),
                item.project_name.clone(),
                item.rel_path.clone(),
                item.size,
                "refused".into(),
                Some("Path escaped the project".into()),
            ));
            continue;
        };
        if !full.exists() {
            let _ = db.zero_artifact(&item.project_path, &item.rel_path);
            continue;
        }
        match delete::remove_artifact_tracked(&full, mode, &files, cancel) {
            Ok(()) => {
                bytes += item.size;
                cleaned_projects.insert(item.project_path.clone());
                let _ = db.zero_artifact(&item.project_path, &item.rel_path);
                items.push((
                    item.project_path.clone(),
                    item.project_name.clone(),
                    item.rel_path.clone(),
                    item.size,
                    "deleted".into(),
                    None,
                ));
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => break,
            Err(err) => {
                errors += 1;
                if in_use(&err) {
                    in_use_count += 1;
                    skipped += 1;
                }
                items.push((
                    item.project_path.clone(),
                    item.project_name.clone(),
                    item.rel_path.clone(),
                    item.size,
                    "error".into(),
                    Some(err.to_string()),
                ));
            }
        }
    }
    stop.store(true, Ordering::Relaxed);
    heartbeat.thread().unpark();
    let _ = heartbeat.join();

    let open_skips = count_open_skips(projects, settings, query, now_secs());
    skipped += open_skips;
    let skipped_summary = skip_summary(open_skips, in_use_count);
    let projects_cleaned = cleaned_projects.len() as u32;
    let summary = project_summary(&planned, &cleaned_projects);
    if !query.dry_run {
        db.insert_sweep(
            started,
            now_secs(),
            if query.trigger.is_empty() { "manual" } else { &query.trigger },
            bytes,
            projects_cleaned,
            errors,
            skipped,
            &summary,
            &skipped_summary,
            &items,
        )?;
    }
    let bytes_label = format_bytes(bytes);
    let message = if query.dry_run {
        format!("Would free {bytes_label} from {projects_cleaned} projects")
    } else if projects_cleaned == 0 && errors == 0 {
        "Nothing to clean".to_string()
    } else {
        format!("Freed {bytes_label} from {projects_cleaned} projects")
    };
    Ok(SweepOutcome {
        dry_run: query.dry_run,
        bytes_freed: bytes,
        bytes_label,
        projects_cleaned,
        errors,
        skipped,
        skipped_summary,
        message,
    })
}

fn plan(
    projects: &[StoredProject],
    settings: &Settings,
    rules: &RuleSet,
    query: &SweepQuery,
    excluded: &HashSet<(String, String)>,
    now: i64,
) -> Vec<Planned> {
    let mut planned = Vec::new();
    for project in projects {
        if let Some(only) = &query.project_path {
            if path_key(Path::new(only)) != path_key(Path::new(&project.path)) {
                continue;
            }
        }
        if project.protected || !policy::is_stale(project, settings, now, query.older_than_days) {
            continue;
        }
        if settings.skip_open_projects && project.open_reason.is_some() {
            continue;
        }
        let decided = policy::choices(project, settings);
        let age = project
            .last_touched
            .map(|ts| age_short(ts, now))
            .unwrap_or_else(|| "unknown".into());
        for choice in decided {
            if !allowed(&choice, rules) {
                continue;
            }
            let include = if choice.lock_blocked {
                query.include_unsafe
            } else {
                choice.deletable
            };
            if !include {
                continue;
            }
            let checked = !is_excluded(excluded, &project.path, &choice.artifact.rel_path);
            planned.push(Planned {
                project_path: project.path.clone(),
                project_name: project.name.clone(),
                rel_path: choice.artifact.rel_path.clone(),
                pattern: choice.artifact.pattern.clone(),
                size: choice.artifact.size_bytes,
                age: age.clone(),
                lockfile: project.lockfile.clone(),
                lock_ok: choice.artifact.lock_ok,
                checked,
            });
        }
    }
    planned
}

fn allowed(choice: &Choice, rules: &RuleSet) -> bool {
    if choice.artifact.rel_path.contains("..") {
        return false;
    }
    if choice.artifact.kind == "extra" && choice.artifact.rule_id == "custom" {
        return rules::pattern_match(&choice.artifact.rel_path, &choice.artifact.pattern);
    }
    let Some(rule) = rules.get(&choice.artifact.rule_id) else {
        return false;
    };
    rule.safe_artifacts
        .iter()
        .chain(rule.opt_in_artifacts.iter())
        .any(|pattern| pattern == &choice.artifact.pattern && rules::pattern_match(&choice.artifact.rel_path, pattern))
}

fn build_preview(planned: Vec<Planned>, settings: &Settings, query: &SweepQuery) -> SweepPreview {
    let mut order: Vec<String> = Vec::new();
    let mut groups: BTreeMap<String, (Planned, BTreeMap<String, (u64, Vec<String>, bool)>)> = BTreeMap::new();
    for item in planned {
        if !order.iter().any(|path| path == &item.project_path) {
            order.push(item.project_path.clone());
        }
        let entry = groups.entry(item.project_path.clone()).or_insert_with(|| {
            (
                item.clone(),
                BTreeMap::new(),
            )
        });
        let slot = entry.1.entry(item.pattern.clone()).or_insert((0, Vec::new(), true));
        slot.0 += item.size;
        slot.2 = slot.2 && item.checked;
        slot.1.push(item.rel_path);
    }
    let mut views = Vec::new();
    for path in order {
        let Some((sample, patterns)) = groups.remove(&path) else { continue };
        let mut items = Vec::new();
        for (pattern, (size, rels, checked)) in patterns {
            let count = rels.len();
            let label = if pattern == "node_modules" && count > 1 {
                format!("node_modules ({count} workspaces)")
            } else if count > 1 {
                format!("{pattern} ×{count}")
            } else {
                pattern.clone()
            };
            items.push(PreviewItem {
                label,
                pattern,
                rel_paths: rels,
                size_bytes: size,
                size_label: format_bytes(size),
                checked,
            });
        }
        items.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        views.push(PreviewGroup {
            name: sample.project_name,
            path: sample.project_path,
            age_label: sample.age,
            lockfile: sample.lockfile,
            lock_ok: sample.lock_ok,
            items,
        });
    }
    views.sort_by(|a, b| {
        let left: u64 = a.items.iter().map(|item| item.size_bytes).sum();
        let right: u64 = b.items.iter().map(|item| item.size_bytes).sum();
        right.cmp(&left)
    });
    let selected_bytes = views.iter().flat_map(|group| &group.items).filter(|item| item.checked).map(|item| item.size_bytes).sum();
    let total_bytes = views.iter().flat_map(|group| &group.items).map(|item| item.size_bytes).sum();
    let folder_count = views.iter().map(|group| group.items.len() as u32).sum();
    let selected_folders = views.iter().flat_map(|group| &group.items).filter(|item| item.checked).count() as u32;
    SweepPreview {
        threshold_days: query.older_than_days.unwrap_or(settings.threshold_days),
        delete_mode: query.delete_mode.unwrap_or(settings.delete_mode),
        groups: views,
        total_bytes,
        selected_bytes,
        folder_count,
        selected_folders,
    }
}

fn empty_progress(bytes_total: u64) -> SweepProgress {
    SweepProgress {
        project: String::new(),
        path: String::new(),
        rel_path: String::new(),
        index: 0,
        total: 0,
        bytes_freed: 0,
        bytes_total,
        files_removed: 0,
    }
}

fn is_excluded(excluded: &HashSet<(String, String)>, project: &str, rel: &str) -> bool {
    let key = path_key(Path::new(project));
    let rel = rel.replace('\\', "/");
    excluded.iter().any(|(path, item)| path_key(Path::new(path)) == key && item.replace('\\', "/") == rel)
}

pub fn exclusion_set(saved: &[(String, String)], extra: &[ExcludeItem]) -> HashSet<(String, String)> {
    let mut set = HashSet::new();
    for (path, rel) in saved {
        set.insert((path.clone(), rel.replace('\\', "/")));
    }
    for item in extra {
        set.insert((item.path.clone(), item.rel_path.replace('\\', "/")));
    }
    set
}

fn count_open_skips(projects: &[StoredProject], settings: &Settings, query: &SweepQuery, now: i64) -> u32 {
    if !settings.skip_open_projects {
        return 0;
    }
    projects
        .iter()
        .filter(|project| {
            query.project_path.as_ref().map(|only| path_key(Path::new(only)) == path_key(Path::new(&project.path))).unwrap_or(true)
                && !project.protected
                && policy::is_stale(project, settings, now, query.older_than_days)
                && project.open_reason.is_some()
                && policy::deletable_bytes(&policy::choices(project, settings)) > 0
        })
        .count() as u32
}

fn skip_summary(open_skips: u32, in_use_count: u32) -> String {
    let mut parts = Vec::new();
    if open_skips > 0 {
        parts.push(if open_skips == 1 {
            "1 open in an editor".to_string()
        } else {
            format!("{open_skips} open in an editor")
        });
    }
    if in_use_count > 0 {
        parts.push(if in_use_count == 1 {
            "1 folder in use, retried".to_string()
        } else {
            format!("{in_use_count} folders in use, retried")
        });
    }
    if parts.is_empty() {
        "None".to_string()
    } else {
        parts.join(" · ")
    }
}

fn project_summary(planned: &[Planned], cleaned: &HashSet<String>) -> String {
    let mut names: Vec<String> = Vec::new();
    for item in planned {
        if cleaned.contains(&item.project_path) && !names.iter().any(|name| name == &item.project_name) {
            names.push(item.project_name.clone());
        }
    }
    if names.is_empty() {
        return "0".into();
    }
    let count = names.len();
    if count <= 2 {
        format!("{count} · {}", names.join(", "))
    } else {
        format!("{count} · {}, {} +{}", names[0], names[1], count - 2)
    }
}

pub fn mode_label(mode: DeleteMode) -> &'static str {
    match mode {
        DeleteMode::Permanent => "permanent",
        DeleteMode::Trash => "trash",
    }
}
