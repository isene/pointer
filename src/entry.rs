use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub is_exec: bool,
    pub size: u64,
    pub modified: SystemTime,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub color_code: String,
    pub tagged: bool,
    pub search_hit: bool,
}

#[derive(Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SortMode { Name, Size, Time, Extension }

impl SortMode {
    pub fn label(&self) -> &str {
        match self {
            SortMode::Name => "name",
            SortMode::Size => "size",
            SortMode::Time => "time",
            SortMode::Extension => "ext",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "size" => SortMode::Size,
            "time" => SortMode::Time,
            "ext" | "extension" => SortMode::Extension,
            _ => SortMode::Name,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            SortMode::Name => SortMode::Size,
            SortMode::Size => SortMode::Time,
            SortMode::Time => SortMode::Extension,
            SortMode::Extension => SortMode::Name,
        }
    }
}

/// Parse LS_COLORS env var into a map
pub fn parse_ls_colors() -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(val) = std::env::var("LS_COLORS") {
        for entry in val.split(':') {
            if let Some((key, code)) = entry.split_once('=') {
                map.insert(key.to_string(), code.to_string());
            }
        }
    }
    map
}

/// Get ANSI color code for a DirEntry
pub fn color_for(entry: &DirEntry, ls_colors: &HashMap<String, String>) -> String {
    if entry.is_symlink {
        if let Some(code) = ls_colors.get("ln") {
            return format!("\x1b[{}m", code);
        }
        return "\x1b[38;5;14m".into();
    }
    if entry.is_dir {
        if let Some(code) = ls_colors.get("di") {
            return format!("\x1b[{}m", code);
        }
        return "\x1b[38;5;12m".into();
    }
    // Check by extension
    if let Some(ext) = entry.path.extension().and_then(|e| e.to_str()) {
        let key = format!("*.{}", ext);
        if let Some(code) = ls_colors.get(&key) {
            return format!("\x1b[{}m", code);
        }
        // Try lowercase
        let key_lower = format!("*.{}", ext.to_lowercase());
        if let Some(code) = ls_colors.get(&key_lower) {
            return format!("\x1b[{}m", code);
        }
    }
    if entry.is_exec {
        if let Some(code) = ls_colors.get("ex") {
            return format!("\x1b[{}m", code);
        }
        return "\x1b[38;5;10m".into();
    }
    String::new()
}

/// Load directory entries
pub fn load_dir(
    dir: &Path,
    show_hidden: bool,
    sort_mode: SortMode,
    sort_invert: bool,
    ls_colors: &HashMap<String, String>,
    tagged_paths: &[PathBuf],
) -> Vec<DirEntry> {
    let mut entries = Vec::new();

    let Ok(read_dir) = fs::read_dir(dir) else { return entries };

    let mut items: Vec<DirEntry> = read_dir
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            let ft = e.file_type().ok()?;
            let is_symlink = ft.is_symlink();
            // Follow symlinks for metadata
            let meta = fs::metadata(e.path()).or_else(|_| e.metadata()).ok()?;
            let is_dir = meta.is_dir();
            let is_exec = !is_dir && meta.permissions().mode() & 0o111 != 0;
            let path = e.path();
            let tagged = tagged_paths.contains(&path);
            let mut entry = DirEntry {
                name,
                path,
                is_dir,
                is_symlink,
                is_exec,
                size: meta.len(),
                modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                mode: meta.permissions().mode(),
                uid: meta.uid(),
                gid: meta.gid(),
                color_code: String::new(),
                tagged,
                search_hit: false,
            };
            entry.color_code = color_for(&entry, ls_colors);
            Some(entry)
        })
        .collect();

    // Sort: dirs first, then by sort mode
    items.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        let ord = match sort_mode {
            SortMode::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortMode::Size => a.size.cmp(&b.size).reverse(),
            SortMode::Time => a.modified.cmp(&b.modified).reverse(),
            SortMode::Extension => {
                let ea = a.path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let eb = b.path.extension().and_then(|e| e.to_str()).unwrap_or("");
                ea.to_lowercase().cmp(&eb.to_lowercase())
            }
        };
        if sort_invert { ord.reverse() } else { ord }
    });

    entries.extend(items);
    entries
}

/// Format file size for display
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1}G", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Format permissions as rwxrwxrwx string
pub fn format_mode(mode: u32) -> String {
    let mut s = String::with_capacity(10);
    let flags = [
        (0o400, 'r'), (0o200, 'w'), (0o100, 'x'),
        (0o040, 'r'), (0o020, 'w'), (0o010, 'x'),
        (0o004, 'r'), (0o002, 'w'), (0o001, 'x'),
    ];
    for (bit, ch) in flags {
        s.push(if mode & bit != 0 { ch } else { '-' });
    }
    s
}

/// Format SystemTime as "YYYY-MM-DD HH:MM"
pub fn format_time(time: SystemTime) -> String {
    let dur = time.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs() as i64;
    // Simple UTC conversion
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;

    // Calculate date from days since epoch
    let (y, m, d) = days_to_ymd(days);
    format!("{:04}-{:02}-{:02} {:02}:{:02}", y, m, d, hours, minutes)
}

fn days_to_ymd(days: i64) -> (i64, i64, i64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
