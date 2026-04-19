use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use crust::style;

use crate::entry::format_size;
use crate::highlight;

const LARGE_FILE_HIGHLIGHT_LIMIT: u64 = 1_000_000;  // 1MB: skip highlighting
const LARGE_FILE_PREVIEW_LIMIT: u64 = 10_000_000;   // 10MB: show size only

/// Thread-safe preview cache: path -> (content, max_lines used)
pub type PreviewCache = Arc<Mutex<HashMap<PathBuf, (String, usize)>>>;

pub fn new_cache() -> PreviewCache {
    Arc::new(Mutex::new(HashMap::with_capacity(8)))
}

pub fn clear_cache(cache: &PreviewCache) {
    if let Ok(mut c) = cache.lock() { c.clear(); }
}

/// Pre-generate previews for adjacent files (call from background)
pub fn preload_adjacent(paths: &[PathBuf], max_lines: usize, use_bat: bool, show_hidden: bool, cache: &PreviewCache) {
    for path in paths {
        let key = path.clone();
        // Skip if already cached
        if let Ok(c) = cache.lock() {
            if c.contains_key(&key) { continue; }
        }
        let content = preview(path, max_lines, use_bat, show_hidden);
        if let Ok(mut c) = cache.lock() {
            c.insert(key, (content, max_lines));
        }
    }
}

/// Get preview from cache, or generate and cache it
pub fn preview_cached(path: &Path, max_lines: usize, use_bat: bool, show_hidden: bool, cache: &PreviewCache) -> String {
    let key = path.to_path_buf();
    if let Ok(c) = cache.lock() {
        if let Some((content, cached_lines)) = c.get(&key) {
            if *cached_lines >= max_lines {
                return content.clone();
            }
        }
    }
    let content = preview(path, max_lines, use_bat, show_hidden);
    if let Ok(mut c) = cache.lock() {
        // Keep cache small
        if c.len() > 16 { c.clear(); }
        c.insert(key, (content.clone(), max_lines));
    }
    content
}

/// Generate preview content for right pane
pub fn preview(path: &Path, max_lines: usize, use_bat: bool, show_hidden: bool) -> String {
    if path.is_dir() {
        return preview_dir(path, show_hidden);
    }
    preview_file(path, max_lines, use_bat)
}

fn preview_dir(path: &Path, show_hidden: bool) -> String {
    // Use ls with same sorting as left pane: dirs first, alphabetical, LS_COLORS
    let mut args = vec!["--color=always", "-1", "--group-directories-first", "-p"];
    if show_hidden {
        args.push("-a");
    }
    let output = Command::new("ls")
        .args(&args)
        .arg(path)
        .output();
    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).to_string()
        }
        _ => {
            let Ok(entries) = fs::read_dir(path) else {
                return "Cannot read directory".into();
            };
            let mut items: Vec<String> = entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            items.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
            items.join("\n")
        }
    }
}

