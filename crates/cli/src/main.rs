use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use mothball_core::{Engine, ScanOptions, SweepQuery};

#[derive(Parser)]
#[command(name = "mothball", version, about = "Clean regenerable build folders from projects you have not touched")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Find projects under the saved roots, or one folder
    Scan {
        /// Scan this folder without forgetting the saved roots
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// List projects from the last scan
    List {
        /// Only projects old enough to clean
        #[arg(long)]
        stale: bool,
    },
    /// Delete regenerable artifacts from stale projects
    Sweep {
        /// Show what would be deleted
        #[arg(long)]
        dry_run: bool,
        /// Delete for real. Without this flag, the sweep is only a preview.
        #[arg(long)]
        yes: bool,
        /// Override the age threshold, for example 14d
        #[arg(long)]
        older_than: Option<String>,
        /// Only this project
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Run the project's install or restore command
    Rehydrate { path: PathBuf },
    /// Never clean this project
    Protect { path: PathBuf },
    /// Allow cleaning this project again
    Unprotect { path: PathBuf },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut engine = Engine::open_default()?;
    let cancel = AtomicBool::new(false);
    match cli.command {
        Command::Scan { root } => {
            let summary = engine.scan(ScanOptions { only_root: root }, &cancel, |progress| {
                println!("{}  {}  ({} projects)", progress.phase, progress.root, progress.projects_found);
            })?;
            println!(
                "Found {} projects. {} stale. {} reclaimable.",
                summary.projects, summary.stale, summary.reclaimable_label
            );
        }
        Command::List { stale } => {
            let projects = engine.projects()?;
            if projects.is_empty() {
                println!("No projects yet. Run `mothball scan --root <folder>`.");
                return Ok(());
            }
            for project in projects.iter().filter(|project| !stale || project.status == mothball_core::ProjectStatus::Stale) {
                println!(
                    "{:<10} {:>8}  {:<22} {:<16} {}",
                    format!("{:?}", project.status).to_lowercase(),
                    project.size_label,
                    project.name,
                    project.last_touched_label,
                    project.path
                );
            }
        }
        Command::Sweep { dry_run, yes, older_than, project } => {
            if yes && dry_run {
                return Err(anyhow!("Choose either --dry-run or --yes."));
            }
            let query = SweepQuery {
                dry_run: !yes,
                trigger: "manual".into(),
                older_than_days: older_than.as_deref().map(parse_days).transpose()?,
                project_path: project.map(|path| path.display().to_string()),
                exclude: Vec::new(),
                always_exclude: false,
                delete_mode: None,
                include_unsafe: false,
            };
            if !yes {
                let preview = engine.preview(&query)?;
                if preview.groups.is_empty() {
                    println!("Nothing to clean.");
                    return Ok(());
                }
                for group in &preview.groups {
                    println!("{}  ({})", group.name, group.age_label);
                    for item in &group.items {
                        let mark = if item.checked { "x" } else { " " };
                        println!("  [{mark}] {:<28} {}", item.label, item.size_label);
                    }
                }
                println!(
                    "{} folders, {}. Re-run with --yes to delete them. Source files are not touched.",
                    preview.selected_folders,
                    mothball_core::format_bytes(preview.selected_bytes)
                );
                return Ok(());
            }
            let outcome = engine.sweep(&query, &cancel, |_| {})?;
            println!("{}", outcome.message);
            if outcome.skipped_summary != "None" {
                println!("Skipped: {}", outcome.skipped_summary);
            }
        }
        Command::Rehydrate { path } => {
            engine.rehydrate(&path, |line| println!("{line}"))?;
        }
        Command::Protect { path } => {
            engine.set_protected(&path, true)?;
            println!("Protected {}", path.display());
        }
        Command::Unprotect { path } => {
            engine.set_protected(&path, false)?;
            println!("Unprotected {}", path.display());
        }
    }
    Ok(())
}

fn parse_days(value: &str) -> Result<u32> {
    let cleaned = value.trim().trim_end_matches(['d', 'D']);
    cleaned.parse::<u32>().map_err(|_| anyhow!("Expected a number of days, like 14d"))
}
