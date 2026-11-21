# Mothball — PLAN.md

> Working name. "Mothballing" a project = putting it into dormant storage, ready to bring back later.

A cross-platform tray app that finds your dev projects, notices which ones you haven't touched in a while, and deletes their **regenerable** bloat (`node_modules`, `target`, `bin/obj`, `.venv`, build caches…) on a nightly schedule. Your source code is never deleted. Bringing a project back is one click ("Rehydrate"), which runs the right install command.

---

## 1. Goals / Non-goals

**Goals**
- Reclaim disk space on the system drive automatically, with zero babysitting.
- Never delete anything that can't be regenerated from source + lockfile.
- Run quietly from the Windows tray / macOS menu bar, with a full dashboard window when you want it.
- Be fast on huge trees (100k+ dirs). Low idle RAM (< 40 MB).

**Non-goals (v1)**
- General disk cleaner (browser caches, Windows Update leftovers, etc.).
- Moving projects to another drive / archiving source code.
- Cloud anything. 100% local, no telemetry.

---

## 2. Core concepts

| Term | Meaning |
|---|---|
| **Root** | A folder Mothball watches, e.g. `C:\dev`, `D:\work`, `~/code`. Scanned recursively (default max depth 6). |
| **Project** | A folder containing a *marker* file (`package.json`, `Cargo.toml`, `*.csproj`, …). Nested projects (monorepos) are handled — the outermost marker owns the project; workspaces inside it are its children. |
| **Artifact** | A regenerable folder inside a project (`node_modules`, `target`, …). The only things ever deleted. |
| **Last touched** | Newest *source* activity in the project (see §4). |
| **Stale** | Last touched is older than the threshold (default **7 days**). |
| **Sweep** | One cleanup run: find stale projects → delete their artifacts → log the results. |
| **Rehydrate** | Re-run the project's install/restore command to bring artifacts back. |

---

## 3. Features

### 3.1 Discovery & scanning
- Add/remove roots in Settings (drag a folder onto the window works too).
- Detects projects by marker files (table in §5). New ecosystems are plain data rules, not code.
- Skips: `.git` internals, symlinks/junctions (**never followed**), system folders, OneDrive placeholders that aren't downloaded locally.
- Incremental rescans: cache the directory index in SQLite; re-walk only roots whose top-level mtime changed, plus a full rescan weekly.
- Calculates the size of each artifact (on-disk allocation size, not logical, so the numbers match what Explorer reports).

### 3.2 Staleness detection
- Global threshold: 1–90 days (default 7). Per-root and per-project overrides.
- "Last touched" = max of:
  1. newest mtime of non-artifact, non-ignored files (walk respects `.gitignore` via the `ignore` crate, capped at N files, then sampled)
  2. last git commit time on any local branch (`.git/logs/HEAD`)
  3. `.git/index` mtime (catches staging/checkouts)
- **Not** NTFS last-access time; it's unreliable/disabled on many Windows installs.
- Projects with **uncommitted changes** are treated as active by default (toggle).

### 3.3 Scheduled sweeps ("end of every day")
- Daily at a configurable time (default **11:00 PM local**).
- Conditions (all toggleable): only when idle ≥ 10 min, only on AC power, not while gaming/full-screen.
- **Missed run catch-up:** if the PC was off/asleep, run 5 min after next login.
- **Pre-sweep heads-up:** toast 15 min before: "Tonight's sweep: 6 projects, ~18.4 GB" with **Preview · Skip tonight · Snooze 1h**.
- **Low-disk trigger (optional):** if the system drive drops below X% free (default 10%), run an early sweep and notify.
- Pause from the tray: 1 hour / until tomorrow / indefinitely.

### 3.4 Safety (the most important section)
- **Only artifact folders from the rule table are deletable.** Nothing else, ever.
- **Lockfile guard:** don't delete `node_modules` unless a lockfile exists (`package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `bun.lockb`). Same idea per ecosystem (`Cargo.lock`, `packages.lock.json`/`obj/project.assets.json`, `poetry.lock`/`uv.lock`/`requirements*.txt`). Without a lockfile, the project is marked "Unsafe to clean" and skipped (override per project).
- **Risky artifacts are opt-in:** `dist/`, `build/`, `out/`, `.venv/`, `vendor/` can hold hand-made content in some repos. Off by default; enabled per ecosystem or per project.
- **Open-project guard:** skip a project if any running process has its cwd or open file handles inside it (Windows Restart Manager API / macOS `proc_pidinfo`), or if it's open in VS Code / Cursor / JetBrains (check the IDE's recent-workspace state + running process args).
- **Protected projects:** pin a project in the UI, or drop a `.mothball` file in its root:
  ```toml
  # .mothball
  protect = true            # never clean
  # or
  threshold_days = 30
  extra = ["generated/"]    # extra folders to treat as artifacts
  ignore = ["dist/"]        # never touch these
  ```
