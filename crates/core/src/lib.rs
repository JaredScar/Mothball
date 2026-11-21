mod caches;
mod db;
mod delete;
mod error;
mod gitinfo;
mod guards;
mod host;
mod model;
mod policy;
mod present;
mod rules;
mod scan;
mod size;
mod sweep;
mod util;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub use error::{Error, Result};
pub use model::*;
pub use util::{clock_label, format_bytes, next_schedule, now_secs, parse_hhmm};

use crate::rules::RuleSet;
use crate::sweep::exclusion_set;
use crate::util::{log_line, normalize_path, path_key};

pub struct Engine {
    data_dir: PathBuf,
    db: db::Database,
    rules: RuleSet,
    settings: Settings,
}

impl Engine {
    pub fn open(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let data_dir = data_dir.into();
        fs::create_dir_all(&data_dir)?;
        let rules = RuleSet::builtin()?;
        let settings = load_settings(&data_dir, &rules)?;
        let database = db::Database::open(&data_dir.join("mothball.db"))?;
        Ok(Self {
            data_dir,
            db: database,
            rules,
            settings,
        })
    }

    pub fn open_default() -> Result<Self> {
        Self::open(default_data_dir())
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn save_settings(&mut self, mut settings: Settings) -> Result<()> {
        normalize(&mut settings, &self.rules);
        let text = toml::to_string_pretty(&settings)?;
        fs::write(self.data_dir.join("config.toml"), text)?;
        self.settings = settings;
        Ok(())
    }

    pub fn add_root(&mut self, path: PathBuf) -> Result<()> {
        let path = normalize_path(&path);
        if !path.is_dir() {
            return Err(Error::msg("That folder doesn't exist."));
        }
        let text = path.display().to_string();
        if self.settings.roots.iter().any(|root| path_key(Path::new(&root.path)) == path_key(&path)) {
            return Ok(());
        }
        let mut settings = self.settings.clone();
        settings.roots.push(RootConfig {
            path: text,
            enabled: true,
            max_depth: 6,
            threshold_days: None,
        });
        self.save_settings(settings)
    }

    pub fn remove_root(&mut self, path: &Path) -> Result<()> {
        let key = path_key(&normalize_path(path));
        let mut settings = self.settings.clone();
        settings.roots.retain(|root| path_key(Path::new(&root.path)) != key);
        self.save_settings(settings)?;
        let roots: Vec<String> = self.settings.roots.iter().map(|root| root.path.clone()).collect();
        self.db.delete_projects_outside(&roots)?;
        Ok(())
    }

    pub fn scan<F: FnMut(ScanProgress) + Send + 'static>(&mut self, opts: ScanOptions, cancel: &AtomicBool, progress: F) -> Result<ScanSummary> {
        let open = if self.settings.skip_open_projects {
            guards::OpenIndex::capture()
        } else {
            guards::OpenIndex::default()
        };
        let roots = if let Some(only) = &opts.only_root {
            vec![RootConfig {
                path: normalize_path(only).display().to_string(),
                enabled: true,
                max_depth: 6,
                threshold_days: None,
            }]
        } else {
            self.settings.roots.iter().filter(|root| root.enabled).cloned().collect()
        };
        let root_count = roots.len() as u32;
        let callback = std::sync::Arc::new(std::sync::Mutex::new(progress));
        let emit: std::sync::Arc<dyn Fn(scan::DiscoverProgress) + Send + Sync> = std::sync::Arc::new(move |tick| {
            let mut cb = callback.lock().unwrap_or_else(|err| err.into_inner());
            cb(ScanProgress {
                phase: tick.phase,
                root: tick.root,
                current: tick.current,
                projects_found: tick.projects_found,
                completed: tick.completed,
                total: tick.total,
                root_index: tick.root_index,
                root_count: tick.root_count,
                files_seen: tick.files_seen,
            });
        });
        for (index, root) in roots.iter().enumerate() {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let path = PathBuf::from(&root.path);
            if !path.is_dir() {
                continue;
            }
            let found = scan::discover_root(
                &path,
                root.max_depth.max(1),
                &self.rules,
                &self.settings,
                &open,
                cancel,
                index as u32 + 1,
                root_count,
                emit.clone(),
            );
            self.db.replace_root(&path, &found, now_secs())?;
        }
        if opts.only_root.is_none() {
            let keys: Vec<String> = self.settings.roots.iter().filter(|root| root.enabled).map(|root| root.path.clone()).collect();
            self.db.delete_projects_outside(&keys)?;
        }
        self.db.kv_set("last_scan", &now_secs().to_string())?;
        let projects = self.projects()?;
        let stale = projects.iter().filter(|project| project.status == ProjectStatus::Stale).count() as u32;
        let bytes: u64 = projects
            .iter()
            .filter(|project| project.status == ProjectStatus::Stale && project.open_reason.is_none())
            .map(|project| project.size_bytes)
            .sum();
        Ok(ScanSummary {
            projects: projects.len() as u32,
            stale,
            reclaimable_bytes: bytes,
            reclaimable_label: format_bytes(bytes),
        })
    }

