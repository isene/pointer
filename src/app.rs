use crust::{Crust, Pane};
use crust::style;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::config::{Config, State};
use crate::entry::{self, DirEntry, SortMode, format_size, format_mode, format_time};
use crate::preview;
use crate::tabs::Tab;
use crate::undo::UndoOp;

/// Shared state for async file operations
pub struct FileOpState {
    pub progress: String,
    pub complete: bool,
    pub result_msg: Option<String>,
    pub result_ok: bool,
    pub undo_op: Option<UndoOp>,
}

pub struct App {
    pub top: Pane,
    pub left: Pane,
    pub right: Pane,
    pub status: Pane,
    pub cols: u16,
    pub rows: u16,
    pub config: Config,
    pub state: State,
    pub files: Vec<DirEntry>,
    pub index: usize,
    pub scroll_ix: usize,
    pub tagged: Vec<PathBuf>,
    pub tagged_size_cache: Option<u64>,
    pub ls_colors: HashMap<String, String>,
    pub sort_mode: SortMode,
    pub sort_invert: bool,
    pub show_hidden: bool,
    pub long_format: bool,
    pub search_term: String,
    pub filter_ext: String,
    pub filter_regex: String,
    pub undo_stack: Vec<UndoOp>,
    pub prev_selected: Option<PathBuf>,
    pub top_extra_cache: Option<(PathBuf, String)>,
    pub tabs: Vec<Tab>,
    pub current_tab: usize,
    pub image_display: Option<glow::Display>,
    pub archive_state: Option<crate::archive::ArchiveState>,
    pub file_op: Arc<Mutex<FileOpState>>,
    pub file_op_thread: Option<std::thread::JoinHandle<()>>,
    pub pick_output: Option<String>,
    pub locate_active: bool,
    pub ssh_state: Option<crate::ssh::SshState>,
    pub preview_cache: crate::preview::PreviewCache,
    pub dir_mtime: Option<std::time::SystemTime>,
    pub preload_busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub right_pane_locked: bool,
}

impl App {
    pub fn new() -> Self {
        let config = Config::load();
        crate::highlight::set_theme(&config.syntax_theme);
        let state = State::load();
        let (cols, rows) = Crust::terminal_size();
        let ls_colors = entry::parse_ls_colors();
        let sort_mode = SortMode::from_str(&config.sort_mode);
        let show_hidden = config.show_hidden;
        let long_format = config.long_format;
        let sort_invert = config.sort_invert;

        // Panes will be properly set up by rebuild_panes() below
        let mut app = App {
            top: Pane::new(1, 1, cols, 1, config.c_top_fg, config.c_top_bg),
            left: Pane::new(1, 2, 1, 1, 15, 0),
            right: Pane::new(2, 2, 1, 1, 255, 0),
            status: Pane::new(1, rows, cols, 1, config.c_status_fg, config.c_status_bg),
            cols,
            rows,
            config,
            state,
            files: Vec::new(),
            index: 0,
            scroll_ix: 0,
            tagged: Vec::new(),
            tagged_size_cache: None,
            ls_colors,
            sort_mode,
            sort_invert,
            show_hidden,
            long_format,
            search_term: String::new(),
            filter_ext: String::new(),
            filter_regex: String::new(),
            undo_stack: Vec::new(),
            prev_selected: None,
            top_extra_cache: None,
            tabs: Vec::new(),
            current_tab: 0,
            image_display: Some(glow::Display::new()),
            archive_state: None,
            file_op: Arc::new(Mutex::new(FileOpState {
                progress: String::new(),
                complete: false,
                result_msg: None,
                result_ok: true,
                undo_op: None,
            })),
            file_op_thread: None,
            pick_output: None,
            locate_active: false,
            ssh_state: None,
            preview_cache: crate::preview::new_cache(),
            dir_mtime: None,
            preload_busy: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            right_pane_locked: false,
        };

        app.rebuild_panes();

        // Restore last dir index
        let cwd = env::current_dir().unwrap_or_default();
        if let Some(&idx) = app.state.dir_index.get(&cwd.to_string_lossy().to_string()) {
            app.index = idx;
        }

        app.load_dir();
        app.init_tabs();
        app
    }

    pub fn load_dir(&mut self) {
        preview::clear_cache(&self.preview_cache);
        let cwd = env::current_dir().unwrap_or_default();
        self.files = entry::load_dir(
            &cwd,
            self.show_hidden,
            self.sort_mode,
            self.sort_invert,
            &self.ls_colors,
            &self.tagged,
        );

        // Apply filters
        if !self.filter_ext.is_empty() {
            let exts: Vec<&str> = self.filter_ext.split(',').map(|s| s.trim()).collect();
            self.files.retain(|e| {
                e.is_dir || {
                    let ext = e.path.extension().and_then(|x| x.to_str()).unwrap_or("");
                    exts.iter().any(|f| f.eq_ignore_ascii_case(ext))
                }
            });
        }
        if !self.filter_regex.is_empty() {
            if let Ok(re) = regex::Regex::new(&self.filter_regex) {
                self.files.retain(|e| e.is_dir || re.is_match(&e.name));
            }
        }

        // Apply search highlights
        if !self.search_term.is_empty() {
            let term = self.search_term.to_lowercase();
            for entry in &mut self.files {
                entry.search_hit = entry.name.to_lowercase().contains(&term);
            }
        }

        // Clamp index
        if self.files.is_empty() {
            self.index = 0;
        } else if self.index >= self.files.len() {
            self.index = self.files.len() - 1;
        }

        // Track dir mtime for idle skip
        self.dir_mtime = std::fs::metadata(&cwd).ok()
            .and_then(|m| m.modified().ok());
    }

