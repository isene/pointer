use crust::{Crust, Pane, Input};
use crust::style;
use crust::pane::Align;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

struct App {
    top: Pane,
    left: Pane,
    right: Pane,
    status: Pane,
    files: Vec<DirEntry>,
    index: usize,
    cols: u16,
    rows: u16,
}

struct DirEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    is_symlink: bool,
    is_exec: bool,
    size: u64,
}

fn main() {
    Crust::init();
    let (cols, rows) = Crust::terminal_size();

    let mut app = App {
        top: Pane::new(1, 1, cols, 1, 252, 236),
        left: Pane::new(1, 2, cols / 2, rows - 2, 255, 0),
        right: Pane::new(cols / 2 + 1, 2, cols - cols / 2, rows - 2, 252, 0),
        status: Pane::new(1, rows, cols, 1, 0, 252),
        files: Vec::new(),
        index: 0,
        cols,
        rows,
    };

    app.left.border = true;
    app.left.border_refresh();
    app.right.border = true;
    app.right.border_refresh();

    app.load_dir();
    app.render();

    loop {
        if let Some(key) = Input::getchr(None) {
            match key.as_str() {
                "q" | "ESC" => break,
                "j" | "DOWN" => {
                    if app.index < app.files.len().saturating_sub(1) {
                        app.index += 1;
                        app.render();
                    }
                }
                "k" | "UP" => {
                    if app.index > 0 {
                        app.index -= 1;
                        app.render();
                    }
                }
                "ENTER" | "l" | "RIGHT" => {
                    if let Some(entry) = app.files.get(app.index) {
                        if entry.is_dir {
                            let path = entry.path.clone();
                            let _ = env::set_current_dir(&path);
                            app.index = 0;
                            app.left.ix = 0;
                            app.load_dir();
                            app.render();
                        }
                    }
                }
                "h" | "LEFT" | "BACK" => {
                    // Go up one directory
                    if let Ok(cwd) = env::current_dir() {
                        if let Some(parent) = cwd.parent() {
                            let _ = env::set_current_dir(parent);
                            app.index = 0;
                            app.left.ix = 0;
                            app.load_dir();
                            app.render();
                        }
                    }
                }
                "g" => {
                    app.index = 0;
                    app.left.ix = 0;
                    app.render();
                }
                "G" => {
                    app.index = app.files.len().saturating_sub(1);
                    app.render();
                }
                "PgDOWN" | " " => {
                    let page = (app.rows - 4) as usize;
                    app.index = (app.index + page).min(app.files.len().saturating_sub(1));
                    app.render();
                }
                "PgUP" => {
                    let page = (app.rows - 4) as usize;
                    app.index = app.index.saturating_sub(page);
                    app.render();
                }
                "RESIZE" => {
                    let (cols, rows) = Crust::terminal_size();
                    app.cols = cols;
                    app.rows = rows;
                    app.resize();
                    app.render();
                }
                _ => {}
            }
        }
    }

    Crust::cleanup();
}

impl App {
    fn load_dir(&mut self) {
        self.files.clear();
        let cwd = env::current_dir().unwrap_or_default();

        // Add parent directory entry
        if cwd.parent().is_some() {
            self.files.push(DirEntry {
                name: "..".to_string(),
                path: cwd.join(".."),
                is_dir: true,
                is_symlink: false,
                is_exec: false,
                size: 0,
            });
        }

        if let Ok(entries) = fs::read_dir(&cwd) {
            let mut items: Vec<DirEntry> = entries
                .flatten()
                .map(|e| {
                    let meta = e.metadata().ok();
                    let is_symlink = e.file_type().map(|t| t.is_symlink()).unwrap_or(false);
                    let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    let is_exec = meta.as_ref()
                        .map(|m| m.permissions().mode() & 0o111 != 0 && m.is_file())
                        .unwrap_or(false);
                    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    DirEntry {
                        name: e.file_name().to_string_lossy().to_string(),
                        path: e.path(),
                        is_dir,
                        is_symlink,
                        is_exec,
                        size,
                    }
                })
                .collect();

            // Sort: dirs first, then files, alphabetical
            items.sort_by(|a, b| {
                match (a.is_dir, b.is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                }
            });

            self.files.extend(items);
        }
    }

