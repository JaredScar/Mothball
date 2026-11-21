use serde::Deserialize;

use crate::error::{Error, Result};
use crate::model::{EcoHit, RehydratePlan};

include!(concat!(env!("OUT_DIR"), "/builtin_rules.rs"));

#[derive(Debug, Clone)]
pub struct RehydrateSpec {
    pub lockfile: Option<String>,
    pub program: String,
    pub args: Vec<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub short: String,
    pub enabled_by_default: bool,
    pub lockfile_required: bool,
    pub markers: Vec<String>,
    pub safe_artifacts: Vec<String>,
    pub opt_in_artifacts: Vec<String>,
    pub lockfiles: Vec<String>,
    pub rehydrate: Vec<RehydrateSpec>,
    pub fallback: Option<RehydrateSpec>,
}

#[derive(Debug, Clone)]
pub struct RuleSet {
    rules: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
struct RuleFile {
    id: String,
    name: String,
    #[serde(default)]
    short: Option<String>,
    #[serde(default)]
    enabled_by_default: bool,
    #[serde(default)]
    lockfile_required: bool,
    #[serde(default)]
    markers: Vec<String>,
    #[serde(default)]
    safe_artifacts: Vec<String>,
    #[serde(default)]
    opt_in_artifacts: Vec<String>,
    #[serde(default)]
    lockfiles: Vec<String>,
    #[serde(default)]
    rehydrate: Vec<RehydrateFile>,
    #[serde(default)]
    fallback: Option<RehydrateFile>,
}

#[derive(Debug, Deserialize)]
struct RehydrateFile {
    #[serde(default)]
    lockfile: Option<String>,
    #[serde(default)]
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    label: Option<String>,
}

impl RuleSet {
    pub fn builtin() -> Result<Self> {
        let mut rules = Vec::new();
        for (name, content) in FILES {
            let file: RuleFile = toml::from_str(content).map_err(|err| {
                Error::msg(format!("rule {name} is invalid: {err}"))
            })?;
            rules.push(file.into_rule());
        }
        rules.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Self { rules })
    }

    pub fn iter(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter()
    }

    pub fn get(&self, id: &str) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.id == id)
    }

    pub fn markers_in(&self, dir: &std::path::Path) -> Vec<String> {
        self.rules
            .iter()
            .filter(|rule| rule.markers.iter().any(|marker| marker_exists(dir, marker)))
            .map(|rule| rule.id.clone())
            .collect()
    }
}

impl RuleFile {
    fn into_rule(self) -> Rule {
        let map = |item: RehydrateFile| RehydrateSpec {
            lockfile: item.lockfile,
            program: item.program,
            args: item.args,
            label: item.label,
        };
        Rule {
            id: self.id,
            short: self.short.unwrap_or_else(|| self.name.clone()),
            name: self.name,
            enabled_by_default: self.enabled_by_default,
            lockfile_required: self.lockfile_required,
            markers: self.markers,
            safe_artifacts: self.safe_artifacts,
            opt_in_artifacts: self.opt_in_artifacts,
            lockfiles: self.lockfiles,
            rehydrate: self.rehydrate.into_iter().map(map).collect(),
            fallback: self.fallback.map(map),
        }
    }
}

pub fn marker_exists(dir: &std::path::Path, marker: &str) -> bool {
    let (parent, pattern) = match marker.rsplit_once(['/', '\\']) {
        Some((parent, file)) => (dir.join(parent), file.to_string()),
        None => (dir.to_path_buf(), marker.to_string()),
    };
    if pattern.contains('*') {
        let Ok(entries) = std::fs::read_dir(&parent) else {
            return false;
        };
        return entries.flatten().any(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            glob_match(&pattern, &name)
                && entry
                    .file_type()
                    .map(|kind| kind.is_file())
                    .unwrap_or(false)
        });
    }
    std::fs::symlink_metadata(parent.join(pattern))
        .map(|meta| meta.is_file())
        .unwrap_or(false)
}

