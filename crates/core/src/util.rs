use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Timelike};

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn system_time_secs(time: SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
}

pub fn normalize_path(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    strip_verbatim(abs)
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy().to_string();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

pub fn path_key(path: &Path) -> String {
    let normalized = normalize_path(path);
    let mut text = normalized.to_string_lossy().trim_end_matches(['\\', '/']).to_string();
    #[cfg(windows)]
    {
        text = text.replace('/', "\\").to_ascii_lowercase();
    }
    text
}

pub fn paths_equal(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

pub fn is_strict_child(parent: &Path, child: &Path) -> bool {
    let parent_key = path_key(parent);
    let child_key = path_key(child);
    if child_key.len() <= parent_key.len() {
        return false;
    }
    let sep = std::path::MAIN_SEPARATOR;
    child_key.starts_with(&parent_key) && child_key[parent_key.len()..].starts_with(sep)
}

/// Join a project path with a relative artifact path, rejecting anything that escapes the project.
pub fn safe_join(project: &Path, rel: &str) -> Option<PathBuf> {
    if rel.trim().is_empty() {
        return None;
    }
    let rel_owned = rel.replace('/', std::path::MAIN_SEPARATOR_STR);
    let rel_path = Path::new(&rel_owned);
    if rel_path.is_absolute() {
        return None;
    }
    if rel_path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    let project = normalize_path(project);
    let full = normalize_path(&project.join(rel_path));
    if is_strict_child(&project, &full) {
        Some(full)
    } else {
        None
    }
}

pub fn extended_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let abs = normalize_path(path);
        let text = abs.to_string_lossy();
        if text.starts_with(r"\\?\") {
            return abs;
        }
        if let Some(rest) = text.strip_prefix(r"\\") {
            return PathBuf::from(format!(r"\\?\UNC\{rest}"));
        }
        return PathBuf::from(format!(r"\\?\{text}"));
    }
    #[cfg(not(windows))]
    {
        normalize_path(path)
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let value = bytes as f64;
    if bytes == 0 {
        return "0 B".to_string();
    }
    if value < KB {
        return format!("{bytes} B");
    }
    if value < KB * KB {
        return format_unit(value / KB, "KB");
    }
    if value < KB * KB * KB {
        return format_unit(value / (KB * KB), "MB");
    }
    if value < KB * KB * KB * KB {
        return format_gb(value / (KB * KB * KB));
    }
    format_gb(value / (KB * KB * KB * KB)).replace("GB", "TB")
}

fn format_unit(value: f64, unit: &str) -> String {
    if value < 10.0 {
        format!("{value:.1} {unit}")
    } else {
        format!("{} {unit}", value.round() as u64)
    }
}

fn format_gb(value: f64) -> String {
    if (value - value.round()).abs() < 0.05 && value >= 100.0 {
        format!("{} GB", value.round() as u64)
    } else {
        format!("{value:.1} GB")
    }
}

pub fn format_gb_int(bytes: u64) -> String {
    let gb = (bytes as f64) / (1024.0 * 1024.0 * 1024.0);
    if gb >= 10.0 {
        format!("{} GB", gb.round() as u64)
    } else if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format_bytes(bytes)
    }
}

pub fn age_label(ts: i64, now: i64) -> String {
    let Some(then) = Local.timestamp_opt(ts, 0).single() else {
        return "Unknown".to_string();
    };
    let Some(now_dt) = Local.timestamp_opt(now, 0).single() else {
        return "Unknown".to_string();
    };
    let days = (now_dt.date_naive() - then.date_naive()).num_days();
    if days <= 0 {
        let secs = (now - ts).max(0);
        if secs < 60 {
            return "Just now".to_string();
        }
        if secs < 3600 {
            let mins = (secs / 60).max(1);
            return format!("{mins} min ago");
        }
        let hours = secs / 3600;
        if hours == 1 {
            return "1 hour ago".to_string();
        }
        return format!("{hours} hours ago");
    }
    if days == 1 {
        return "Yesterday".to_string();
    }
    if days < 30 {
        return format!("{days} days ago");
    }
    if days < 365 {
        let months = (days / 30).max(1);
        if months == 1 {
            return "1 month ago".to_string();
        }
        return format!("{months} months ago");
    }
    let years = (days / 365).max(1);
    if years == 1 {
        "1 year ago".to_string()
    } else {
        format!("{years} years ago")
    }
}

pub fn age_short(ts: i64, now: i64) -> String {
    let label = age_label(ts, now);
    label
        .trim_end_matches(" ago")
        .replace("Yesterday", "1 day")
        .replace("Just now", "now")
        .to_string()
}