    /// Check if directory has changed since last load
    pub fn dir_changed(&self) -> bool {
        let cwd = env::current_dir().unwrap_or_default();
        let current_mtime = std::fs::metadata(&cwd).ok()
            .and_then(|m| m.modified().ok());
        current_mtime != self.dir_mtime
    }

    /// Reload directory listing, preserving selection by name
    pub fn reload_and_render(&mut self) {
        let prev_name = self.files.get(self.index).map(|e| e.name.clone());
        self.load_dir();
        if let Some(name) = prev_name {
            if let Some(pos) = self.files.iter().position(|e| e.name == name) {
                self.index = pos;
            }
        }
        self.render();
    }

    pub fn render(&mut self) {
        // Set window title like RTFM
        let cwd = std::env::current_dir().unwrap_or_default();
        Crust::set_title(&format!("pointer: {}", cwd.display()));

        self.render_top();
        self.render_left();
        if self.config.preview {
            self.render_right();
        }
        self.render_status();
    }

    fn render_top(&mut self) {
        // Apply topmatch: set top bar bg based on current path
        let cwd_str = env::current_dir().unwrap_or_default().to_string_lossy().to_string();
        let top_bg = self.config.topmatch.iter()
            .find(|(pattern, _)| pattern.is_empty() || cwd_str.contains(pattern.as_str()))
            .map(|(_, color)| *color)
            .unwrap_or(self.config.c_top_bg);
        if self.top.bg != top_bg {
            self.top.bg = top_bg;
        }
        // RTFM style: user@host: full_path → symlink_target (owner:group perms size date) [N items]
        // No embedded ANSI codes; let the pane's fg/bg color everything uniformly.
        let user = get_cached_user();
        let host = get_cached_host();

        let info = if let Some(entry) = self.files.get(self.index) {
            let full_path = entry.path.to_string_lossy().to_string();
            let mut path = full_path;

            if entry.is_symlink {
                if let Ok(target) = std::fs::read_link(&entry.path) {
                    path.push_str(&format!(" \u{2192} {}", target.display()));
                }
            }

            let owner = format!("{}:{}", uid_to_name(entry.uid), gid_to_name(entry.gid));
            let perms = format_mode(entry.mode);
            let size = format_size(entry.size);
            let date = format_time(entry.modified);

            let extra = if let Some((ref cached_path, ref cached_extra)) = self.top_extra_cache {
                if *cached_path == entry.path {
                    cached_extra.clone()
                } else {
                    let e = compute_top_extra(entry);
                    self.top_extra_cache = Some((entry.path.clone(), e.clone()));
                    e
                }
            } else {
                let e = compute_top_extra(entry);
                self.top_extra_cache = Some((entry.path.clone(), e.clone()));
                e
            };

            format!(" {}@{}: {} ({} {}  {}  {}){}", user, host, path, owner, perms, size, date, extra)
        } else {
            let cwd = env::current_dir().unwrap_or_default();
            format!(" {}@{}: {}", user, host, cwd.display())
        };

        let tab_info = self.tab_indicator();
        self.top.say(&format!("{}{}", info, tab_info));
    }

    fn render_left(&mut self) {
        let content_h = visible_height(&self.left);
        let mut lines = Vec::new();

        for (i, entry) in self.files.iter().enumerate() {
            let name_part = if self.long_format {
                format_long_entry(entry)
            } else {
                format_short_entry(entry)
            };

            // RTFM style: → prefix + underline for selected, reverse for tagged, both if selected+tagged
            let line = if i == self.index && entry.tagged {
                format!("\u{2192} {}", style::reverse(&style::underline(&name_part)))
            } else if i == self.index {
                format!("\u{2192} {}", style::underline(&name_part))
            } else if entry.tagged {
                format!("  {}", style::reverse(&name_part))
            } else if entry.search_hit && !self.search_term.is_empty() {
                format!("  {}", style::fg(&name_part, 220))
            } else {
                format!("  {}", name_part)
            };
            lines.push(line);
        }

        // Scrolloff=3: keep 3 lines visible above/below cursor (like RTFM)
        let total = self.files.len();
        let scrolloff: usize = 3;
        if total <= content_h {
            self.scroll_ix = 0;
        } else if self.index < self.scroll_ix + scrolloff {
            self.scroll_ix = self.index.saturating_sub(scrolloff);
        } else if self.index + scrolloff >= self.scroll_ix + content_h {
            let max_ix = total.saturating_sub(content_h);
            self.scroll_ix = (self.index + scrolloff + 1).saturating_sub(content_h).min(max_ix);
        }

        self.left.set_text(&lines.join("\n"));
        self.left.ix = self.scroll_ix;
        self.left.refresh();
    }