fn glob_match(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return name == pattern;
    }
    if parts.len() == 2 {
        return name.starts_with(parts[0])
            && name.ends_with(parts[1])
            && name.len() >= parts[0].len() + parts[1].len();
    }
    let mut rest = name;
    for (index, part) in parts.iter().enumerate() {
        if index == 0 {
            if !rest.starts_with(part) {
                return false;
            }
            rest = &rest[part.len()..];
        } else if index + 1 == parts.len() {
            return rest.ends_with(part);
        } else if part.is_empty() {
            continue;
        } else if let Some(found) = rest.find(part) {
            rest = &rest[found + part.len()..];
        } else {
            return false;
        }
    }
    true
}

pub fn pattern_match(rel: &str, pattern: &str) -> bool {
    let rel = rel.replace('\\', "/");
    let pattern = pattern.trim().trim_matches('/');
    if pattern.is_empty() {
        return false;
    }
    if pattern.contains('/') {
        rel == pattern || rel.ends_with(&format!("/{pattern}"))
    } else {
        rel == pattern || rel.rsplit('/').next() == Some(pattern)
    }
}

pub fn display_for(rule: &Rule, lockfile: Option<&str>) -> EcoHit {
    let mut label = rule.short.clone();
    if let Some(file) = lockfile {
        if let Some(spec) = rule
            .rehydrate
            .iter()
            .find(|spec| spec.lockfile.as_deref() == Some(file))
        {
            if let Some(extra) = &spec.label {
                if !extra.eq_ignore_ascii_case(&rule.short) && !extra.eq_ignore_ascii_case(&rule.name)
                {
                    label = format!("{} · {extra}", rule.short);
                }
            }
        }
    }
    EcoHit {
        id: rule.id.clone(),
        short: rule.short.clone(),
        label,
    }
}

pub fn rehydrate_plan(rule: &Rule, lockfile: Option<&str>) -> RehydratePlan {
    let spec = rule
        .rehydrate
        .iter()
        .find(|spec| spec.lockfile.as_deref() == lockfile)
        .or(rule.fallback.as_ref());
    match spec {
        Some(spec) if !spec.program.is_empty() => RehydratePlan {
            program: spec.program.clone(),
            args: spec.args.clone(),
            message: None,
        },
        _ if rule.id == "unity" => RehydratePlan {
            program: String::new(),
            args: Vec::new(),
            message: Some("Open this project in Unity. It rebuilds Library, Temp, and Logs.".into()),
        },
        _ if rule.id == "go" => RehydratePlan {
            program: String::new(),
            args: Vec::new(),
            message: Some("Go keeps its module cache globally. Prune it from Global caches.".into()),
        },
        _ => RehydratePlan {
            program: String::new(),
            args: Vec::new(),
            message: Some("No restore command is configured for this project.".into()),
        },
    }
}

pub fn badge_for(id: &str) -> (&'static str, &'static str) {
    match id {
        "node" | "ionic" | "fivem" => ("JS", "js"),
        "rust" => ("RS", "rs"),
        "dotnet" => ("NET", "net"),
        "python" => ("PY", "py"),
        "gradle" | "android" | "maven" => ("JV", "jv"),
        "flutter" => ("FT", "ft"),
        "ios" => ("iOS", "ios"),
        "php" => ("PHP", "php"),
        "go" => ("GO", "go"),
        "unity" => ("UY", "uy"),
        "terraform" => ("TF", "tf"),
        _ => ("•", "js"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_rules_cover_the_v1_table() {
        let rules = RuleSet::builtin().unwrap();
        for id in [
            "node", "rust", "dotnet", "python", "gradle", "android", "maven", "flutter", "ios",
            "ionic", "php", "go", "unity", "terraform", "fivem",
        ] {
            assert!(rules.get(id).is_some(), "missing {id}");
        }
        let node = rules.get("node").unwrap();
        assert!(node.safe_artifacts.iter().any(|name| name == "node_modules"));
        assert!(node.opt_in_artifacts.iter().any(|name| name == "dist"));
        assert!(!rules.get("unity").unwrap().enabled_by_default);
        assert!(!rules.get("php").unwrap().enabled_by_default);
    }

    #[test]
    fn artifact_patterns_match_nested_folders() {
        assert!(pattern_match("node_modules", "node_modules"));
        assert!(pattern_match("packages/web/node_modules", "node_modules"));
        assert!(pattern_match("android/app/build", "build"));
        assert!(pattern_match("src/app", ".angular/cache") == false);
        assert!(pattern_match("web/.angular/cache", ".angular/cache"));
        assert!(!pattern_match("src/lib.rs", "target"));
    }
}