fn preview_file(path: &Path, max_lines: usize, use_bat: bool) -> String {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let ext_lower = ext.to_lowercase();

    // HyperList: use dedicated highlighter
    if ext_lower == "hl" {
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > LARGE_FILE_PREVIEW_LIMIT {
                return format!("{}\n\n{}\nSize: {}",
                    style::bold(&path.file_name().unwrap_or_default().to_string_lossy()),
                    style::fg("[File too large for preview]", 245),
                    format_size(meta.len()));
            }
        }
        if use_bat {
            if let Some(highlighted) = bat_preview(path, max_lines) {
                return highlighted;
            }
        }
        if let Ok(bytes) = fs::read(path) {
            let content = String::from_utf8_lossy(&bytes).replace('\t', "   ");
            return highlight::highlight_hyperlist(&content, max_lines);
        }
    }

    // Markdown: dedicated highlighter (headers, bold, italic, code, links, lists).
    if ext_lower == "md" || ext_lower == "markdown" {
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > LARGE_FILE_PREVIEW_LIMIT {
                return format!("{}\n\n{}\nSize: {}",
                    style::bold(&path.file_name().unwrap_or_default().to_string_lossy()),
                    style::fg("[File too large for preview]", 245),
                    format_size(meta.len()));
            }
        }
        if use_bat {
            if let Some(highlighted) = bat_preview(path, max_lines) {
                return highlighted;
            }
        }
        if let Ok(bytes) = fs::read(path) {
            let content = String::from_utf8_lossy(&bytes);
            return highlight::highlight_markdown(&content, max_lines);
        }
    }

    // LaTeX/TeX: dedicated highlighter (commands, envs, comments, math).
    if matches!(ext_lower.as_str(), "tex" | "latex" | "ltx" | "sty" | "cls" | "bib") {
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > LARGE_FILE_PREVIEW_LIMIT {
                return format!("{}\n\n{}\nSize: {}",
                    style::bold(&path.file_name().unwrap_or_default().to_string_lossy()),
                    style::fg("[File too large for preview]", 245),
                    format_size(meta.len()));
            }
        }
        if use_bat {
            if let Some(highlighted) = bat_preview(path, max_lines) {
                return highlighted;
            }
        }
        if let Ok(bytes) = fs::read(path) {
            let content = String::from_utf8_lossy(&bytes);
            return highlight::highlight_tex(&content, max_lines);
        }
    }

    // Plain text (.txt, .log): URL/email/TODO highlighting.
    if matches!(ext_lower.as_str(), "txt" | "log" | "readme") {
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > LARGE_FILE_PREVIEW_LIMIT {
                return format!("{}\n\n{}\nSize: {}",
                    style::bold(&path.file_name().unwrap_or_default().to_string_lossy()),
                    style::fg("[File too large for preview]", 245),
                    format_size(meta.len()));
            }
        }
        if use_bat {
            if let Some(highlighted) = bat_preview(path, max_lines) {
                return highlighted;
            }
        }
        if let Ok(bytes) = fs::read(path) {
            let content = String::from_utf8_lossy(&bytes);
            return highlight::highlight_text(&content, max_lines);
        }
    }

    // Text files: internal highlighter (fast) or bat (toggled with 'b')
    if is_text_ext(ext) || ext.is_empty() {
        // Large file guard
        if let Ok(meta) = fs::metadata(path) {
            let size = meta.len();
            if size > LARGE_FILE_PREVIEW_LIMIT {
                return format!("{}\n\n{}\nSize: {}",
                    style::bold(&path.file_name().unwrap_or_default().to_string_lossy()),
                    style::fg("[File too large for preview]", 245),
                    format_size(size));
            }
            if use_bat {
                // 'b' toggled: use external bat
                if let Some(highlighted) = bat_preview(path, max_lines) {
                    return highlighted;
                }
            }
            // Internal highlighter (default, zero-spawn)
            if size <= LARGE_FILE_HIGHLIGHT_LIMIT {
                return highlighted_preview(path, ext, max_lines);
            }
            // >1MB: plain text, no highlighting
            return text_preview(path, max_lines);
        }
    }

    // PDF preview
    if ext_lower == "pdf" {
        return pdf_preview(path);
    }

    // LibreOffice formats: odt, odp, odg, ods
    if matches!(ext_lower.as_str(), "odt" | "odp" | "odg" | "ods" | "odc") {
        if let Some(s) = run_preview_cmd("odt2txt", &[path.as_os_str().to_str().unwrap_or("")]) {
            return s;
        }
        return format!("{}\n\nInstall odt2txt for preview", style::bold("[LibreOffice document]"));
    }

    // MS Word docx
    if ext_lower == "docx" {
        if let Some(s) = shell_preview(&format!("docx2txt {:?} - 2>/dev/null | tr -d '\\r'", path)) {
            return s;
        }
        return format!("{}\n\nInstall docx2txt for preview", style::bold("[Word document]"));
    }

    // MS Excel xlsx
    if ext_lower == "xlsx" {
        if let Some(s) = shell_preview(&format!(
            "ssconvert -O 'separator=\t' -T Gnumeric_stf:stf_assistant {:?} fd://1 2>/dev/null", path)) {
            return s;
        }
        return format!("{}\n\nInstall ssconvert (gnumeric) for preview", style::bold("[Excel spreadsheet]"));
    }

    // MS PowerPoint pptx
    if ext_lower == "pptx" {
        if let Some(s) = shell_preview(&format!(
            "unzip -qc {:?} 2>/dev/null | grep -oP '(?<=<a:t>).*?(?=</a:t>)'", path)) {
            return s;
        }
        return format!("{}\n\nInstall unzip for preview", style::bold("[PowerPoint]"));
    }

    // Legacy MS Office: doc, xls, ppt
    if ext_lower == "doc" {
        if let Some(s) = shell_preview(&format!(
            "catdoc {:?} 2>/dev/null || soffice --headless --cat {:?} 2>/dev/null", path, path)) {
            return s;
        }
    }
    if ext_lower == "xls" {
        if let Some(s) = shell_preview(&format!(
            "xls2csv {:?} 2>/dev/null || soffice --headless --cat {:?} 2>/dev/null", path, path)) {
            return s;
        }
    }
    if ext_lower == "ppt" {
        if let Some(s) = shell_preview(&format!(
            "catppt {:?} 2>/dev/null || soffice --headless --cat {:?} 2>/dev/null", path, path)) {
            return s;
        }
    }

    // EPUB
    if ext_lower == "epub" {
        if let Some(s) = shell_preview(&format!(
            "unzip -qc {:?} 2>/dev/null | grep -oP '(?<=>)[^<]+' | head -200", path)) {
            return s;
        }
    }

    // JSON (pretty print with jq)
    if ext_lower == "json" {
        if let Some(s) = shell_preview(&format!("jq -C . {:?} 2>/dev/null", path)) {
            return s;
        }
    }

    // XML (xmllint pretty print)
    if ext_lower == "xml" {
        if let Some(s) = shell_preview(&format!("xmllint --format {:?} 2>/dev/null", path)) {
            return s;
        }
    }

    // Video/audio metadata
    if matches!(ext_lower.as_str(), "mp4" | "mkv" | "avi" | "mov" | "webm" | "mp3" | "flac" | "ogg" | "wav" | "m4a") {
        if let Some(s) = shell_preview(&format!("ffprobe -hide_banner {:?} 2>&1 | head -30", path)) {
            return s;
        }
        if let Some(s) = shell_preview(&format!("mediainfo {:?} 2>/dev/null | head -40", path)) {
            return s;
        }
    }

    // Archive listing
    if is_archive_ext(&ext_lower) {
        return archive_listing(path, ext);
    }

    // Image: show info (actual image display handled by image.rs)
    if is_image_ext(ext) {
        let meta = fs::metadata(path).ok();
        return format!(
            "{}\n\n{}\nSize: {}",
            style::bold(&path.file_name().unwrap_or_default().to_string_lossy()),
            style::fg("[Image file]", 81),
            format_size(meta.as_ref().map(|m| m.len()).unwrap_or(0)),
        );
    }

    // Try MIME-based detection as last resort
    let mime = mime_type(path);
    if mime.starts_with("text/") {
        return highlighted_preview(path, ext, max_lines);
    }

    // Binary / unknown: basic info
    let meta = fs::metadata(path).ok();
    format!(
        "{}\n\nSize: {}\nType: {}",
        style::bold(&path.file_name().unwrap_or_default().to_string_lossy()),
        format_size(meta.as_ref().map(|m| m.len()).unwrap_or(0)),
        if mime.is_empty() { ext.to_string() } else { mime },
    )
}