    fn render_right(&mut self) {
        if self.right_pane_locked { return; }

        let selected_path = self.files.get(self.index).map(|e| e.path.clone());

        // Only re-render if selection changed
        if selected_path == self.prev_selected {
            return;
        }
        self.prev_selected = selected_path.clone();

        // Clear any previous image
        self.clear_image();

        if let Some(path) = selected_path {
            let max_lines = visible_height(&self.right);
            let content = preview::preview_cached(&path, max_lines, self.config.bat, self.show_hidden, &self.preview_cache);
            self.right.set_text(&content);
            self.right.ix = 0;
            self.right.full_refresh();

            // Show image if applicable
            self.show_image_if_applicable();

            // Pre-load adjacent previews in background
            self.preload_adjacent_previews(max_lines);
        }
    }

    fn preload_adjacent_previews(&self, max_lines: usize) {
        use std::sync::atomic::Ordering;
        if self.preload_busy.load(Ordering::Relaxed) { return; }
        let mut paths = Vec::new();
        for offset in [1, 2, -1i32] {
            let idx = self.index as i32 + offset;
            if idx >= 0 && (idx as usize) < self.files.len() {
                let entry = &self.files[idx as usize];
                if !entry.is_dir {
                    paths.push(entry.path.clone());
                }
            }
        }
        if paths.is_empty() { return; }
        let cache = self.preview_cache.clone();
        let use_bat = self.config.bat;
        let show_hidden = self.show_hidden;
        let busy = self.preload_busy.clone();
        busy.store(true, Ordering::Relaxed);
        std::thread::spawn(move || {
            preview::preload_adjacent(&paths, max_lines, use_bat, show_hidden, &cache);
            busy.store(false, Ordering::Relaxed);
        });
    }

    pub fn force_render_right(&mut self) {
        self.clear_image();
        self.prev_selected = None;
        if self.config.preview {
            self.render_right();
        }
    }

    /// Set right pane text (clears image and old content first)
    pub fn show_in_right(&mut self, text: &str) {
        self.clear_image();
        self.right.set_text(text);
        self.right.ix = 0;
        self.right.full_refresh();
        self.right_pane_locked = true;
    }

    fn render_status(&mut self) {
        // RTFM style: filter/tag info + help hint
        let filter_msg = if !self.filter_ext.is_empty() {
            format!("Showing only file type '{}' ", self.filter_ext)
        } else if !self.filter_regex.is_empty() {
            format!("Showing files matching '{}' ", self.filter_regex)
        } else {
            String::new()
        };
        let tag_msg = if !self.tagged.is_empty() {
            let total_size = *self.tagged_size_cache.get_or_insert_with(|| {
                self.tagged.iter()
                    .filter_map(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len())
                    .sum()
            });
            format!("[{} tagged: {}] ", self.tagged.len(), format_size(total_size))
        } else {
            String::new()
        };
        let sort_msg = if self.sort_mode != SortMode::Name || self.sort_invert {
            let inv = if self.sort_invert { " (reversed)" } else { "" };
            format!("[sort: {}{}] ", self.sort_mode.label(), inv)
        } else {
            String::new()
        };
        let left = format!(" {}{}{}: for command (use @s for selected item, @t for tagged items) - press ? for help",
            filter_msg, tag_msg, sort_msg);
        let version = format!("pointer v{}", env!("CARGO_PKG_VERSION"));
        let pad = (self.cols as usize).saturating_sub(crust::display_width(&left) + version.len() + 1);
        self.status.set_text(&format!("{}{}{}", left, " ".repeat(pad), version));
        self.status.full_refresh();
    }

    pub fn resize(&mut self) {
        let (cols, rows) = Crust::terminal_size();
        self.cols = cols;
        self.rows = rows;
        self.rebuild_panes();
    }

