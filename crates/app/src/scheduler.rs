use std::sync::atomic::AtomicBool;

use chrono::{DateTime, Duration, Local, TimeZone};
use mothball_core::{clock_label, next_schedule, Engine, SweepQuery};
use tauri::{AppHandle, Emitter, Manager};

use crate::{engine_mut, show_toast, AppState};

pub fn arm_catch_up(engine: &Engine) {
    let settings = engine.settings();
    if !settings.onboarded || !settings.catch_up || !settings.confirmed_real_sweep {
        return;
    }
    let now = Local::now();
    let scheduled = schedule_today(&settings.schedule_time, now);
    if now <= scheduled + Duration::minutes(30) {
        return;
    }
    let today = day_key(now);
    if kv(engine, "last_sweep_day") == today || kv(engine, "catch_up_done") == today {
        return;
    }
    if !kv(engine, "catch_up_at").is_empty() {
        return;
    }
    let at = (now + Duration::minutes(5)).timestamp();
    let _ = engine.kv_set("catch_up_at", &at.to_string());
}

pub fn tick(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut engine = match state.engine.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::Poisoned(err)) => err.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return,
    };
    if !engine.settings().onboarded {
        return;
    }
    let (paused, _) = engine.pause_state().unwrap_or((false, String::new()));
    if paused {
        return;
    }

    let settings = engine.settings().clone();
    let now = Local::now();
    let today = day_key(now);
    let scheduled = schedule_today(&settings.schedule_time, now);
    let host = engine.host();
    let allowed = conditions_allow(&settings, host.idle_seconds, host.on_ac, host.fullscreen);
    let swept_today = kv(&engine, "last_sweep_day") == today;

    if !swept_today && now >= scheduled && now < scheduled + Duration::minutes(30) {
        if !settings.confirmed_real_sweep {
            if kv(&engine, "review_day") != today {
                let _ = engine.kv_set("review_day", &today);
                show_toast(
                    app,
                    "heads-up",
                    "Review your first sweep",
                    "Mothball won't delete anything until you review a sweep and confirm it.",
                );
            }
            return;
        }
        if allowed {
            let _ = sweep_locked(app, &mut engine, "scheduled", settings.notifications);
        }
        return;
    }

    if settings.catch_up && settings.confirmed_real_sweep && !swept_today {
        if let Ok(at) = kv(&engine, "catch_up_at").parse::<i64>() {
            let now_secs = now.timestamp();
            if now_secs >= at && now_secs < at + 6 * 3600 && allowed {
                let _ = sweep_locked(app, &mut engine, "catch_up", settings.notifications);
                return;
            }
            if now_secs >= at + 6 * 3600 {
                let _ = engine.kv_set("catch_up_at", "");
                let _ = engine.kv_set("catch_up_done", &today);
            }
        }
    }

    if settings.low_disk_enabled && settings.confirmed_real_sweep && kv(&engine, "low_disk_day") != today {
        if let Ok(dash) = engine.dashboard() {
            if dash.disk.free_percent < settings.low_disk_percent as f64 && dash.reclaimable_bytes > 0 && allowed {
                let _ = engine.kv_set("low_disk_day", &today);
                let _ = sweep_locked(app, &mut engine, "low_disk", settings.notifications);
                return;
            }
        }
    }

    if settings.notifications && !swept_today && settings.confirmed_real_sweep {
        let lead = Duration::minutes(settings.heads_up_minutes as i64);
        let start = scheduled - lead;
        if now >= start && now < scheduled && kv(&engine, "heads_up_day") != today && !host.fullscreen {
            if let Ok(dash) = engine.dashboard() {
                if dash.reclaimable_bytes > 0 {
                    let _ = engine.kv_set("heads_up_day", &today);
                    show_toast(
                        app,
                        "heads-up",
                        &format!("Sweep starts at {}", clock_label(&settings.schedule_time)),
                        &format!(
                            "{} projects untouched for {}+ days · about {} will be freed.",
                            dash.stale_count, settings.threshold_days, dash.reclaimable_label
                        ),
                    );
                }
            }
        }
    }

    if let Ok(dash) = engine.dashboard() {
        if let Some(tray) = app.tray_by_id("mothball") {
            let _ = tray.set_tooltip(Some(format!("Mothball · {} reclaimable", dash.reclaimable_label)));
        }
    }
}

pub fn sweep_locked(app: &AppHandle, engine: &mut Engine, trigger: &str, notify: bool) -> Result<(), String> {
    let query = SweepQuery {
        dry_run: false,
        trigger: trigger.to_string(),
        ..SweepQuery::default()
    };
    let outcome = engine
        .sweep(&query, &AtomicBool::new(false), |_| {})
        .map_err(|err| err.to_string())?;
    let today = day_key(Local::now());
    if trigger == "low_disk" {
        let _ = engine.kv_set("low_disk_day", &today);
    } else if outcome.errors == 0 || outcome.bytes_freed > 0 {
        let _ = engine.kv_set("last_sweep_day", &today);
        let _ = engine.kv_set("catch_up_at", "");
    }
    if notify {
        show_toast(app, "done", "Sweep finished", &outcome.message);
    }
    let _ = app.emit("data-changed", ());
    Ok(())
}

pub fn set_pause(engine: &Engine, until: &str) -> Result<(), String> {
    let value = match until {
        "resume" => "0".to_string(),
        "1h" => (Local::now() + Duration::hours(1)).timestamp().to_string(),
        "tomorrow" | "tonight" => tomorrow_morning().to_string(),
        "forever" => "forever".to_string(),
        other => other.to_string(),
    };
    engine.kv_set("paused_until", &value).map_err(|err| err.to_string())
}

fn conditions_allow(settings: &mothball_core::Settings, idle_seconds: u64, on_ac: bool, fullscreen: bool) -> bool {
    if settings.only_when_idle && idle_seconds < settings.idle_minutes as u64 * 60 {
        return false;
    }
    if settings.only_on_ac && !on_ac {
        return false;
    }
    if settings.skip_fullscreen && fullscreen {
        return false;
    }
    true
}

fn schedule_today(schedule_time: &str, now: DateTime<Local>) -> DateTime<Local> {
    let next = next_schedule(schedule_time, now);
    if next.date_naive() == now.date_naive() {
        next
    } else {
        next - Duration::days(1)
    }
}

fn tomorrow_morning() -> i64 {
    let next = (Local::now() + Duration::days(1)).date_naive().and_hms_opt(5, 0, 0);
    next.and_then(|naive| Local::now().timezone().from_local_datetime(&naive).single())
        .map(|time| time.timestamp())
        .unwrap_or_else(|| (Local::now() + Duration::hours(12)).timestamp())
}

fn day_key(now: DateTime<Local>) -> String {
    now.format("%Y-%m-%d").to_string()
}

fn kv(engine: &Engine, key: &str) -> String {
    engine.kv_get(key).ok().flatten().unwrap_or_default()
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        loop {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tick(&app)));
            std::thread::sleep(std::time::Duration::from_secs(20));
        }
    });
}

pub fn lock_and_sweep(app: &AppHandle, trigger: &str) {
    let app = app.clone();
    let trigger = trigger.to_string();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let mut engine = engine_mut(&state);
        if let Err(err) = sweep_locked(&app, &mut engine, &trigger, true) {
            eprintln!("mothball: {err}");
        }
    });
}
