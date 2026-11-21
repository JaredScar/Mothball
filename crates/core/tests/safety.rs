use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

use mothball_core::{Engine, ProjectStatus, ScanOptions, SweepQuery};

fn data(dir: &tempfile::TempDir) -> Engine {
    Engine::open(dir.path().join("data")).unwrap()
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn age(path: &Path, days: u64) {
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    let when = SystemTime::now() - Duration::from_secs(days * 86_400 + 3_600);
    file.set_modified(when).unwrap();
}

fn scan(engine: &mut Engine, root: &Path) {
    age_sources(root);
    engine
        .scan(ScanOptions { only_root: Some(root.to_path_buf()) }, &AtomicBool::new(false), |_| {})
        .unwrap();
}

fn age_sources(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(name.as_str(), "node_modules" | "target" | "dist" | ".git") {
            continue;
        }
        if path.is_dir() {
            age_sources(&path);
        } else {
            age(&path, 30);
        }
    }
}

fn sweep_yes(engine: &mut Engine) -> mothball_core::SweepOutcome {
    let mut query = SweepQuery::default();
    query.dry_run = false;
    query.trigger = "test".into();
    engine.sweep(&query, &AtomicBool::new(false), |_| {}).unwrap()
}

fn link_dir(link: &Path, target: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if let Some(parent) = link.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let link = std::path::absolute(link).unwrap_or_else(|_| link.to_path_buf());
        let target = std::path::absolute(target).unwrap_or_else(|_| target.to_path_buf());
        let link = link.display().to_string().replace('/', "\\");
        let target = target.display().to_string().replace('/', "\\");
        let mut command = Command::new("cmd");
        command
            .raw_arg(format!("/C mklink /J \"{link}\" \"{target}\""))
            .creation_flags(0x0800_0000);
        command.status().map(|status| status.success()).unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}

#[test]
fn node_sweep_keeps_source_and_deletes_modules() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("dev");
    let project = root.join("web");
    write(&project.join("package.json"), r#"{"name":"web"}"#);
    write(&project.join("package-lock.json"), "{}");
    write(&project.join("src/main.ts"), "export const hi = 1\n");
    write(&project.join("notes/keep.txt"), "do not touch");
    write(&project.join("node_modules/leftpad/index.js"), "module.exports = 1\n");
    for path in ["package.json", "package-lock.json", "src/main.ts", "notes/keep.txt"] {
        age(&project.join(path), 30);
    }

    let mut engine = data(&tmp);
    scan(&mut engine, &root);
    let projects = engine.projects().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].status, ProjectStatus::Stale);
    assert!(projects[0].size_bytes > 0, "artifact size should be measured");

    let outcome = sweep_yes(&mut engine);
    assert!(outcome.bytes_freed > 0);
    assert!(!project.join("node_modules").exists());
    assert_eq!(fs::read_to_string(project.join("src/main.ts")).unwrap(), "export const hi = 1\n");
    assert!(project.join("notes/keep.txt").exists());
    assert!(project.join("package.json").exists());
}

#[test]
fn missing_lockfile_is_unsafe_and_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("dev");
    let project = root.join("old-portfolio");
    write(&project.join("package.json"), r#"{"name":"old"}"#);
    write(&project.join("node_modules/pkg/index.js"), "keep");
    write(&project.join("index.js"), "source");
    age(&project.join("package.json"), 40);
    age(&project.join("index.js"), 40);

    let mut engine = data(&tmp);
    scan(&mut engine, &root);
    let projects = engine.projects().unwrap();
    assert_eq!(projects[0].status, ProjectStatus::Unsafe);
    let outcome = sweep_yes(&mut engine);
    assert_eq!(outcome.bytes_freed, 0);
    assert!(project.join("node_modules/pkg/index.js").exists());
    assert!(project.join("index.js").exists());
}