    /// Rebuild all panes. Border is drawn OUTSIDE pane area (like rcurses).
    /// Content is always at the same position regardless of border state.
    /// Gap rows 2 and rows-1 are reserved for borders.
    fn rebuild_panes(&mut self) {
        let cols = self.cols;
        let rows = self.rows;
        let ratio = (self.config.width as u16).clamp(2, 7);

        let left_border = matches!(self.config.border, 2 | 3);
        let right_border = matches!(self.config.border, 1 | 2);

        // Content area: rows 3..rows-2, cols 2..split and split+2..cols-1
        // Borders drawn outside into gap rows/cols
        let content_y: u16 = 3;
        let content_h = rows.saturating_sub(4); // rows-4 content rows
        let split = cols * ratio / 10;

        // Left content: col 2..split, right content: col split+3..cols-1
        // Gap: col 1 (left border), col split+1 (left border right), col split+2 (right border left), col cols (right border right)
        let lx: u16 = 2;
        let lw = split.saturating_sub(1);
        let rx = split + 3;
        let rw = cols.saturating_sub(split).saturating_sub(3);

        self.top = Pane::new(1, 1, cols, 1, self.config.c_top_fg, self.config.c_top_bg);
        self.left = Pane::new(lx, content_y, lw, content_h, 15, 0);
        self.left.border = left_border;
        self.right = Pane::new(rx, content_y, rw, content_h, 255, 0);
        self.right.border = right_border;
        self.status = Pane::new(1, rows, cols, 1, self.config.c_status_fg, self.config.c_status_bg);

        Crust::clear_screen();
        if left_border { self.left.border_refresh(); }
        if right_border { self.right.border_refresh(); }
        self.prev_selected = None;
    }

    // --- Navigation ---

    pub fn unlock_right_pane(&mut self) {
        self.right_pane_locked = false;
        self.prev_selected = None; // force re-render on next render_right
    }