    pub fn projects(&self) -> Result<Vec<ProjectView>> {
        let now = now_secs();
        let stored = self.db.load_projects()?;
        Ok(stored.iter().map(|project| present::project_view(project, &self.settings, now)).collect())
    }

    pub fn dashboard(&self) -> Result<Dashboard> {
        let projects = self.projects()?;
        let (paused, pause_label) = self.pause_state()?;
        let reclaimable = projects
            .iter()
            .filter(|project| project.status == ProjectStatus::Stale && project.open_reason.is_none())
            .map(|project| project.size_bytes)
            .sum();
        let disk = host::disk_for(&self.settings.roots, reclaimable);
        let since = now_secs() - 30 * 86_400;
        let (freed_30, _) = self.db.freed_since(since)?;
        let freed_all = self.db.freed_all()?;
        let last_scan = self.db.kv_get("last_scan")?.and_then(|value| value.parse().ok());
        Ok(present::dashboard(&projects, &self.settings, disk, freed_30, freed_all, last_scan, paused, pause_label))
    }

    pub fn tray(&self) -> Result<TrayState> {
        let projects = self.projects()?;
        let (paused, pause_label) = self.pause_state()?;
        let reclaimable = projects
            .iter()
            .filter(|project| project.status == ProjectStatus::Stale && project.open_reason.is_none())
            .map(|project| project.size_bytes)
            .sum();
        let disk = host::disk_for(&self.settings.roots, reclaimable);
        let recent = self.db.recent_cleaned()?;
        Ok(present::tray(&projects, &self.settings, disk, &recent, paused, pause_label))
    }

    pub fn settings_view(&self) -> Result<SettingsResponse> {
        let counts = self.db.project_counts_by_root()?;
        let counts = counts.into_iter().collect();
        Ok(present::settings_response(&self.settings, &self.rules, &counts))
    }

    pub fn preview(&self, query: &SweepQuery) -> Result<SweepPreview> {
        let projects = self.db.load_projects()?;
        let saved = self.db.exclusions()?;
        let excluded = exclusion_set(&saved, &query.exclude);
        Ok(sweep::preview(&projects, &self.settings, &self.rules, query, &excluded, now_secs()))
    }

