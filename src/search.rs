use crate::app::App;

impl App {
    /// Start search: prompt for term, highlight matches, jump to first
    pub fn search_prompt(&mut self) {
        self.status.prompt = "Search: ".into();
        self.status.record = true;
        let default = self.search_term.clone();
        let term = self.prompt("Search: ", &default);
        if term.is_empty() {
            return;
        }
        self.search_term = term;
        self.apply_search();
        // Jump to first match from current position
        self.search_next();
    }

    /// Find next search match
    pub fn search_next(&mut self) {
        if self.search_term.is_empty() { return; }
        let term = self.search_term.to_lowercase();
        let start = self.index + 1;
        // Search forward from current position
        for i in start..self.files.len() {
            if self.files[i].name.to_lowercase().contains(&term) {
                self.index = i;
                return;
            }
        }
        // Wrap around
        for i in 0..start.min(self.files.len()) {
            if self.files[i].name.to_lowercase().contains(&term) {
                self.index = i;
                return;
            }
        }
    }

    /// Find previous search match
    pub fn search_prev(&mut self) {
        if self.search_term.is_empty() { return; }
        let term = self.search_term.to_lowercase();
        // Search backward from current position
        if self.index > 0 {
            for i in (0..self.index).rev() {
                if self.files[i].name.to_lowercase().contains(&term) {
                    self.index = i;
                    return;
                }
            }
        }
        // Wrap around
        for i in (self.index..self.files.len()).rev() {
            if self.files[i].name.to_lowercase().contains(&term) {
                self.index = i;
                return;
            }
        }
    }

    /// Clear search
    pub fn search_clear(&mut self) {
        self.search_term.clear();
        for entry in &mut self.files {
            entry.search_hit = false;
        }
    }

    fn apply_search(&mut self) {
        let term = self.search_term.to_lowercase();
        for entry in &mut self.files {
            entry.search_hit = entry.name.to_lowercase().contains(&term);
        }
    }

    /// Filter by file extension
    pub fn filter_ext_prompt(&mut self) {
        let default = self.filter_ext.clone();
        let ext = self.prompt("Filter ext: ", &default);
        self.filter_ext = ext;
        self.filter_regex.clear();
        self.load_dir();
        self.index = 0;
        self.scroll_ix = 0;
    }

    /// Filter by regex pattern
    pub fn filter_regex_prompt(&mut self) {
        let default = self.filter_regex.clone();
        let pattern = self.prompt("Filter regex: ", &default);
        self.filter_regex = pattern;
        self.filter_ext.clear();
        self.load_dir();
        self.index = 0;
        self.scroll_ix = 0;
    }

    /// Clear all filters
    pub fn filter_clear(&mut self) {
        self.filter_ext.clear();
        self.filter_regex.clear();
        self.load_dir();
    }

    /// Grep file contents (g key)
    pub fn grep_files(&mut self) {
        let pattern = self.prompt("Grep: ", "");
        if pattern.is_empty() { return; }
        let output = std::process::Command::new("grep")
            .args(["-rn", "--color=always", &pattern, "."])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                let result = String::from_utf8_lossy(&o.stdout).to_string();
                self.show_in_right(&result);
            }
            _ => { self.msg_info("No matches"); }
        }
    }

    /// Locate files (L key)
    pub fn locate_files(&mut self) {
        let pattern = self.prompt("Locate: ", "");
        if pattern.is_empty() { return; }
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("locate {} 2>/dev/null | head -200", pattern))
            .output();
        match output {
            Ok(o) if o.status.success() && !o.stdout.is_empty() => {
                let result = String::from_utf8_lossy(&o.stdout).to_string();
                self.show_in_right(&result);
                self.msg_info("Locate results shown. Press # to jump to a line.");
            }
            _ => { self.msg_info("No results"); }
        }
    }

    /// Jump to locate result (# key)
    pub fn jump_locate(&mut self) {
        let input = self.prompt("# Line: ", "");
        let nr: usize = match input.trim().parse() {
            Ok(n) if n > 0 => n,
            _ => { self.msg_error("Invalid line number"); return; }
        };
        let text = self.right.text().to_string();
        let lines: Vec<&str> = text.lines().collect();
        if nr > lines.len() {
            self.msg_error("Line number out of range");
            return;
        }
        let line = crust::strip_ansi(lines[nr - 1]);
        let path = line.trim();
        let target = std::path::PathBuf::from(path);
        if target.is_dir() {
            self.save_dir_index();
            let _ = std::env::set_current_dir(&target);
            self.index = 0;
            self.scroll_ix = 0;
            self.prev_selected = None;
            self.load_dir();
        } else if let Some(parent) = target.parent() {
            if parent.is_dir() {
                self.save_dir_index();
                let _ = std::env::set_current_dir(parent);
                self.index = 0;
                self.scroll_ix = 0;
                self.prev_selected = None;
                self.load_dir();
                if let Some(name) = target.file_name() {
                    let name = name.to_string_lossy();
                    if let Some(pos) = self.files.iter().position(|e| e.name == name.as_ref()) {
                        self.index = pos;
                    }
                }
            }
        }
    }

    /// fzf file finder (C-L key)
    pub fn fzf_jump(&mut self) {
        let tmp = "/tmp/pointer_fzf_selection";
        self.run_interactive(&format!("find . 2>/dev/null | fzf > {} 2>/dev/tty", tmp));
        if let Ok(selection) = std::fs::read_to_string(tmp) {
            let _ = std::fs::remove_file(tmp);
            let path = std::path::PathBuf::from(selection.trim());
            if path.as_os_str().is_empty() { return; }
            let target = if path.is_absolute() { path } else {
                std::env::current_dir().unwrap_or_default().join(&path)
            };
            if target.is_dir() {
                self.save_dir_index();
                let _ = std::env::set_current_dir(&target);
                self.index = 0;
                self.scroll_ix = 0;
                self.prev_selected = None;
                self.load_dir();
            } else if let Some(parent) = target.parent() {
                if parent.is_dir() {
                    self.save_dir_index();
                    let _ = std::env::set_current_dir(parent);
                    self.index = 0;
                    self.scroll_ix = 0;
                    self.prev_selected = None;
                    self.load_dir();
                    if let Some(name) = target.file_name() {
                        let name = name.to_string_lossy();
                        if let Some(pos) = self.files.iter().position(|e| e.name == name.as_ref()) {
                            self.index = pos;
                        }
                    }
                }
            }
        }
    }

    /// Navi integration (C-N key)
    pub fn navi_invoke(&mut self) {
        self.run_interactive("navi");
    }
}