    pub fn move_down(&mut self) {
        if self.files.is_empty() { return; }
        self.unlock_right_pane();
        if self.index >= self.files.len() - 1 {
            self.index = 0;
        } else {
            self.index += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.files.is_empty() { return; }
        self.unlock_right_pane();
        if self.index == 0 {
            self.index = self.files.len() - 1;
        } else {
            self.index -= 1;
        }
    }

    pub fn go_top(&mut self) {
        self.unlock_right_pane();
        self.index = 0;
        self.scroll_ix = 0;
    }

    pub fn go_bottom(&mut self) {
        self.unlock_right_pane();
        self.index = self.files.len().saturating_sub(1);
    }

    pub fn page_down(&mut self) {
        self.unlock_right_pane();
        let page = visible_height(&self.left);
        self.index = (self.index + page).min(self.files.len().saturating_sub(1));
    }

    pub fn page_up(&mut self) {
        self.unlock_right_pane();
        let page = visible_height(&self.left);
        self.index = self.index.saturating_sub(page);
    }

    pub fn enter(&mut self) {
        self.unlock_right_pane();
        if self.is_archive_mode() {
            self.archive_enter();
            return;
        }

        let Some(entry) = self.files.get(self.index) else { return };
        if entry.is_dir {
            let path = entry.path.clone();
            self.save_dir_index();
            self.track_recent_dir(&path);
            let _ = env::set_current_dir(&path);
            self.index = 0;
            self.scroll_ix = 0;
            self.prev_selected = None;
            let cwd = env::current_dir().unwrap_or_default();
            if let Some(&idx) = self.state.dir_index.get(&cwd.to_string_lossy().to_string()) {
                self.index = idx;
            }
            self.load_dir();
        } else {
            let ext = entry.path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if preview::is_archive_ext(ext) {
                let path = entry.path.clone();
                self.enter_archive(&path);
                return;
            }
            let path = entry.path.clone();
            self.track_recent_file(&path);
            self.open_file(&path);
        }
    }

    pub fn go_up_dir(&mut self) {
        self.unlock_right_pane();
        if self.is_archive_mode() {
            self.archive_go_up();
            return;
        }

        let cwd = env::current_dir().unwrap_or_default();
        if let Some(parent) = cwd.parent() {
            let prev_dir_name = cwd.file_name().map(|n| n.to_string_lossy().to_string());
            self.save_dir_index();
            self.track_recent_dir(&parent.to_path_buf());
            let _ = env::set_current_dir(parent);
            self.index = 0;
            self.scroll_ix = 0;
            self.prev_selected = None;
            self.load_dir();
            if let Some(name) = prev_dir_name {
                if let Some(pos) = self.files.iter().position(|e| e.name == name) {
                    self.index = pos;
                }
            }
        }
    }

    /// Force-open selected item (x key): bypass archive mode, use xdg-open
    pub fn open_selected_force(&mut self) {
        let Some(entry) = self.files.get(self.index) else { return };
        let path = entry.path.clone();
        if entry.is_dir {
            self.save_dir_index();
            let _ = env::set_current_dir(&path);
            self.index = 0;
            self.scroll_ix = 0;
            self.prev_selected = None;
            self.load_dir();
        } else {
            self.open_file(&path);
        }
    }

    pub fn open_file(&mut self, path: &PathBuf) {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if preview::is_text_ext(ext) {
            let editor = env::var("EDITOR").unwrap_or_else(|_| "vim".into());
            self.run_interactive(&format!("{} {:?}", editor, path));
            return;
        }
        let _ = std::process::Command::new("xdg-open")
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    pub fn run_interactive(&mut self, cmd: &str) {
        Crust::cleanup();
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .status();
        Crust::init();
        self.load_dir();
        self.rebuild_panes();
    }

    // --- View toggles ---

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        let name = self.files.get(self.index).map(|e| e.name.clone());
        self.load_dir();
        if let Some(name) = name {
            if let Some(pos) = self.files.iter().position(|e| e.name == name) {
                self.index = pos;
            }
        }
    }

    pub fn toggle_long_format(&mut self) {
        self.long_format = !self.long_format;
    }

    pub fn cycle_sort(&mut self) {
        self.sort_mode = self.sort_mode.next();
        self.load_dir();
    }

    pub fn toggle_sort_invert(&mut self) {
        self.sort_invert = !self.sort_invert;
        self.load_dir();
    }

    pub fn toggle_preview(&mut self) {
        self.config.preview = !self.config.preview;
        if self.config.preview {
            self.prev_selected = None;
        } else {
            self.right.clear();
        }
    }

    pub fn toggle_bat(&mut self) {
        self.config.bat = !self.config.bat;
        self.prev_selected = None;
        preview::clear_cache(&self.preview_cache);
        if self.config.bat {
            self.msg_info("Syntax: bat (external)");
        } else {
            self.msg_info(&format!("Syntax: internal ({})", self.config.syntax_theme));
        }
    }

    pub fn change_width(&mut self) {
        let mut w = self.config.width;
        w = if w >= 7 { 2 } else { w + 1 };
        self.config.width = w;
        self.config.save();
        self.resize();
    }

    pub fn change_width_reverse(&mut self) {
        let mut w = self.config.width;
        w = if w <= 2 { 7 } else { w - 1 };
        self.config.width = w;
        self.config.save();
        self.resize();
    }

    pub fn toggle_border(&mut self) {
        self.config.border = (self.config.border + 1) % 4;
        self.config.save();
        self.rebuild_panes();
    }

    // --- State ---

    pub fn save_dir_index(&mut self) {
        let cwd = env::current_dir().unwrap_or_default();
        self.state.dir_index.insert(cwd.to_string_lossy().to_string(), self.index);
        // Cap at 200 entries to prevent unbounded state file growth
        if self.state.dir_index.len() > 200 {
            // Remove arbitrary excess entries (HashMap has no ordering, just trim)
            let keys: Vec<String> = self.state.dir_index.keys().take(self.state.dir_index.len() - 200).cloned().collect();
            for k in keys { self.state.dir_index.remove(&k); }
        }
    }

    pub fn save_and_quit(&mut self) {
        self.save_dir_index();
        self.config.show_hidden = self.show_hidden;
        self.config.long_format = self.long_format;
        self.config.sort_mode = self.sort_mode.label().to_string();
        self.config.sort_invert = self.sort_invert;
        self.config.save();
        self.state.save();
        // Write exit directory so the parent shell can cd to it
        if let Ok(cwd) = env::current_dir() {
            let _ = std::fs::write(crate::config::lastdir_path(), cwd.to_string_lossy().as_bytes());
        }
    }

    pub fn refresh(&mut self) {
        self.load_dir();
        self.prev_selected = None;
        self.rebuild_panes();
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.files.get(self.index).map(|e| e.path.clone())
    }

    pub fn selected_entry(&self) -> Option<&DirEntry> {
        self.files.get(self.index)
    }

    // --- Config management ---

    pub fn show_config(&mut self) {
        use crust::Input;
        let themes = crate::highlight::available_themes();
        let sort_modes = ["name", "size", "time", "ext"];
        let border_modes = ["0: none", "1: right", "2: both", "3: left"];

        loop {
            let theme_idx = themes.iter().position(|t| *t == self.config.syntax_theme).unwrap_or(0);
            let sort_idx = sort_modes.iter().position(|s| *s == self.config.sort_mode).unwrap_or(0);

            let mut lines = Vec::new();
            lines.push(String::new());
            lines.push(format!("  {}", style::bold("Preferences")));
            let sep_w = 40;
            lines.push(format!("  {}", style::fg(&"-".repeat(sep_w), 238)));
            lines.push(String::new());
            lines.push(format!("  {} Width:        {}/7", style::fg("w", 220), self.config.width));
            lines.push(format!("  {} Border:       {}", style::fg("b", 220), border_modes[self.config.border as usize % 4]));
            lines.push(format!("  {} Preview:      {}", style::fg("p", 220),
                if self.config.preview { style::fg("on", 35) } else { style::fg("off", 196) }));
            lines.push(format!("  {} Syntax:       {}", style::fg("s", 220),
                if self.config.bat { style::fg("bat (external)", 81) }
                else { style::fg(&format!("internal ({})", themes[theme_idx]), 81) }));
            lines.push(format!("  {} Theme:        {}", style::fg("t", 220), style::fg(themes[theme_idx], 81)));
            lines.push(format!("  {} Hidden files: {}", style::fg("h", 220),
                if self.show_hidden { style::fg("shown", 35) } else { style::fg("hidden", 196) }));
            lines.push(format!("  {} Long format:  {}", style::fg("l", 220),
                if self.long_format { style::fg("on", 35) } else { style::fg("off", 196) }));
            lines.push(format!("  {} Sort:         {}{}", style::fg("o", 220),
                sort_modes[sort_idx],
                if self.sort_invert { " (reversed)" } else { "" }));
            lines.push(format!("  {} Sort reverse: {}", style::fg("i", 220),
                if self.sort_invert { style::fg("on", 35) } else { style::fg("off", 196) }));
            lines.push(format!("  {} Trash:        {}", style::fg("x", 220),
                if self.config.trash { style::fg("on", 35) } else { style::fg("off", 196) }));
            lines.push(String::new());
            lines.push(format!("  {}", style::fg(&"-".repeat(sep_w), 238)));
            lines.push(format!("  {} Top fg:       {}", style::fg("1", 220), self.config.c_top_fg));
            lines.push(format!("  {} Top bg:       {}", style::fg("2", 220), self.config.c_top_bg));
            lines.push(format!("  {} Status fg:    {}", style::fg("3", 220), self.config.c_status_fg));
            lines.push(format!("  {} Status bg:    {}", style::fg("4", 220), self.config.c_status_bg));
            lines.push(String::new());
            lines.push(format!("  {}", style::fg(&"-".repeat(sep_w), 238)));
            lines.push(format!("  {} Top bg matching:", style::fg("m", 220)));
            for (i, (pattern, color)) in self.config.topmatch.iter().enumerate() {
                let p = if pattern.is_empty() { "(default)" } else { pattern };
                lines.push(format!("    {} {} \u{2192} {}", style::fg(&format!("{}", i), 245), p,
                    style::fg(&format!("{}", color), *color as u8)));
            }
            lines.push(format!("    {} add  {} remove", style::fg("+", 220), style::fg("-", 220)));
            lines.push(String::new());
            lines.push(format!("  {}", style::fg(&"-".repeat(sep_w), 238)));
            lines.push(format!("  {} Save config", style::fg("W", 220)));
            lines.push(format!("  {} Show config file", style::fg("F", 220)));
            lines.push(format!("  {} Close", style::fg("ESC", 220)));

            self.show_in_right(&lines.join("\n"));

            let Some(key) = Input::getchr(None) else { break };
            match key.as_str() {
                "ESC" | "q" | "C" => {
                    self.unlock_right_pane();
                    self.render();
                    break;
                }
                "w" => {
                    self.config.width = if self.config.width >= 7 { 2 } else { self.config.width + 1 };
                    self.resize();
                    self.render_top();
                    self.render_left();
                    self.render_status();
                }
                "b" => {
                    self.config.border = (self.config.border + 1) % 4;
                    self.resize();
                    self.render_top();
                    self.render_left();
                    self.render_status();
                }
                "p" => {
                    self.config.preview = !self.config.preview;
                }
                "s" => {
                    self.config.bat = !self.config.bat;
                    preview::clear_cache(&self.preview_cache);
                }
                "t" => {
                    let next = (theme_idx + 1) % themes.len();
                    self.config.syntax_theme = themes[next].to_string();
                    crate::highlight::set_theme(themes[next]);
                    preview::clear_cache(&self.preview_cache);
                }
                "h" => {
                    self.show_hidden = !self.show_hidden;
                    self.config.show_hidden = self.show_hidden;
                    self.load_dir();
                }
                "l" => {
                    self.long_format = !self.long_format;
                    self.config.long_format = self.long_format;
                }
                "o" => {
                    let next = (sort_idx + 1) % sort_modes.len();
                    self.config.sort_mode = sort_modes[next].to_string();
                    self.sort_mode = crate::entry::SortMode::from_str(sort_modes[next]);
                    self.load_dir();
                }
                "i" => {
                    self.sort_invert = !self.sort_invert;
                    self.config.sort_invert = self.sort_invert;
                    self.load_dir();
                }
                "x" => {
                    self.config.trash = !self.config.trash;
                }
                "1" => {
                    let val = self.prompt_value("Top fg (0-255): ");
                    if let Some(v) = val { self.config.c_top_fg = v; self.top.fg = v; }
                }
                "2" => {
                    let val = self.prompt_value("Top bg (0-255): ");
                    if let Some(v) = val { self.config.c_top_bg = v; self.top.bg = v; }
                }
                "3" => {
                    let val = self.prompt_value("Status fg (0-255): ");
                    if let Some(v) = val { self.config.c_status_fg = v; self.status.fg = v; }
                }
                "4" => {
                    let val = self.prompt_value("Status bg (0-255): ");
                    if let Some(v) = val { self.config.c_status_bg = v; self.status.bg = v; }
                }
                "m" => {
                    // Edit existing topmatch entry by index
                    let idx_str = self.prompt_value_str("Edit entry # (or Enter to skip): ");
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if idx < self.config.topmatch.len() {
                            let (ref old_pat, _) = self.config.topmatch[idx];
                            let new_color = self.prompt_value(&format!("New color for '{}' (0-255): ",
                                if old_pat.is_empty() { "(default)" } else { old_pat }));
                            if let Some(c) = new_color {
                                self.config.topmatch[idx].1 = c;
                            }
                        }
                    }
                }
                "+" => {
                    let pattern = self.prompt_value_str("Path pattern (empty=default): ");
                    let color = self.prompt_value("Top bg color (0-255): ");
                    if let Some(c) = color {
                        self.config.topmatch.push((pattern, c));
                    }
                }
                "-" => {
                    if self.config.topmatch.len() > 1 {
                        let idx_str = self.prompt_value_str("Remove entry #: ");
                        if let Ok(idx) = idx_str.parse::<usize>() {
                            if idx < self.config.topmatch.len() {
                                self.config.topmatch.remove(idx);
                            }
                        }
                    }
                }
                "W" => {
                    self.write_config();
                }
                "F" => {
                    let path = crate::config::config_path_str();
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        self.show_in_right(&content);
                    }
                    if let Some(k) = Input::getchr(None) {
                        if k == "ESC" || k == "q" { continue; }
                    }
                }
                _ => {}
            }
        }
    }

    fn prompt_value(&mut self, label: &str) -> Option<u16> {
        let result = self.prompt_value_str(label);
        result.parse::<u16>().ok()
    }

    fn prompt_value_str(&mut self, label: &str) -> String {
        let result = self.status.ask_with_bg(label, "", self.config.c_status_bg);
        self.status.bg = self.config.c_status_bg;
        self.status.full_refresh();
        self.render_status();
        result
    }

    pub fn write_config(&mut self) {
        self.config.show_hidden = self.show_hidden;
        self.config.long_format = self.long_format;
        self.config.sort_mode = self.sort_mode.label().to_string();
        self.config.sort_invert = self.sort_invert;
        self.config.save();
        self.msg_success("Config saved");
    }

    pub fn reload_config(&mut self) {
        self.config = Config::load();
        self.sort_mode = SortMode::from_str(&self.config.sort_mode);
        self.show_hidden = self.config.show_hidden;
        self.long_format = self.config.long_format;
        self.sort_invert = self.config.sort_invert;
        self.rebuild_panes();
        self.load_dir();
        self.msg_success("Config reloaded");
    }

    /// Show current sort/filter as ls command equivalent
    pub fn show_sort_command(&mut self) {
        let mut parts = vec!["ls".to_string()];
        if self.show_hidden { parts.push("-a".into()); }
        if self.long_format { parts.push("-l".into()); }
        match self.sort_mode {
            SortMode::Size => parts.push("-S".into()),
            SortMode::Time => parts.push("-t".into()),
            SortMode::Extension => parts.push("-X".into()),
            _ => {}
        }
        if self.sort_invert { parts.push("-r".into()); }
        parts.push("--group-directories-first".into());
        if !self.filter_ext.is_empty() {
            parts.push(format!("(filter: {})", self.filter_ext));
        }
        if !self.filter_regex.is_empty() {
            parts.push(format!("(regex: /{}/) ", self.filter_regex));
        }
        self.msg_info(&parts.join(" "));
    }

    /// Track a file access for recent files
    pub fn track_recent_file(&mut self, path: &PathBuf) {
        let s = path.to_string_lossy().to_string();
        self.state.recent_files.retain(|p| p != &s);
        self.state.recent_files.insert(0, s);
        if self.state.recent_files.len() > 50 {
            self.state.recent_files.truncate(50);
        }
    }

    /// Track a directory access for recent dirs
    pub fn track_recent_dir(&mut self, path: &PathBuf) {
        let s = path.to_string_lossy().to_string();
        self.state.recent_dirs.retain(|p| p != &s);
        self.state.recent_dirs.insert(0, s);
        if self.state.recent_dirs.len() > 20 {
            self.state.recent_dirs.truncate(20);
        }
    }

    // --- Async file operations ---

    pub fn file_op_running(&self) -> bool {
        self.file_op_thread.as_ref().map(|t| !t.is_finished()).unwrap_or(false)
    }

    /// Check and display async file operation progress/completion
    pub fn check_file_op(&mut self) {
        let state = self.file_op.lock().unwrap();
        if state.complete {
            let msg = state.result_msg.clone().unwrap_or_default();
            let ok = state.result_ok;
            let undo = state.undo_op.clone();
            drop(state);

            if let Some(op) = undo {
                self.undo_stack.push(op);
            }
            if ok {
                self.msg_success(&msg);
            } else {
                self.msg_error(&msg);
            }
            self.load_dir();

            // Reset state
            let mut state = self.file_op.lock().unwrap();
            state.complete = false;
            state.progress.clear();
            state.result_msg = None;
            state.undo_op = None;
            self.file_op_thread = None;
        } else if !state.progress.is_empty() {
            let progress = state.progress.clone();
            drop(state);
            self.msg_info(&progress);
        }
    }

    // --- Colored status feedback ---
    // Green(46)=success, Cyan(81)=info, Yellow(220)=warning, Red(196)=error, Gray(245)=cancelled

    pub fn msg_success(&mut self, msg: &str) {
        self.status.say(&style::fg(&format!(" {}", msg), 46));
    }
    pub fn msg_info(&mut self, msg: &str) {
        self.status.say(&style::fg(&format!(" {}", msg), 81));
    }
    pub fn msg_warn(&mut self, msg: &str) {
        self.status.say(&style::fg(&format!(" {}", msg), 220));
    }
    pub fn msg_error(&mut self, msg: &str) {
        self.status.say(&style::fg(&format!(" {}", msg), 196));
    }
    pub fn msg_cancel(&mut self) {
        self.status.say(&style::fg(" Cancelled", 245));
    }

    /// Prompt with dark blue background (like RTFM command mode)
    pub fn prompt(&mut self, prompt: &str, default: &str) -> String {
        let result = self.status.ask_with_bg(prompt, default, 18);
        // Restore status bar after prompt (clears lingering prompt text)
        self.render_status();
        result
    }
}

