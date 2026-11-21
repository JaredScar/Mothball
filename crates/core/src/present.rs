use std::collections::BTreeMap;
use std::path::Path;

use chrono::{Duration, Local, NaiveDate, TimeZone};

use crate::db::{CsvItem, RecentCleanRow, SweepRow};
use crate::model::{
    ArtifactView, Badge, Dashboard, DiskView, HistoryBar, HistorySweep, HistoryView, ProjectStatus, ProjectView, RecentItem, RootConfig, RootView, RuleCard, Settings, SettingsResponse, StoredProject, TrayState,
};
use crate::policy::{self, Choice};
use crate::rules::{self, RuleSet};
use crate::util::{age_label, clock_label, format_bytes, format_day_portable, format_when_portable, next_schedule, now_secs, path_key, schedule_is_today_later};

pub fn project_view(project: &StoredProject, settings: &Settings, now: i64) -> ProjectView {
    let decided = policy::choices(project, settings);
    let shown = display_bytes(&decided);
    let blocked: u64 = decided.iter().filter(|item| item.lock_blocked).map(|item| item.artifact.size_bytes).sum();
    let stale = policy::is_stale(project, settings, now, None);
    let status = if project.protected {
        ProjectStatus::Protected
    } else if !stale {
        ProjectStatus::Active
    } else if shown == 0 && blocked == 0 {
        ProjectStatus::Cleaned
    } else if shown == 0 && blocked > 0 {
        ProjectStatus::Unsafe
    } else {
        ProjectStatus::Stale
    };
    let size = if status == ProjectStatus::Unsafe { blocked } else { shown };
    let action = match status {
        ProjectStatus::Stale => "clean",
        ProjectStatus::Active => "protect",
        ProjectStatus::Protected => "unprotect",
        ProjectStatus::Cleaned => "rehydrate",
        ProjectStatus::Unsafe => "review",
    };
    let unsafe_reason = if status == ProjectStatus::Unsafe {
        Some("No lockfile — restoring this later would not be exact.".to_string())
    } else {
        None
    };
    ProjectView {
        id: project.id,
        name: project.name.clone(),
        path: project.path.clone(),
        root_path: project.root_path.clone(),
        ecosystem: project.ecosystem_label.clone(),
        badges: badges(project),
        last_touched: project.last_touched,
        last_touched_label: project.last_touched.map(|ts| age_label(ts, now)).unwrap_or_else(|| "No source activity".into()),
        artifact_summary: summary(&decided),
        size_bytes: size,
        size_label: format_bytes(size),
        status,
        protected: project.protected,
        git_dirty: project.git_dirty,
        lockfile: project.lockfile.clone(),
        lock_ok: project.artifacts.iter().all(|artifact| artifact.lock_ok || artifact.kind == "opt_in"),
        open_reason: project.open_reason.clone(),
        unsafe_reason,
        action: action.into(),
        artifacts: decided
            .iter()
            .map(|item| ArtifactView {
                rel_path: item.artifact.rel_path.clone(),
                pattern: item.artifact.pattern.clone(),
                rule_id: item.artifact.rule_id.clone(),
                size_bytes: item.artifact.size_bytes,
                size_label: format_bytes(item.artifact.size_bytes),
                kind: item.artifact.kind.clone(),
                deletable: item.deletable && item.display && !project.protected && status == ProjectStatus::Stale,
                reason: item.reason.clone(),
            })
            .collect(),
    }
}

fn display_bytes(items: &[Choice]) -> u64 {
    items.iter().filter(|item| item.display && !item.lock_blocked).map(|item| item.artifact.size_bytes).sum()
}

fn summary(items: &[Choice]) -> String {
    let mut groups: BTreeMap<String, usize> = BTreeMap::new();
    for item in items.iter().filter(|item| item.display) {
        *groups.entry(item.artifact.pattern.clone()).or_default() += 1;
    }
    if groups.is_empty() {
        return "none".into();
    }
    groups
        .into_iter()
        .map(|(pattern, count)| if count > 1 { format!("{pattern} ×{count}") } else { pattern })
        .collect::<Vec<_>>()
        .join(", ")
}

fn badges(project: &StoredProject) -> Vec<Badge> {
    let mut seen = BTreeMap::new();
    for eco in &project.ecosystems {
        let (label, tone) = rules::badge_for(&eco.id);
        seen.entry(eco.id.clone()).or_insert(Badge {
            id: eco.id.clone(),
            label: label.to_string(),
            tone: tone.to_string(),
        });
    }
    seen.into_values().collect()
}