    fn render(&mut self) {
        // Top bar: current directory
        let cwd = env::current_dir().unwrap_or_default();
        let home = dirs_home().unwrap_or_default();
        let display_path = if cwd.starts_with(&home) {
            format!("~/{}", cwd.strip_prefix(&home).unwrap().display())
        } else {
            cwd.display().to_string()
        };
        self.top.say(&format!(" {} | {} items", style::bold(&display_path), self.files.len()));

        // Left pane: file listing
        let content_h = (self.left.h - if self.left.border { 2 } else { 0 }) as usize;
        let mut lines = Vec::new();

        for (i, entry) in self.files.iter().enumerate() {
            let mut name = entry.name.clone();
            if entry.is_dir && name != ".." {
                name.push('/');
            } else if entry.is_symlink {
                name.push('@');
            } else if entry.is_exec {
                name.push('*');
            }

            let colored = if i == self.index {
                style::reverse(&style::bold(&name))
            } else if entry.is_dir {
                style::fg(&name, 111)
            } else if entry.is_symlink {
                style::fg(&name, 248)
            } else if entry.is_exec {
                style::fg(&name, 46)
            } else {
                name.clone()
            };
            lines.push(colored);
        }

        // Auto-scroll to keep selection visible
        if self.index >= self.left.ix + content_h {
            self.left.ix = self.index - content_h + 1;
        }
        if self.index < self.left.ix {
            self.left.ix = self.index;
        }

        self.left.set_text(&lines.join("\n"));
        self.left.refresh();

        // Right pane: preview of selected file/dir
        if let Some(entry) = self.files.get(self.index) {
            let preview = if entry.is_dir {
                preview_dir(&entry.path)
            } else {
                preview_file(&entry.path, (self.right.h - 2) as usize)
            };
            self.right.set_text(&preview);
            self.right.ix = 0;
            self.right.refresh();
        }

        // Status bar
        let info = if let Some(entry) = self.files.get(self.index) {
            format!(" {} | {} | {}/{}",
                entry.name,
                format_size(entry.size),
                self.index + 1,
                self.files.len()
            )
        } else {
            " Empty".to_string()
        };
        self.status.say(&info);
    }

    fn resize(&mut self) {
        self.top = Pane::new(1, 1, self.cols, 1, 252, 236);
        self.left = Pane::new(1, 2, self.cols / 2, self.rows - 2, 255, 0);
        self.left.border = true;
        self.right = Pane::new(self.cols / 2 + 1, 2, self.cols - self.cols / 2, self.rows - 2, 252, 0);
        self.right.border = true;
        self.status = Pane::new(1, self.rows, self.cols, 1, 0, 252);
        Crust::clear_screen();
        self.left.border_refresh();
        self.right.border_refresh();
    }
}

fn preview_dir(path: &Path) -> String {
    let mut lines = Vec::new();
    if let Ok(entries) = fs::read_dir(path) {
        let mut items: Vec<String> = entries
            .flatten()
            .map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    style::fg(&format!("{}/", name), 111)
                } else {
                    name
                }
            })
            .collect();
        items.sort();
        for item in items.iter().take(100) {
            lines.push(item.clone());
        }
        if items.len() > 100 {
            lines.push(style::fg(&format!("... and {} more", items.len() - 100), 245));
        }
    }
    lines.join("\n")
}

fn preview_file(path: &Path, max_lines: usize) -> String {
    // Check if text file
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let is_text = matches!(ext,
        "txt" | "md" | "rs" | "rb" | "py" | "js" | "ts" | "sh" | "bash" |
        "toml" | "yaml" | "yml" | "json" | "xml" | "html" | "css" |
        "c" | "h" | "cpp" | "go" | "java" | "lua" | "vim" | "conf" |
        "cfg" | "ini" | "log" | "csv" | "hl" | "gemspec" | "lock" | ""
    );

    if !is_text {
        // Binary file: show basic info
        let meta = fs::metadata(path).ok();
        return format!(
            "{}\n\nSize: {}\nType: {}",
            style::bold(&path.file_name().unwrap_or_default().to_string_lossy()),
            format_size(meta.as_ref().map(|m| m.len()).unwrap_or(0)),
            ext
        );
    }

    match fs::read_to_string(path) {
        Ok(content) => {
            let mut lines: Vec<&str> = content.lines().take(max_lines).collect();
            if content.lines().count() > max_lines {
                lines.push("...");
            }
            lines.join("\n")
        }
        Err(e) => format!("Error: {}", e),
    }
}

fn format_size(bytes: u64) -> String {
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

fn dirs_home() -> Option<PathBuf> {
    env::var("HOME").ok().map(PathBuf::from)
}