/// Run a command directly, return stdout if successful and non-empty
fn run_preview_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(cmd).args(args).output().ok()?;
    if output.status.success() && !output.stdout.is_empty() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

/// Run a shell command, return stdout if successful and non-empty
fn shell_preview(cmd: &str) -> Option<String> {
    let output = Command::new("sh").arg("-c").arg(cmd).output().ok()?;
    if !output.stdout.is_empty() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

/// Detect language from shebang line (e.g. #!/usr/bin/env ruby -> "rb")
fn detect_shebang(first_line: &str) -> Option<&'static str> {
    let line = first_line.trim();
    if !line.starts_with("#!") { return None; }
    let cmd = line.rsplit('/').next().unwrap_or("");
    // Strip "env " prefix
    let cmd = cmd.strip_prefix("env ").unwrap_or(cmd);
    // Strip version suffixes (python3 -> python, ruby3.2 -> ruby)
    let base = cmd.split(|c: char| c.is_ascii_digit() || c == '.').next().unwrap_or(cmd);
    match base {
        "ruby" => Some("rb"),
        "python" => Some("py"),
        "perl" => Some("pl"),
        "node" | "nodejs" | "deno" | "bun" => Some("js"),
        "bash" | "sh" | "zsh" | "fish" | "dash" => Some("sh"),
        "lua" => Some("lua"),
        "php" => Some("py"),  // close enough highlighting
        _ => None,
    }
}

fn highlighted_preview(path: &Path, ext: &str, max_lines: usize) -> String {
    match fs::read(path) {
        Ok(bytes) => {
            if bytes.iter().take(512).any(|&b| b == 0) {
                let meta = fs::metadata(path).ok();
                return format!("{}\n\n{}\nSize: {}",
                    style::bold(&path.file_name().unwrap_or_default().to_string_lossy()),
                    style::fg("[Binary file]", 245),
                    format_size(meta.as_ref().map(|m| m.len()).unwrap_or(0)));
            }
            let content = String::from_utf8_lossy(&bytes);
            // Use extension, or detect from shebang
            let lang = if ext.is_empty() || highlight::lang_known(&ext.to_lowercase()).is_none() {
                content.lines().next()
                    .and_then(detect_shebang)
                    .unwrap_or(ext)
            } else {
                ext
            };
            highlight::highlight(&content, &lang.to_lowercase(), max_lines)
        }
        Err(e) => format!("Error: {}", e),
    }
}