    pub fn sweep<F: FnMut(SweepProgress) + Send + 'static>(&mut self, query: &SweepQuery, cancel: &AtomicBool, progress: F) -> Result<SweepOutcome> {
        if query.always_exclude {
            let pairs = query.exclude.iter().map(|item| (item.path.clone(), item.rel_path.clone())).collect::<Vec<_>>();
            self.db.add_exclusions(&pairs)?;
        }
        if let Some(mode) = query.delete_mode {
            if mode != self.settings.delete_mode {
                let mut settings = self.settings.clone();
                settings.delete_mode = mode;
                self.save_settings(settings)?;
            }
        }
        let projects = self.db.load_projects()?;
        let saved = self.db.exclusions()?;
        let excluded = exclusion_set(&saved, &query.exclude);
        let outcome = sweep::run(&self.db, &projects, &self.settings, &self.rules, query, &excluded, cancel, progress)?;
        if !query.dry_run {
            log_line(&self.data_dir, &outcome.message);
            if outcome.bytes_freed > 0 {
                let mut settings = self.settings.clone();
                settings.confirmed_real_sweep = true;
                self.save_settings(settings)?;
            }
        }
        Ok(outcome)
    }

    pub fn rehydrate<F: FnMut(String)>(&self, path: &Path, mut on_line: F) -> Result<()> {
        let projects = self.db.load_projects()?;
        let key = path_key(&normalize_path(path));
        let project = projects
            .into_iter()
            .find(|project| path_key(Path::new(&project.path)) == key)
            .ok_or_else(|| Error::msg("That project isn't in the index yet. Scan it first."))?;
        let mut selected = None;
        for eco in &project.ecosystems {
            let Some(rule) = self.rules.get(&eco.id) else { continue };
            let plan = rules::rehydrate_plan(rule, project.lockfile.as_deref());
            if !plan.program.is_empty() {
                selected = Some(plan);
                break;
            }
            if selected.is_none() {
                selected = Some(plan);
            }
        }
        let Some(plan) = selected else {
            on_line("Nothing to restore for this project.".into());
            return Ok(());
        };
        if plan.program.is_empty() {
            on_line(plan.message.unwrap_or_else(|| "Nothing to restore.".into()));
            return Ok(());
        }
        on_line(format!("$ {} {}", plan.program, plan.args.join(" ")));
        let project_path = PathBuf::from(&project.path);
        let mut command = rehydrate_command(&project_path, &plan);
        command.current_dir(&project_path);
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        let mut child = command.spawn().map_err(|err| Error::msg(format!("Couldn't start {}: {err}", plan.program)))?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let out_tx = tx.clone();
        std::thread::spawn(move || pipe_lines(stdout, out_tx));
        std::thread::spawn(move || pipe_lines(stderr, tx));
        while let Ok(line) = rx.recv() {
            on_line(line);
        }
        let status = child.wait()?;
        if !status.success() {
            return Err(Error::msg(format!("Restore command exited with {status}.")));
        }
        on_line("Restore finished.".into());
        Ok(())
    }

    pub fn set_protected(&mut self, path: &Path, protected: bool) -> Result<()> {
        if !self.db.set_protected(path, protected)? {
            return Err(Error::msg("That project isn't in the index yet."));
        }
        if !protected {
            let _ = clear_moth_protect(&normalize_path(path));
        }
        Ok(())
    }

    pub fn set_allow_unsafe(&mut self, path: &Path, allow: bool) -> Result<()> {
        if !self.db.set_allow_unsafe(path, allow)? {
            return Err(Error::msg("That project isn't in the index yet."));
        }
        Ok(())
    }

    pub fn history(&self, days: u32) -> Result<HistoryView> {
        let rows = self.db.sweep_rows()?;
        let all_time = self.db.freed_all()?;
        Ok(present::history(&rows, days, all_time))
    }

    pub fn history_csv(&self) -> Result<String> {
        let items = self.db.sweep_items_for_csv()?;
        Ok(present::csv(&items))
    }

    pub fn suggest_roots(&self) -> Vec<SuggestedRoot> {
        suggest_roots()
    }

    pub fn caches(&self) -> Vec<CacheView> {
        caches::list(&self.settings)
    }

    pub fn prune_cache(&self, id: &str, confirm: bool) -> Result<String> {
        let output = caches::prune(id, confirm)?;
        log_line(&self.data_dir, &format!("cache {id}: {output}"));
        Ok(output)
    }

    pub fn host(&self) -> HostConditions {
        host::conditions()
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.db.kv_get(key)
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.db.kv_set(key, value)
    }

    pub fn pause_state(&self) -> Result<(bool, String)> {
        match self.db.kv_get("paused_until")? {
            Some(value) if value == "forever" => Ok((true, "Until you resume".into())),
            Some(value) => {
                if let Ok(until) = value.parse::<i64>() {
                    if until > now_secs() {
                        return Ok((true, format!("Until {}", crate::util::format_when_portable(until))));
                    }
                }
                Ok((false, String::new()))
            }
            None => Ok((false, String::new())),
        }
    }
}

fn rehydrate_command(project: &Path, plan: &RehydratePlan) -> std::process::Command {
    let mut program = plan.program.clone();
    if program == "gradlew" || program == "./gradlew" {
        if cfg!(windows) {
            if project.join("gradlew.bat").is_file() {
                program = "gradlew.bat".into();
            } else if project.join("android").join("gradlew.bat").is_file() {
                program = project.join("android").join("gradlew.bat").display().to_string();
            } else {
                program = "gradle".into();
            }
        } else if project.join("gradlew").is_file() {
            program = "./gradlew".into();
        } else {
            program = "gradle".into();
        }
    }
    let mut command = util::hidden_command(&program);
    command.args(&plan.args);
    command
}

fn pipe_lines(stream: Option<impl std::io::Read + Send>, tx: std::sync::mpsc::Sender<String>) {
    let Some(stream) = stream else { return };
    let reader = std::io::BufReader::new(stream);
    use std::io::BufRead;
    for line in reader.lines().map_while(std::result::Result::ok) {
        if tx.send(line).is_err() {
            break;
        }
    }
}