// --- Helpers ---


fn visible_height(pane: &Pane) -> usize {
    // Border is drawn outside the pane, so content height is always h
    pane.h as usize
}

/// Format entry name with type suffix: / for dirs, @ for symlinks, * for executables
/// Symlinks to directories get @/ suffix like RTFM
fn format_short_entry(entry: &DirEntry) -> String {
    let mut name = entry.name.clone();
    if entry.is_symlink {
        name.push('@');
        if entry.is_dir { name.push('/'); }
    } else if entry.is_dir {
        name.push('/');
    } else if entry.is_exec {
        name.push('*');
    }
    if !entry.color_code.is_empty() {
        format!("{}{}\x1b[0m", entry.color_code, name)
    } else {
        name
    }
}

fn format_long_entry(entry: &DirEntry) -> String {
    let mut name = entry.name.clone();
    if entry.is_symlink {
        name.push('@');
        if entry.is_dir { name.push('/'); }
    } else if entry.is_dir {
        name.push('/');
    } else if entry.is_exec {
        name.push('*');
    }
    let mode = format_mode(entry.mode);
    let size = format_size(entry.size);
    let time = format_time(entry.modified);
    let colored_name = if !entry.color_code.is_empty() {
        format!("{}{}\x1b[0m", entry.color_code, name)
    } else {
        name
    };
    format!("{} {:>6} {} {}", mode, size, time, colored_name)
}

