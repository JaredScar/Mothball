use crate::model::{Settings, StoredArtifact, StoredProject};
use crate::util::path_key;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Choice {
    pub artifact: StoredArtifact,
    pub deletable: bool,
    pub display: bool,
    pub lock_blocked: bool,
    pub reason: Option<String>,
}

pub fn choices(project: &StoredProject, settings: &Settings) -> Vec<Choice> {
    project
        .artifacts
        .iter()
        .map(|artifact| choice(project, artifact, settings))
        .collect()
}

fn choice(project: &StoredProject, artifact: &StoredArtifact, settings: &Settings) -> Choice {
    let mut decision = Choice {
        artifact: artifact.clone(),
        deletable: false,
        display: false,
        lock_blocked: false,
        reason: None,
    };
    if artifact.ignored {
        decision.reason = Some("Ignored in .mothball".into());
        return decision;
    }
    if artifact.rule_id != "custom" {
        let enabled = settings
            .ecosystems
            .get(&artifact.rule_id)
            .map(|setting| setting.enabled)
            .unwrap_or(true);
        if !enabled {
            decision.reason = Some("Ecosystem is turned off".into());
            return decision;
        }
    }
    if artifact.kind == "opt_in" {
        let enabled = settings
            .ecosystems
            .get(&artifact.rule_id)
            .map(|setting| setting.opt_in.iter().any(|pattern| pattern == &artifact.pattern))
            .unwrap_or(false);
        if !enabled {
            decision.reason = Some("Opt-in folder, left off".into());
            return decision;
        }
    }
    if !artifact.lock_ok && settings.skip_without_lockfile && !project.allow_unsafe {
        decision.display = artifact.size_bytes > 0;
        decision.lock_blocked = true;
        decision.reason = Some("No lockfile, so this can't be restored exactly".into());
        return decision;
    }
    decision.display = artifact.size_bytes > 0;
    decision.deletable = true;
    decision
}

pub fn display_bytes(items: &[Choice]) -> u64 {
    items.iter().filter(|item| item.display).map(|item| item.artifact.size_bytes).sum()
}

pub fn deletable_bytes(items: &[Choice]) -> u64 {
    items
        .iter()
        .filter(|item| item.deletable && !item.lock_blocked)
        .map(|item| item.artifact.size_bytes)
        .sum()
}

pub fn threshold_days(project: &StoredProject, settings: &Settings, older_than: Option<u32>) -> u32 {
    if let Some(days) = older_than {
        return days.clamp(1, 3650);
    }
    project
        .threshold_days
        .or_else(|| {
            settings.roots.iter().find(|root| path_key(Path::new(&root.path)) == path_key(Path::new(&project.root_path))).and_then(|root| root.threshold_days)
        })
        .unwrap_or(settings.threshold_days)
        .clamp(1, 3650)
}

pub fn is_stale(project: &StoredProject, settings: &Settings, now: i64, older_than: Option<u32>) -> bool {
    if project.git_dirty && settings.dirty_counts_as_active {
        return false;
    }
    let days = threshold_days(project, settings, older_than);
    match project.last_touched {
        Some(touched) => now.saturating_sub(touched) > i64::from(days) * 86_400,
        None => true,
    }
}