#[test]
fn recent_project_is_active() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("dev");
    let project = root.join("selvr");
    write(&project.join("Cargo.toml"), "[package]\nname='selvr'\nversion='0.1.0'\n");
    write(&project.join("Cargo.lock"), "# lock\n");
    write(&project.join("src/main.rs"), "fn main() {}\n");
    write(&project.join("target/debug/selvr"), "binary");

    let mut engine = data(&tmp);
    engine
        .scan(ScanOptions { only_root: Some(root) }, &AtomicBool::new(false), |_| {})
        .unwrap();
    assert_eq!(engine.projects().unwrap()[0].status, ProjectStatus::Active);
    let outcome = sweep_yes(&mut engine);
    assert_eq!(outcome.bytes_freed, 0);
    assert!(project.join("target/debug/selvr").exists());
    assert!(project.join("src/main.rs").exists());
}

#[test]
fn mothball_file_can_protect_and_ignore() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("dev");
    let protected = root.join("perminator");
    write(&protected.join("package.json"), "{}");
    write(&protected.join("package-lock.json"), "{}");
    write(&protected.join("src/app.ts"), "app");
    write(&protected.join("node_modules/a.js"), "a");
    write(&protected.join(".mothball"), "protect = true\n");
    age(&protected.join("src/app.ts"), 40);
    age(&protected.join("package.json"), 40);

    let ignored = root.join("careful");
    write(&ignored.join("package.json"), "{}");
    write(&ignored.join("package-lock.json"), "{}");
    write(&ignored.join("src/app.ts"), "app");
    write(&ignored.join("node_modules/a.js"), "a");
    write(&ignored.join("dist/app.js"), "built");
    write(&ignored.join(".mothball"), "ignore = [\"dist\"]\n");
    age(&ignored.join("src/app.ts"), 40);
    age(&ignored.join("package.json"), 40);

    let mut engine = data(&tmp);
    let mut settings = engine.settings().clone();
    settings.ecosystems.get_mut("node").unwrap().opt_in.push("dist".into());
    engine.save_settings(settings).unwrap();
    scan(&mut engine, &root);

    let projects = engine.projects().unwrap();
    let pinned = projects.iter().find(|project| project.name == "perminator").unwrap();
    assert_eq!(pinned.status, ProjectStatus::Protected);
    sweep_yes(&mut engine);
    assert!(protected.join("node_modules/a.js").exists());
    assert!(!ignored.join("node_modules").exists());
    assert!(ignored.join("dist/app.js").exists(), "ignored dist must survive even when opt-in is on");
    assert!(ignored.join("src/app.ts").exists());
}

#[test]
fn monorepo_is_one_project() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("dev");
    let project = root.join("hivemind");
    write(&project.join("package.json"), r#"{"name":"root","private":true}"#);
    write(&project.join("pnpm-lock.yaml"), "lockfileVersion: 9\n");
    write(&project.join("packages/web/package.json"), r#"{"name":"web"}"#);
    write(&project.join("packages/api/package.json"), r#"{"name":"api"}"#);
    write(&project.join("packages/web/node_modules/a.js"), "a");
    write(&project.join("packages/api/node_modules/b.js"), "b");
    write(&project.join("node_modules/.pnpm/c.js"), "c");
    write(&project.join("README.md"), "hi");
    age(&project.join("README.md"), 20);
    age(&project.join("package.json"), 20);

    let mut engine = data(&tmp);
    scan(&mut engine, &root);
    let projects = engine.projects().unwrap();
    assert_eq!(projects.len(), 1, "nested package.json files stay inside the outer project");
    assert!(projects[0].artifact_summary.contains('×') || projects[0].artifact_summary.contains("node_modules"));
    sweep_yes(&mut engine);
    assert!(!project.join("node_modules").exists());
    assert!(!project.join("packages/web/node_modules").exists());
    assert!(project.join("packages/web/package.json").exists());
    assert!(project.join("README.md").exists());
}

#[test]
fn deleting_a_linked_node_modules_does_not_follow_it() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("dev");
    let project = root.join("linked");
    let store = tmp.path().join("store");
    write(&project.join("package.json"), "{}");
    write(&project.join("package-lock.json"), "{}");
    write(&project.join("src/main.js"), "source");
    write(&store.join("precious.txt"), "do not delete");
    age(&project.join("src/main.js"), 30);
    age(&project.join("package.json"), 30);
    assert!(link_dir(&project.join("node_modules"), &store), "could not create a directory junction");

    let mut engine = data(&tmp);
    scan(&mut engine, &root);
    sweep_yes(&mut engine);
    assert!(!project.join("node_modules").exists(), "the link itself should be removed");
    assert_eq!(fs::read_to_string(store.join("precious.txt")).unwrap(), "do not delete");
    assert!(project.join("src/main.js").exists());
}

#[test]
fn symlinks_inside_node_modules_are_not_traversed() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("dev");
    let project = root.join("pnpmish");
    let outside = tmp.path().join("global-pkg");
    write(&project.join("package.json"), "{}");
    write(&project.join("package-lock.json"), "{}");
    write(&project.join("src/main.js"), "source");
    write(&project.join("node_modules/.pnpm/real.js"), "real");
    write(&outside.join("index.js"), "outside package");
    age(&project.join("src/main.js"), 30);
    age(&project.join("package.json"), 30);
    assert!(link_dir(&project.join("node_modules/react"), &outside));

    let mut engine = data(&tmp);
    scan(&mut engine, &root);
    sweep_yes(&mut engine);
    assert!(!project.join("node_modules").exists());
    assert_eq!(fs::read_to_string(outside.join("index.js")).unwrap(), "outside package");
    assert!(project.join("src/main.js").exists());
}