fn get_cached_user() -> String {
    use std::sync::OnceLock;
    static USER: OnceLock<String> = OnceLock::new();
    USER.get_or_init(|| env::var("USER").unwrap_or_else(|_| "user".into())).clone()
}

fn get_cached_host() -> String {
    use std::sync::OnceLock;
    static HOST: OnceLock<String> = OnceLock::new();
    HOST.get_or_init(|| {
        std::fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "host".into())
            .trim()
            .to_string()
    }).clone()
}

pub fn uid_to_name(uid: u32) -> String {
    use std::sync::OnceLock;
    // Cache the current user's uid/name since it's almost always the same
    static CACHED: OnceLock<(u32, String)> = OnceLock::new();
    let (cached_uid, cached_name) = CACHED.get_or_init(|| {
        let name = env::var("USER").unwrap_or_else(|_| uid.to_string());
        (uid, name)
    });
    if uid == *cached_uid {
        cached_name.clone()
    } else {
        uid.to_string()
    }
}

pub fn gid_to_name(gid: u32) -> String {
    let user = get_cached_user();
    let primary_gid = unsafe { libc::getgid() };
    if gid == primary_gid {
        user
    } else {
        gid.to_string()
    }
}

/// Compute extra info for top bar (dir item count, image dims, pdf pages)
fn compute_top_extra(entry: &crate::entry::DirEntry) -> String {
    if entry.is_dir {
        let n = std::fs::read_dir(&entry.path).map(|d| d.count()).unwrap_or(0);
        format!(" [{} items]", n)
    } else {
        let ext = entry.path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if crate::preview::is_image_ext(ext) {
            get_image_info(&entry.path)
        } else if ext.eq_ignore_ascii_case("pdf") {
            get_pdf_info(&entry.path)
        } else {
            String::new()
        }
    }
}

/// Get image dimensions and color info for top bar (like RTFM)
fn get_image_info(path: &std::path::Path) -> String {
    let output = std::process::Command::new("identify")
        .arg("-format")
        .arg("[%wx%h %[colorspace] %[bit-depth]-bit]")
        .arg(path)
        .output();
    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => String::new(),
    }
}

/// Get PDF page count for top bar
fn get_pdf_info(path: &std::path::Path) -> String {
    let output = std::process::Command::new("pdfinfo")
        .arg(path)
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            for line in text.lines() {
                if line.starts_with("Pages:") {
                    let pages = line.split(':').nth(1).unwrap_or("").trim();
                    return format!(" [{} pages]", pages);
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}