fn text_preview(path: &Path, max_lines: usize) -> String {
    // Quick binary check: read first 512 bytes only
    if let Ok(mut f) = fs::File::open(path) {
        use std::io::Read;
        let mut header = [0u8; 512];
        if let Ok(n) = f.read(&mut header) {
            if header[..n].iter().any(|&b| b == 0) {
                let meta = fs::metadata(path).ok();
                return format!("{}\n\n{}\nSize: {}",
                    style::bold(&path.file_name().unwrap_or_default().to_string_lossy()),
                    style::fg("[Binary file]", 245),
                    format_size(meta.as_ref().map(|m| m.len()).unwrap_or(0)));
            }
        }
    }
    match fs::read(path) {
        Ok(bytes) => {
            let content = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = content.lines().take(max_lines).collect();
            let truncated = content.lines().count() > max_lines;
            let mut result = lines.join("\n");
            if truncated {
                result.push_str(&format!("\n{}", style::fg("...", 245)));
            }
            result
        }
        Err(e) => format!("Error: {}", e),
    }
}

fn bat_preview(path: &Path, max_lines: usize) -> Option<String> {
    let bat = find_bat()?;
    let output = Command::new(&bat)
        .args(["--color=always", "--style=plain", "--paging=never",
               "--tabs=8", "--line-range", &format!("1:{}", max_lines)])
        .arg(path)
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).replace('\r', "");
        Some(s)
    } else {
        None
    }
}

fn find_bat() -> Option<String> {
    for name in ["bat", "batcat"] {
        if Command::new("which").arg(name).output()
            .map(|o| o.status.success()).unwrap_or(false)
        {
            return Some(name.into());
        }
    }
    None
}

fn pdf_preview(path: &Path) -> String {
    let output = Command::new("pdftotext")
        .args(["-f", "1", "-l", "4"])
        .arg(path)
        .arg("-")
        .output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => format!("{}\n\n{}", style::bold("[PDF]"), "Install pdftotext for preview"),
    }
}

fn archive_listing(path: &Path, ext: &str) -> String {
    let cmd = match ext.to_lowercase().as_str() {
        "zip" | "jar" | "war" => format!("unzip -l {:?}", path),
        "tar" => format!("tar -tvf {:?}", path),
        "gz" | "tgz" => format!("tar -tzvf {:?}", path),
        "bz2" | "tbz2" => format!("tar -tjvf {:?}", path),
        "xz" | "txz" => format!("tar -tJvf {:?}", path),
        "rar" => format!("unrar l {:?}", path),
        "7z" => format!("7z l {:?}", path),
        _ => return format!("[Archive: {}]", ext),
    };
    let output = Command::new("sh").arg("-c").arg(&cmd).output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => format!("[Cannot list archive: {}]", ext),
    }
}

fn mime_type(path: &Path) -> String {
    Command::new("file")
        .args(["--mime-type", "-b"])
        .arg(path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn is_text_ext(ext: &str) -> bool {
    matches!(ext.to_lowercase().as_str(),
        "txt" | "md" | "rs" | "rb" | "py" | "js" | "ts" | "tsx" | "jsx" |
        "sh" | "bash" | "zsh" | "fish" | "toml" | "yaml" | "yml" | "json" |
        "xml" | "html" | "htm" | "css" | "scss" | "less" | "c" | "h" |
        "cpp" | "hpp" | "cc" | "go" | "java" | "lua" | "vim" | "conf" |
        "cfg" | "ini" | "log" | "csv" | "hl" | "gemspec" | "lock" |
        "makefile" | "dockerfile" | "cmake" | "gradle" | "pl" | "pm" |
        "r" | "sql" | "diff" | "patch" | "zig" | "nim" | "el" | "lisp" |
        "clj" | "ex" | "exs" | "erl" | "hs" | "ml" | "mli" | "swift" |
        "kt" | "kts" | "scala" | "v" | "sv" | "vhd" | "tcl" | "rkt" |
        "xrpn" | "tex" | "latex" | "ltx" | "sty" | "cls" | "bib" |
        "markdown" | "readme" | ""
    )
}

pub fn is_image_ext(ext: &str) -> bool {
    matches!(ext.to_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" |
        "tiff" | "tif" | "avif" | "heic" | "heif"
    )
}

pub fn is_archive_ext(ext: &str) -> bool {
    matches!(ext.to_lowercase().as_str(),
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "tbz2" | "xz" | "txz" |
        "rar" | "7z" | "jar" | "war"
    )
}
