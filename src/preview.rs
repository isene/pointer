use std::fs;
use std::path::Path;
use std::process::Command;
use crust::style;

use crate::entry::format_size;

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

    // Markdown via pandoc
    if ext_lower == "md" {
        if let Some(s) = run_preview_cmd("pandoc", &[path.as_os_str().to_str().unwrap_or(""), "-t", "plain"]) {
            return s;
        }
    }

    // Try bat for syntax highlighting (text files)
    if use_bat && is_text_ext(ext) {
        if let Some(highlighted) = bat_preview(path, max_lines) {
            return highlighted;
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

    // Text file preview
    if is_text_ext(ext) || ext.is_empty() {
        return text_preview(path, max_lines);
    }

    // Try MIME-based detection as last resort
    let mime = mime_type(path);
    if mime.starts_with("text/") {
        return text_preview(path, max_lines);
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

fn text_preview(path: &Path, max_lines: usize) -> String {
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
               "--line-range", &format!("1:{}", max_lines)])
        .arg(path)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
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
        ""
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