pub fn dashboard(
    projects: &[ProjectView],
    settings: &Settings,
    disk: DiskView,
    freed_30: u64,
    freed_all: u64,
    last_scan: Option<i64>,
    paused: bool,
    pause_label: String,
) -> Dashboard {
    let reclaimable: u64 = projects
        .iter()
        .filter(|project| project.status == ProjectStatus::Stale && project.open_reason.is_none())
        .map(|project| project.size_bytes)
        .sum();
    let stale_count = projects.iter().filter(|project| project.status == ProjectStatus::Stale).count() as u32;
    let now = Local::now();
    let tonight = schedule_is_today_later(&settings.schedule_time, now);
    Dashboard {
        reclaimable_bytes: reclaimable,
        reclaimable_label: format_bytes(reclaimable),
        freed30_bytes: freed_30,
        freed30_label: format_bytes(freed_30),
        freed_all_bytes: freed_all,
        freed_all_label: format_bytes(freed_all),
        project_count: projects.len() as u32,
        stale_count,
        next_sweep_label: if paused { "Paused".into() } else { clock_label(&settings.schedule_time) },
        next_sweep_sub: if paused {
            pause_label.clone()
        } else if tonight {
            "Tonight".into()
        } else {
            "Tomorrow".into()
        },
        roots_label: roots_label(&settings.roots),
        last_scan_label: last_scan.map(|ts| age_label(ts, now_secs())).unwrap_or_else(|| "not yet".into()),
        disk,
        paused,
        pause_label,
        onboarded: settings.onboarded,
        confirmed_real_sweep: settings.confirmed_real_sweep,
        schedule_time: settings.schedule_time.clone(),
    }
}

pub fn tray(
    projects: &[ProjectView],
    settings: &Settings,
    disk: DiskView,
    recent_rows: &[RecentCleanRow],
    paused: bool,
    pause_label: String,
) -> TrayState {
    let reclaimable: u64 = projects
        .iter()
        .filter(|project| project.status == ProjectStatus::Stale && project.open_reason.is_none())
        .map(|project| project.size_bytes)
        .sum();
    let stale = projects.iter().filter(|project| project.status == ProjectStatus::Stale).count() as u32;
    let folders = settings.roots.iter().filter(|root| root.enabled).count();
    let now = Local::now();
    let tonight = schedule_is_today_later(&settings.schedule_time, now);
    let mut recent = Vec::new();
    let mut grouped: BTreeMap<String, (String, String, u64, Vec<String>)> = BTreeMap::new();
    for row in recent_rows {
        let entry = grouped.entry(row.path.clone()).or_insert((row.name.clone(), row.path.clone(), 0, Vec::new()));
        entry.2 += row.size_bytes;
        if !entry.3.iter().any(|rel| rel == &row.rel_path) {
            entry.3.push(row.rel_path.clone());
        }
    }
    let mut grouped: Vec<_> = grouped.into_values().collect();
    grouped.sort_by(|a, b| b.2.cmp(&a.2));
    for (name, path, size, rels) in grouped.into_iter().take(4) {
        let project = projects.iter().find(|project| path_key(Path::new(&project.path)) == path_key(Path::new(&path)));
        let (badge, tone) = project
            .and_then(|project| project.badges.first())
            .map(|badge| (badge.label.clone(), badge.tone.clone()))
            .unwrap_or_else(|| ("•".into(), "js".into()));
        let detail = rels
            .iter()
            .map(|rel| rel.rsplit('/').next().unwrap_or(rel).to_string())
            .collect::<Vec<_>>()
            .join(" · ");
        recent.push(RecentItem {
            name,
            path,
            badge,
            tone,
            detail,
            size_label: format_bytes(size),
        });
    }
    let _ = next_schedule(&settings.schedule_time, now);
    TrayState {
        watching_label: format!(
            "Watching {folders} folder{} · {} project{}",
            if folders == 1 { "" } else { "s" },
            projects.len(),
            if projects.len() == 1 { "" } else { "s" }
        ),
        disk,
        reclaimable_label: format_bytes(reclaimable),
        reclaimable_bytes: reclaimable,
        stale_projects: stale,
        next_sweep_label: clock_label(&settings.schedule_time),
        next_sweep_prefix: if paused {
            "Paused".into()
        } else if tonight {
            "Next sweep tonight at".into()
        } else {
            "Next sweep tomorrow at".into()
        },
        paused,
        pause_label,
        recent,
    }
}