#[test]
fn long_paths_inside_an_artifact_are_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("dev");
    let project = root.join("deep");
    write(&project.join("package.json"), "{}");
    write(&project.join("package-lock.json"), "{}");
    write(&project.join("src/main.js"), "source");
    age(&project.join("src/main.js"), 30);
    age(&project.join("package.json"), 30);

    let mut deep = project.join("node_modules");
    while deep.as_os_str().len() < 280 {
        deep.push("abcdefghijklmnop");
    }
    mkdir_all(&deep);
    let file = deep.join("payload.txt");
    fs::write(extended(&file), b"deep").unwrap();

    let mut engine = data(&tmp);
    scan(&mut engine, &root);
    sweep_yes(&mut engine);
    assert!(!project.join("node_modules").exists());
    assert!(project.join("src/main.js").exists());
}

#[test]
fn only_rule_artifacts_are_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("dev");
    let project = root.join("mixed");
    write(&project.join("package.json"), "{}");
    write(&project.join("package-lock.json"), "{}");
    write(&project.join("src/app.ts"), "source");
    write(&project.join("secrets/keys.txt"), "nope");
    write(&project.join("docs/guide.md"), "docs");
    write(&project.join("dist/app.js"), "built but opt-in");
    write(&project.join("node_modules/pkg/index.js"), "dep");
    for rel in ["package.json", "src/app.ts", "secrets/keys.txt", "docs/guide.md"] {
        age(&project.join(rel), 25);
    }
    let mut engine = data(&tmp);
    scan(&mut engine, &root);
    sweep_yes(&mut engine);
    assert!(!project.join("node_modules").exists());
    assert!(project.join("dist/app.js").exists());
    assert!(project.join("secrets/keys.txt").exists());
    assert!(project.join("docs/guide.md").exists());
    assert!(project.join("src/app.ts").exists());
}

#[cfg(windows)]
#[test]
fn a_locked_file_is_reported_and_source_remains() {
    use std::os::windows::fs::OpenOptionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("dev");
    let project = root.join("locked");
    write(&project.join("package.json"), "{}");
    write(&project.join("package-lock.json"), "{}");
    write(&project.join("src/main.js"), "source");
    let locked = project.join("node_modules/busy.txt");
    write(&locked, "busy");
    age(&project.join("src/main.js"), 30);
    age(&project.join("package.json"), 30);
    let handle = fs::OpenOptions::new().read(true).share_mode(0).open(&locked).unwrap();

    let mut engine = data(&tmp);
    scan(&mut engine, &root);
    let outcome = sweep_yes(&mut engine);
    assert!(locked.exists(), "a locked file must not be forced");
    assert!(project.join("src/main.js").exists());
    assert!(outcome.errors > 0 || project.join("node_modules").exists());
    drop(handle);
}

fn mkdir_all(path: &Path) {
    fs::create_dir_all(extended(path)).unwrap();
}

fn extended(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.display().to_string();
        if text.starts_with(r"\\?\") {
            path.to_path_buf()
        } else {
            PathBuf::from(format!(r"\\?\{text}"))
        }
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}