- **Dry run first:** the first sweep after install is a dry run, and the user reviews and confirms it.
- **Delete mode:** permanent delete (default, since everything is regenerable and Recycle Bin doesn't free space) or Recycle Bin / Trash (slow for `node_modules`; warn).
- **Never follow links:** pnpm's `node_modules` is full of symlinks/junctions into the global store; deletion must unlink, not traverse. This gets dedicated tests.
- Every deletion is logged (path, size, rule, timestamp) in SQLite, viewable in History and exportable as CSV.

### 3.5 Rehydrate
- One-click from the dashboard or tray "Recent" list: runs the detected command in the project dir (`npm ci` / `pnpm install --frozen-lockfile` / `yarn install --immutable` / `cargo fetch` / `dotnet restore` / `uv sync` …).
- Streams output into a small log panel. Optional: auto-rehydrate when a project is opened in VS Code (via a tiny companion VS Code extension, later phase).

### 3.6 Global caches (secondary, opt-in)
Separate "Caches" tab, each with its own toggle and age rule. Uses the tool's own prune command where one exists, instead of raw deletes:
- npm `npm cache verify`, pnpm `pnpm store prune`, Yarn `yarn cache clean`
- NuGet `dotnet nuget locals all --clear`, Gradle `~/.gradle/caches` (entries unused > N days)
- Cargo (`cargo cache --autoclean` if installed), pip `pip cache purge`, uv `uv cache prune`
- Xcode `DerivedData`, iOS simulators (`xcrun simctl delete unavailable`)
- Docker: `docker system prune` (**off by default**, explicit confirm)
- **WSL/Docker VHDX note:** deleting inside WSL doesn't shrink the `.vhdx` on C:. Offer "Compact WSL disk" (`wsl --manage <distro> --set-sparse true` or Optimize-VHD) as an advanced action.

### 3.7 Tray / menu bar
- Icon states: idle · scanning (animated) · sweep scheduled soon · paused · error.
- **Left-click:** popover (disk meter, next sweep, reclaimable now, recent activity, quick actions).
- **Right-click:** native menu: Sweep now · Preview sweep · Pause ▸ · Open Mothball · Settings · Quit.
- Tooltip: `Mothball — 18.4 GB reclaimable · next sweep 11:00 PM`.

### 3.8 Dashboard window
- **Overview:** drive usage, total reclaimed (all time / 30 days), reclaimable now, next sweep.
- **Projects:** sortable table (name, path, ecosystems, last touched, artifact size, status). Filters: Stale / Active / Protected / Cleaned / Unsafe. Bulk actions. Row detail drawer shows each artifact folder + size + rule.
- **Sweep preview:** checklist of exactly what will be deleted, with totals; uncheck items to exclude them once or always.
- **History:** bar chart of GB reclaimed per day + log table.
- **Caches:** §3.6.
- **Settings:** roots, threshold, schedule, conditions, rules per ecosystem, safety, delete mode, notifications, start at login, theme.

### 3.9 Notifications
- Pre-sweep heads-up (above).
- Post-sweep summary: "Freed 14.2 GB from 9 projects" → click opens History.
- Errors: "3 folders couldn't be deleted (in use)" → retried on the next sweep.
- All are toggleable; quiet hours.

### 3.10 CLI (bundled, same Rust core)
```
mothball scan [--root PATH]
mothball list --stale
mothball sweep --dry-run | --yes [--older-than 14d]
mothball rehydrate PATH
mothball protect PATH
```
Useful for CI agents and scripting, and makes the core easy to test.

---

## 4. "Last touched" algorithm (pseudo)

```
fn last_touched(project) -> Time:
    t = 0
    for file in walk(project, respect_gitignore, skip=artifact_dirs ∪ [".git"], max_files=20_000):
        t = max(t, file.mtime)
    if project.has_git:
        t = max(t, mtime(".git/index"), newest_reflog_entry(".git/logs/HEAD"))
    return t

stale = now - last_touched(project) > threshold(project)
     && !(dirty_git && settings.dirty_counts_as_active)
     && !protected && !open_in_ide && !has_open_handles
```

---

## 5. Ecosystem rule table (v1)

| Ecosystem | Marker | Safe artifacts (default ON) | Opt-in artifacts | Lockfile guard | Rehydrate |
|---|---|---|---|---|---|
| Node / JS | `package.json` | `node_modules`, `.next`, `.nuxt`, `.svelte-kit`, `.angular/cache`, `.turbo`, `.parcel-cache`, `.vite`, `.expo`, `coverage` | `dist`, `build`, `out` | npm/pnpm/yarn/bun lock | `npm ci` etc. (by lockfile) |
| Rust | `Cargo.toml` | `target` | — | `Cargo.lock` (apps) | `cargo fetch` |
| .NET | `*.csproj`, `*.sln` | `bin`, `obj` | — | none needed | `dotnet restore` |
| Java/Kotlin (Gradle) | `build.gradle(.kts)` | `build`, `.gradle` | — | — | `./gradlew --offline help` |
| Java (Maven) | `pom.xml` | `target` | — | — | `mvn dependency:go-offline` |
| Python | `pyproject.toml`, `requirements.txt`, `setup.py` | `__pycache__`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `.tox` | `.venv`, `venv` | lock/requirements | `uv sync` / `pip install -r` |
| Flutter/Dart | `pubspec.yaml` | `.dart_tool`, `build` | — | `pubspec.lock` | `flutter pub get` |
| iOS (CocoaPods) | `Podfile` | `Pods` | — | `Podfile.lock` | `pod install` |
| Android | `android/` + Gradle | `android/.gradle`, `android/app/build` | — | — | gradle |
| Ionic/Capacitor | `capacitor.config.*` | Node rules + Android/iOS rules above | `www` | — | `npm ci && npx cap sync` |
| PHP | `composer.json` | — | `vendor` | `composer.lock` | `composer install` |
| Go | `go.mod` | — | — | — | (module cache is global → Caches tab) |
| Unity | `ProjectSettings/ProjectVersion.txt` | `Library`, `Temp`, `Obj`, `Logs` | `Build` | — | reopen in Unity |
| Terraform | `*.tf` | `.terraform` | — | `.terraform.lock.hcl` | `terraform init` |
| FiveM resources | `fxmanifest.lua` | `node_modules` (when `package.json` is present) | `dist` | — | `npm ci` |

Rules ship as `rules/*.toml` so contributors can add ecosystems via PR without touching Rust.

---

## 6. Architecture

**Recommendation: Tauri 2 (Rust core) + Angular UI.**
- Electron would mean ~150 MB of RAM sitting in the tray 24/7 just to delete folders. Tauri idles at ~15–30 MB and ships a ~10 MB installer.
- The hot path (walking huge trees, deleting `node_modules` with 260+ char paths) belongs in Rust anyway.
- Angular keeps the UI in your usual stack. (React works fine too.)

```
mothball/
├─ crates/
│  ├─ core/        # scanning, rules, staleness, deletion, sqlite — no UI deps
│  ├─ cli/         # `mothball` binary, thin wrapper over core
│  └─ app/         # Tauri 2 shell: tray, scheduler, notifications, IPC commands
├─ ui/             # Angular 19 standalone components, signals
├─ rules/          # node.toml, rust.toml, dotnet.toml ...
└─ PLAN.md
```

**Key crates:** `ignore` + `jwalk` (parallel gitignore-aware walking), `remove_dir_all` (robust Windows deletes, long paths), `rusqlite`, `serde`/`toml`, `chrono`, `notify-rust` / Tauri notification plugin, `tauri-plugin-autostart`, `tauri-plugin-updater`, `sysinfo` (processes, disks), `windows` crate (Restart Manager, `GetLastInputInfo` for idle, power status), `git2` (or parse `.git` files directly, which is cheaper).

**Scheduler:** in-process (tokio interval + persisted `last_run` for catch-up). The app runs at login via autostart. No Windows Service, because a service runs as SYSTEM in session 0: it can't show tray/toasts, and it shouldn't have rights over your user files. A per-user tray process is the right "service" here.

**Data locations**
- Windows: `%APPDATA%\Mothball\` (`config.toml`, `mothball.db`, `logs/`)
- macOS: `~/Library/Application Support/Mothball/`

**IPC (Tauri commands):** `scan_roots`, `list_projects(filter)`, `preview_sweep`, `run_sweep(opts)`, `cancel_sweep`, `rehydrate(path)`, `set_protected(path, bool)`, `get_history(range)`, `get_settings` / `save_settings`, events `scan_progress`, `sweep_progress`, `sweep_done`.

**SQLite schema (sketch)**
```sql
roots(id, path, enabled, max_depth, threshold_days NULL)
projects(id, root_id, path UNIQUE, name, ecosystems JSON, last_touched, git_dirty,
         protected, threshold_days NULL, status, last_scanned)
artifacts(id, project_id, rel_path, rule_id, size_bytes, last_measured)
sweeps(id, started_at, finished_at, trigger, dry_run, bytes_freed, projects_cleaned, errors)
sweep_items(id, sweep_id, project_id, rel_path, size_bytes, result, error)
```

---

## 7. Platform notes

**Windows**
- Tray: Tauri tray + a borderless popover window positioned above the tray icon (account for the taskbar being on any edge).
- Long paths: use `\\?\` prefixed paths; `remove_dir_all` handles read-only attributes.
- Defender real-time scanning slows deletes, which is fine for a background job. Show progress anyway.
- Code-sign the installer (SmartScreen). MSI + NSIS via Tauri bundler. winget manifest later.

**macOS**
- Menu bar extra: template (monochrome) icon, popover anchored under it. `LSUIElement = true` (no Dock icon unless the dashboard is open).
- Folders under `~/Desktop`, `~/Documents`, `~/Downloads` trigger TCC prompts, so explain this in onboarding. Recommend `~/code`-style roots.
- Notarize + sign. Universal binary (arm64 + x86_64).

**Linux (stretch):** Tauri tray via AppIndicator; same core.

---

## 8. Onboarding flow
1. Welcome: "Mothball cleans regenerable dev folders from projects you haven't touched in a while. Your code is never deleted."
2. Pick roots. Suggestions auto-detected (`C:\dev`, `C:\Users\<you>\source\repos`, `~/code`, `~/Projects`, `~/IdeaProjects`, and any folder with 3+ git repos under your user dir).
3. First scan with live progress → "We found 47 projects. 31 are stale. **63.8 GB** reclaimable."
4. Choose threshold + schedule (7 days, 11 PM defaults).
5. Review the first dry run → confirm → done; app minimizes to the tray.

---

## 9. Milestones

**M0 — Core + CLI (week 1)**
- Rule loader, project detection, size calc, last-touched, SQLite index.
- `mothball scan / list / sweep --dry-run / sweep --yes`.
- Tests: fixture trees for every ecosystem; pnpm symlink fixture; 300-char path fixture; open-file-lock fixture.

**M1 — Tray app MVP (week 2)**
- Tauri shell, tray icon + right-click menu, popover, Projects table, Sweep preview, manual sweep.
- Settings: roots, threshold, delete mode.

**M2 — Automation (week 3)**
- Scheduler with idle/AC conditions, catch-up, pre-sweep toast with actions, post-sweep summary.
- Open-project and IDE guards, `.mothball` file, protected projects.
- Autostart, History tab.

**M3 — Rehydrate + Caches (week 4)**
- Rehydrate with streamed output. Global caches tab. Low-disk trigger.

**M4 — Ship (week 5)**
- Onboarding, macOS menu bar polish, signing/notarization, auto-updater, landing page, GitHub release.

**Later:** VS Code/Cursor companion extension (auto-rehydrate on open, "mothball this project" command), WSL VHDX compaction, Linux, per-root "move artifacts to D:" via junctions, team policy file.

---

## 10. Acceptance criteria (MVP)
- Scans 200 projects / 2M files in < 60 s cold, < 5 s warm.
- Never deletes a path that isn't a rule-matched artifact (property test: random trees, assert all deletions ⊂ allowed set).
- Never traverses a symlink/junction during scan or delete.
- Skips projects with open handles or open in VS Code.
- Idle RAM < 40 MB, idle CPU ≈ 0%.
- Sweep result in History matches the bytes actually freed on disk (±2%).

---

## 11. Open questions
- Final name (Mothball / Dustpan / Prune / Hibernate).
- Open source (MIT) vs. free core + paid "Pro" (caches, low-disk trigger, multi-machine)?
- Default delete mode: permanent (recommended) or Trash?
- Should "Rehydrate" also run automatically on `cd` via a shell hook (PowerShell profile / zsh)?