fn clear_moth_protect(project: &Path) -> Result<()> {
    let file = project.join(".mothball");
    if !file.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(&file)?;
    let mut value: toml::Value = toml::from_str(&text).unwrap_or(toml::Value::Table(toml::map::Map::new()));
    let toml::Value::Table(table) = &mut value else {
        return Ok(());
    };
    if table.get("protect").and_then(|item| item.as_bool()) == Some(true) {
        table.insert("protect".into(), toml::Value::Boolean(false));
        fs::write(file, toml::to_string_pretty(&value)?)?;
    }
    Ok(())
}

fn load_settings(data_dir: &Path, rules: &RuleSet) -> Result<Settings> {
    let path = data_dir.join("config.toml");
    let mut settings = if path.is_file() {
        toml::from_str(&fs::read_to_string(path)?)?
    } else {
        Settings::default()
    };
    normalize(&mut settings, rules);
    Ok(settings)
}

fn normalize(settings: &mut Settings, rules: &RuleSet) {
    if settings.threshold_days == 0 {
        settings.threshold_days = 7;
    }
    settings.threshold_days = settings.threshold_days.clamp(1, 90);
    if settings.schedule_time.trim().is_empty() {
        settings.schedule_time = "23:00".into();
    }
    if settings.heads_up_minutes == 0 {
        settings.heads_up_minutes = 15;
    }
    if settings.idle_minutes == 0 {
        settings.idle_minutes = 10;
    }
    if settings.low_disk_percent == 0 {
        settings.low_disk_percent = 10;
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut roots = Vec::new();
    for mut root in settings.roots.drain(..) {
        if root.max_depth == 0 {
            root.max_depth = 6;
        }
        root.path = normalize_path(Path::new(&root.path)).display().to_string();
        if seen.insert(path_key(Path::new(&root.path))) {
            roots.push(root);
        }
    }
    settings.roots = roots;
    for rule in rules.iter() {
        settings.ecosystems.entry(rule.id.clone()).or_insert(EcosystemSetting {
            enabled: rule.enabled_by_default,
            opt_in: Vec::new(),
        });
    }
    for id in caches::ids() {
        settings.cache_toggles.entry(id.to_string()).or_insert(false);
    }
}

pub fn default_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("MOTHBALL_DATA_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("Mothball");
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Application Support/Mothball");
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return PathBuf::from(home).join(".local").join("share").join("Mothball");
    }
    PathBuf::from("mothball-data")
}

fn suggest_roots() -> Vec<SuggestedRoot> {
    let mut found = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    let mut candidates = Vec::new();
    if let Some(home) = &home {
        for name in ["source/repos", "source", "dev", "code", "Projects", "IdeaProjects", "repos", "work", "src", "Documents/GitHub"] {
            candidates.push(home.join(name));
        }
        if let Ok(entries) = fs::read_dir(home) {
            for entry in entries.flatten().take(80) {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if matches!(name.as_str(), "appdata" | "application data" | "documents" | "downloads" | "desktop" | "pictures" | "videos" | "music" | "onedrive" | "ntuser.dat" | "cookies") {
                    continue;
                }
                let repos = count_git_children(&path);
                if repos >= 3 {
                    push_root(&mut found, &mut seen, path, format!("{repos} git repos"), repos);
                }
            }
        }
    }
    for letter in ["C", "D", "E"] {
        for name in ["dev", "work", "src", "code", "projects"] {
            candidates.push(PathBuf::from(format!(r"{letter}:\{name}")));
        }
    }
    for path in candidates {
        if path.is_dir() {
            let repos = count_git_children(&path);
            let reason = if repos > 0 { format!("{repos} git repos") } else { "Common location".into() };
            push_root(&mut found, &mut seen, path, reason, repos);
        }
    }
    found
}

fn push_root(found: &mut Vec<SuggestedRoot>, seen: &mut std::collections::BTreeSet<String>, path: PathBuf, reason: String, repos: u32) {
    let path = normalize_path(&path);
    let key = path_key(&path);
    if seen.insert(key) {
        found.push(SuggestedRoot {
            path: path.display().to_string(),
            reason,
            git_repos: repos,
        });
    }
}

fn count_git_children(dir: &Path) -> u32 {
    let mut count = 0u32;
    if dir.join(".git").exists() {
        count += 1;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return count;
    };
    for entry in entries.flatten().take(300) {
        let path = entry.path();
        if path.join(".git").exists() {
            count += 1;
        }
    }
    count
}