pub fn history(rows: &[SweepRow], days: u32, all_time: u64) -> HistoryView {
    let days = days.clamp(1, 365);
    let today = Local::now().date_naive();
    let mut bars = Vec::new();
    let mut total = 0u64;
    let mut sweep_count = 0u32;
    for offset in (0..days).rev() {
        let date = today - Duration::days(i64::from(offset));
        let start = day_start(date);
        let end = start + 86_400;
        let bytes: u64 = rows
            .iter()
            .filter(|row| row.started_at >= start && row.started_at < end)
            .map(|row| row.bytes_freed)
            .sum();
        total += bytes;
        sweep_count += rows.iter().filter(|row| row.started_at >= start && row.started_at < end).count() as u32;
        let index = bars.len();
        let last = days as usize - 1;
        let label = if index == 0 || index == last || index == last / 2 {
            format_day_portable(Local.from_local_datetime(&date.and_hms_opt(12, 0, 0).unwrap()).single().unwrap_or_else(Local::now))
        } else {
            String::new()
        };
        bars.push(HistoryBar {
            date: date.format("%Y-%m-%d").to_string(),
            label,
            bytes,
            bytes_label: format_bytes(bytes),
        });
    }
    let range_start = day_start(today - Duration::days(i64::from(days) - 1));
    let sweeps = rows
        .iter()
        .filter(|row| row.started_at >= range_start)
        .take(50)
        .map(|row| HistorySweep {
            id: row.id,
            when_label: format_when_portable(row.started_at),
            trigger: trigger_label(&row.trigger),
            projects_label: row.summary.clone(),
            skipped_label: if row.skipped_summary.is_empty() { "None".into() } else { row.skipped_summary.clone() },
            freed_label: format_bytes(row.bytes_freed),
            freed_bytes: row.bytes_freed,
        })
        .collect();
    HistoryView {
        range_days: days,
        total_bytes: total,
        total_label: format_bytes(total),
        sweep_count,
        all_time_bytes: all_time,
        all_time_label: format_bytes(all_time),
        bars,
        sweeps,
    }
}

fn trigger_label(trigger: &str) -> String {
    match trigger {
        "scheduled" => "Scheduled".into(),
        "manual" => "Manual".into(),
        "tray" => "Manual (tray)".into(),
        "low_disk" => "Low disk".into(),
        "catch_up" => "Catch-up (PC was off)".into(),
        other => other.to_string(),
    }
}

fn day_start(date: NaiveDate) -> i64 {
    let naive = date.and_hms_opt(0, 0, 0).unwrap();
    Local.from_local_datetime(&naive).single().map(|dt| dt.timestamp()).unwrap_or(0)
}

pub fn settings_response(settings: &Settings, rules: &RuleSet, counts: &BTreeMap<String, u32>) -> SettingsResponse {
    let cards = rules
        .iter()
        .map(|rule| {
            let setting = settings.ecosystems.get(&rule.id);
            RuleCard {
                id: rule.id.clone(),
                name: rule.name.clone(),
                safe: rule.safe_artifacts.clone(),
                opt_in: rule.opt_in_artifacts.clone(),
                enabled: setting.map(|setting| setting.enabled).unwrap_or(rule.enabled_by_default),
                enabled_opt_in: setting.map(|setting| setting.opt_in.clone()).unwrap_or_default(),
            }
        })
        .collect();
    let roots = settings
        .roots
        .iter()
        .map(|root| {
            let count = counts.get(&path_key(Path::new(&root.path))).copied().unwrap_or(0);
            RootView {
                path: root.path.clone(),
                enabled: root.enabled,
                max_depth: root.max_depth,
                threshold_days: root.threshold_days,
                project_count: count,
                threshold_label: root
                    .threshold_days
                    .map(|days| format!("{count} projects · {days} days"))
                    .unwrap_or_else(|| format!("{count} projects · default threshold")),
            }
        })
        .collect();
    SettingsResponse {
        settings: settings.clone(),
        rules: cards,
        roots,
    }
}

pub fn csv(items: &[CsvItem]) -> String {
    let mut out = String::from("when,trigger,project,path,folder,size_bytes,result,error\n");
    for item in items {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            csv_escape(&format_when_portable(item.started_at)),
            csv_escape(&item.trigger),
            csv_escape(&item.project_name),
            csv_escape(&item.project_path),
            csv_escape(&item.rel_path),
            item.size_bytes,
            csv_escape(&item.result),
            csv_escape(item.error.as_deref().unwrap_or("")),
        ));
    }
    out
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn roots_label(roots: &[RootConfig]) -> String {
    let enabled: Vec<&str> = roots.iter().filter(|root| root.enabled).map(|root| root.path.as_str()).collect();
    if enabled.is_empty() {
        "No folders yet".into()
    } else {
        enabled.join(" · ")
    }
}
