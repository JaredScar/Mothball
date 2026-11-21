use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

fn default_depth() -> u32 {
    6
}

fn default_true() -> bool {
    true
}

fn default_threshold() -> u32 {
    7
}

fn default_schedule() -> String {
    "23:00".to_string()
}

fn default_heads_up() -> u32 {
    15
}

fn default_idle() -> u32 {
    10
}

fn default_low_disk() -> u32 {
    10
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeleteMode {
    Permanent,
    Trash,
}

impl Default for DeleteMode {
    fn default() -> Self {
        Self::Permanent
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EcosystemSetting {
    pub enabled: bool,
    pub opt_in: Vec<String>,
}

impl Default for EcosystemSetting {
    fn default() -> Self {
        Self {
            enabled: true,
            opt_in: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootConfig {
    pub path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_depth")]
    pub max_depth: u32,
    #[serde(default)]
    pub threshold_days: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub onboarded: bool,
    pub confirmed_real_sweep: bool,
    #[serde(default = "default_threshold")]
    pub threshold_days: u32,
    #[serde(default = "default_true")]
    pub dirty_counts_as_active: bool,
    #[serde(default = "default_schedule")]
    pub schedule_time: String,
    #[serde(default = "default_heads_up")]
    pub heads_up_minutes: u32,
    #[serde(default = "default_true")]
    pub only_when_idle: bool,
    #[serde(default = "default_idle")]
    pub idle_minutes: u32,
    #[serde(default = "default_true")]
    pub only_on_ac: bool,
    #[serde(default = "default_true")]
    pub skip_fullscreen: bool,
    #[serde(default = "default_true")]
    pub catch_up: bool,
    #[serde(default = "default_true")]
    pub low_disk_enabled: bool,
    #[serde(default = "default_low_disk")]
    pub low_disk_percent: u32,
    pub start_at_login: bool,
    pub delete_mode: DeleteMode,
    #[serde(default = "default_true")]
    pub skip_without_lockfile: bool,
    #[serde(default = "default_true")]
    pub skip_open_projects: bool,
    #[serde(default = "default_true")]
    pub respect_mothball_files: bool,
    #[serde(default = "default_true")]
    pub notifications: bool,
    pub roots: Vec<RootConfig>,
    pub ecosystems: BTreeMap<String, EcosystemSetting>,
    pub cache_toggles: BTreeMap<String, bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarded: false,
            confirmed_real_sweep: false,
            threshold_days: 7,
            dirty_counts_as_active: true,
            schedule_time: "23:00".to_string(),
            heads_up_minutes: 15,
            only_when_idle: true,
            idle_minutes: 10,
            only_on_ac: true,
            skip_fullscreen: true,
            catch_up: true,
            low_disk_enabled: true,
            low_disk_percent: 10,
            start_at_login: true,
            delete_mode: DeleteMode::Permanent,
            skip_without_lockfile: true,
            skip_open_projects: true,
            respect_mothball_files: true,
            notifications: true,
            roots: Vec::new(),
            ecosystems: BTreeMap::new(),
            cache_toggles: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleCard {
    pub id: String,
    pub name: String,
    pub safe: Vec<String>,
    pub opt_in: Vec<String>,
    pub enabled: bool,
    pub enabled_opt_in: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootView {
    pub path: String,
    pub enabled: bool,
    pub max_depth: u32,
    pub threshold_days: Option<u32>,
    pub project_count: u32,
    pub threshold_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResponse {
    pub settings: Settings,
    pub rules: Vec<RuleCard>,
    pub roots: Vec<RootView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Badge {
    pub id: String,
    pub label: String,
    pub tone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactView {
    pub rel_path: String,
    pub pattern: String,
    pub rule_id: String,
    pub size_bytes: u64,
    pub size_label: String,
    pub kind: String,
    pub deletable: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    Stale,
    Active,
    Protected,
    Cleaned,
    Unsafe,
}

impl ProjectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stale => "stale",
            Self::Active => "active",
            Self::Protected => "protected",
            Self::Cleaned => "cleaned",
            Self::Unsafe => "unsafe",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub root_path: String,
    pub ecosystem: String,
    pub badges: Vec<Badge>,
    pub last_touched: Option<i64>,
    pub last_touched_label: String,
    pub artifact_summary: String,
    pub size_bytes: u64,
    pub size_label: String,
    pub status: ProjectStatus,
    pub protected: bool,
    pub git_dirty: bool,
    pub lockfile: Option<String>,
    pub lock_ok: bool,
    pub open_reason: Option<String>,
    pub unsafe_reason: Option<String>,
    pub action: String,
    pub artifacts: Vec<ArtifactView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskView {
    pub name: String,
    pub mount: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
    pub free_label: String,
    pub total_label: String,
    pub used_ratio: f64,
    pub reclaimable_ratio: f64,
    pub free_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub reclaimable_bytes: u64,
    pub reclaimable_label: String,
    pub freed30_bytes: u64,
    pub freed30_label: String,
    pub freed_all_bytes: u64,
    pub freed_all_label: String,
    pub project_count: u32,
    pub stale_count: u32,
    pub next_sweep_label: String,
    pub next_sweep_sub: String,
    pub roots_label: String,
    pub last_scan_label: String,
    pub disk: DiskView,
    pub paused: bool,
    pub pause_label: String,
    pub onboarded: bool,
    pub confirmed_real_sweep: bool,
    pub schedule_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentItem {
    pub name: String,
    pub path: String,
    pub badge: String,
    pub tone: String,
    pub detail: String,
    pub size_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayState {
    pub watching_label: String,
    pub disk: DiskView,
    pub reclaimable_label: String,
    pub reclaimable_bytes: u64,
    pub stale_projects: u32,
    pub next_sweep_label: String,
    pub next_sweep_prefix: String,
    pub paused: bool,
    pub pause_label: String,
    pub recent: Vec<RecentItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryBar {
    pub date: String,
    pub label: String,
    pub bytes: u64,
    pub bytes_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySweep {
    pub id: i64,
    pub when_label: String,
    pub trigger: String,
    pub projects_label: String,
    pub skipped_label: String,
    pub freed_label: String,
    pub freed_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    pub range_days: u32,
    pub total_bytes: u64,
    pub total_label: String,
    pub sweep_count: u32,
    pub all_time_bytes: u64,
    pub all_time_label: String,
    pub bars: Vec<HistoryBar>,
    pub sweeps: Vec<HistorySweep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
    pub label: String,
    pub pattern: String,
    pub rel_paths: Vec<String>,
    pub size_bytes: u64,
    pub size_label: String,
    pub checked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewGroup {
    pub name: String,
    pub path: String,
    pub age_label: String,
    pub lockfile: Option<String>,
    pub lock_ok: bool,
    pub items: Vec<PreviewItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepPreview {
    pub threshold_days: u32,
    pub delete_mode: DeleteMode,
    pub groups: Vec<PreviewGroup>,
    pub total_bytes: u64,
    pub selected_bytes: u64,
    pub folder_count: u32,
    pub selected_folders: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepQuery {
    pub dry_run: bool,
    pub trigger: String,
    pub older_than_days: Option<u32>,
    pub project_path: Option<String>,
    #[serde(default)]
    pub exclude: Vec<ExcludeItem>,
    #[serde(default)]
    pub always_exclude: bool,
    pub delete_mode: Option<DeleteMode>,
    #[serde(default)]
    pub include_unsafe: bool,
}

impl Default for SweepQuery {
    fn default() -> Self {
        Self {
            dry_run: true,
            trigger: "manual".into(),
            older_than_days: None,
            project_path: None,
            exclude: Vec::new(),
            always_exclude: false,
            delete_mode: None,
            include_unsafe: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcludeItem {
    pub path: String,
    pub rel_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepOutcome {
    pub dry_run: bool,
    pub bytes_freed: u64,
    pub bytes_label: String,
    pub projects_cleaned: u32,
    pub errors: u32,
    pub skipped: u32,
    pub skipped_summary: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub phase: String,
    pub root: String,
    pub current: String,
    pub projects_found: u32,
    pub completed: u32,
    pub total: u32,
    pub root_index: u32,
    pub root_count: u32,
    pub files_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub projects: u32,
    pub stale: u32,
    pub reclaimable_bytes: u64,
    pub reclaimable_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepProgress {
    pub project: String,
    pub path: String,
    pub rel_path: String,
    pub index: u32,
    pub total: u32,
    pub bytes_freed: u64,
    pub bytes_total: u64,
    pub files_removed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedRoot {
    pub path: String,
    pub reason: String,
    pub git_repos: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheView {
    pub id: String,
    pub name: String,
    pub detail: String,
    pub enabled: bool,
    pub available: bool,
    pub command: String,
    pub dangerous: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostConditions {
    pub idle_seconds: u64,
    pub on_ac: bool,
    pub fullscreen: bool,
    pub idle_known: bool,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub only_root: Option<std::path::PathBuf>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self { only_root: None }
    }
}

#[derive(Debug, Clone)]
pub struct FoundArtifact {
    pub rel_path: String,
    pub rule_id: String,
    pub pattern: String,
    pub size_bytes: u64,
    pub kind: String,
    pub ignored: bool,
    pub lock_ok: bool,
    pub is_link: bool,
}

#[derive(Debug, Clone)]
pub struct FoundProject {
    pub path: std::path::PathBuf,
    pub name: String,
    pub ecosystems: Vec<EcoHit>,
    pub ecosystem_label: String,
    pub workspaces: Vec<String>,
    pub artifacts: Vec<FoundArtifact>,
    pub last_touched: Option<i64>,
    pub git_dirty: bool,
    pub protected: bool,
    pub threshold_days: Option<u32>,
    pub lockfile: Option<String>,
    pub open_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcoHit {
    pub id: String,
    pub short: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct StoredArtifact {
    pub rel_path: String,
    pub rule_id: String,
    pub pattern: String,
    pub size_bytes: u64,
    pub kind: String,
    pub ignored: bool,
    pub lock_ok: bool,
}

#[derive(Debug, Clone)]
pub struct StoredProject {
    pub id: i64,
    pub root_path: String,
    pub path: String,
    pub name: String,
    pub ecosystem_label: String,
    pub ecosystems: Vec<EcoHit>,
    pub last_touched: Option<i64>,
    pub git_dirty: bool,
    pub protected: bool,
    pub threshold_days: Option<u32>,
    pub allow_unsafe: bool,
    pub open_reason: Option<String>,
    pub lockfile: Option<String>,
    pub artifacts: Vec<StoredArtifact>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RehydratePlan {
    pub program: String,
    pub args: Vec<String>,
    pub message: Option<String>,
}