pub fn format_clock(ts: i64) -> String {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%I:%M %p").to_string().trim_start_matches('0').to_string())
        .unwrap_or_else(|| "--".to_string())
}

pub fn format_when(ts: i64) -> String {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| {
            format!(
                "{} · {}",
                dt.format("%b %-d"),
                dt.format("%I:%M %p").to_string().trim_start_matches('0')
            )
        })
        .unwrap_or_else(|| "--".to_string())
}

pub fn format_day(ts: i64) -> String {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%b %-d").to_string())
        .unwrap_or_default()
}

/// chrono on Windows doesn't support `%-d`. Fall back if formatting failed.
pub fn format_when_portable(ts: i64) -> String {
    let Some(dt) = Local.timestamp_opt(ts, 0).single() else {
        return "--".to_string();
    };
    format!("{} {} · {}", month(dt), dt.day(), clock(dt))
}

pub fn format_day_portable(dt: DateTime<Local>) -> String {
    format!("{} {}", month(dt), dt.day())
}

fn month(dt: DateTime<Local>) -> &'static str {
    match dt.month() {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    }
}

fn clock(dt: DateTime<Local>) -> String {
    let (mut hour, minute) = (dt.hour(), dt.minute());
    let suffix = if hour >= 12 { "PM" } else { "AM" };
    hour %= 12;
    if hour == 0 {
        hour = 12;
    }
    format!("{hour}:{minute:02} {suffix}")
}

pub fn parse_hhmm(value: &str) -> (u32, u32) {
    let text = value.trim();
    let upper = text.to_ascii_uppercase();
    let pm = upper.ends_with("PM");
    let am = upper.ends_with("AM");
    let cleaned = upper.trim_end_matches("PM").trim_end_matches("AM").trim();
    let mut parts = cleaned.split(':');
    let mut hour: u32 = parts.next().and_then(|p| p.trim().parse().ok()).unwrap_or(23);
    let minute: u32 = parts.next().and_then(|p| p.trim().parse().ok()).unwrap_or(0);
    if pm && hour < 12 {
        hour += 12;
    }
    if am && hour == 12 {
        hour = 0;
    }
    (hour.min(23), minute.min(59))
}

pub fn next_schedule(schedule_time: &str, now: DateTime<Local>) -> DateTime<Local> {
    let (hour, minute) = parse_hhmm(schedule_time);
    let time = NaiveTime::from_hms_opt(hour, minute, 0).unwrap_or(NaiveTime::from_hms_opt(23, 0, 0).unwrap());
    let today = now.date_naive().and_time(time);
    let target = if today > now.naive_local() {
        today
    } else {
        now.date_naive()
            .checked_add_signed(Duration::days(1))
            .unwrap_or(now.date_naive())
            .and_time(time)
    };
    now.timezone()
        .from_local_datetime(&target)
        .single()
        .unwrap_or(now)
}

pub fn schedule_is_today_later(schedule_time: &str, now: DateTime<Local>) -> bool {
    let (hour, minute) = parse_hhmm(schedule_time);
    let time = NaiveTime::from_hms_opt(hour, minute, 0).unwrap_or(NaiveTime::MIN);
    now.naive_local().time() < time
}

pub fn clock_label(schedule_time: &str) -> String {
    let (hour, minute) = parse_hhmm(schedule_time);
    let suffix = if hour >= 12 { "PM" } else { "AM" };
    let mut display = hour % 12;
    if display == 0 {
        display = 12;
    }
    format!("{display}:{minute:02} {suffix}")
}

pub fn log_line(data_dir: &Path, message: &str) {
    use std::io::Write;
    let dir = data_dir.join("logs");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("mothball.log"))
    {
        let stamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(file, "{stamp} {message}");
    }
}

pub fn hidden_command(program: &str) -> std::process::Command {
    let mut command = {
        #[cfg(windows)]
        {
            let mut command = std::process::Command::new("cmd");
            command.arg("/D").arg("/S").arg("/C").arg(program);
            command
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new(program)
        }
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_match_the_dashboard_style() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(300 * 1024 * 1024), "300 MB");
        assert_eq!(format_bytes(640 * 1024 * 1024), "640 MB");
        let gb = (8.2 * 1024.0 * 1024.0 * 1024.0) as u64;
        assert_eq!(format_bytes(gb), "8.2 GB");
    }

    #[test]
    fn safe_join_rejects_escape() {
        let project = Path::new(r"C:\dev\app");
        assert!(safe_join(project, r"..\outside").is_none());
        assert!(safe_join(project, "/etc/passwd").is_none());
        assert!(safe_join(project, "").is_none());
        let joined = safe_join(project, "node_modules").unwrap();
        assert!(is_strict_child(project, &joined));
    }
}
