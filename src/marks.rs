use crate::app::App;
use crust::style;
use crust::Input;
use std::env;

impl App {
    /// Set a bookmark: show marks, single-key input, support '-' to delete
    pub fn set_mark(&mut self) {
        // Show existing marks in right pane first (like RTFM)
        self.show_marks_display();
        self.status.say(&style::fg(" Set mark (letter) or -letter to delete:", 220));

        // Single keypress
        let Some(key) = Input::getchr(None) else { return };
        if key == "ESC" { return; }

        // Delete mark: if '-', wait for letter
        if key == "-" {
            self.status.say(&style::fg(" Delete mark (press letter):", 220));
            let Some(del_key) = Input::getchr(None) else { return };
            if del_key == "ESC" { return; }
            if self.state.marks.remove(&del_key).is_some() {
                self.msg_success(&format!("Mark '{}' deleted", del_key));
            } else {
                self.msg_warn(&format!("Mark '{}' not set", del_key));
            }
            return;
        }

        if key.len() != 1 { return; }
        let ch = key.chars().next().unwrap();
        if !ch.is_alphanumeric() { return; }
        let cwd = env::current_dir().unwrap_or_default();
        self.state.marks.insert(key.clone(), cwd.to_string_lossy().to_string());
        self.msg_success(&format!("Mark '{}' set", ch));
    }

    /// Show marks in right pane, then wait for a key to jump immediately
    pub fn jump_to_mark(&mut self) {
        self.show_marks_display();

        let Some(key) = Input::getchr(None) else { return };
        if key == "ESC" { return; }

        if let Some(path) = self.state.marks.get(&key).cloned() {
            let target = std::path::PathBuf::from(&path);
            if target.is_dir() {
                self.save_dir_index();
                // Rotate mark history: shift 1->2->3->4->5, set ' to current
                self.rotate_mark_history();
                let _ = env::set_current_dir(&target);
                self.index = 0;
                self.scroll_ix = 0;
                self.prev_selected = None;
                self.load_dir();
                self.msg_success(&format!("Jumped to mark '{}'", key));
            } else {
                self.msg_error(&format!("Mark '{}': directory not found", key));
            }
        } else {
            self.msg_warn(&format!("Mark '{}' not set", key));
        }
    }

    /// Rotate automatic mark history (marks 1-5, ' = last dir)
    pub fn rotate_mark_history(&mut self) {
        let cwd = env::current_dir().unwrap_or_default().to_string_lossy().to_string();
        // Shift 4->5, 3->4, 2->3, 1->2
        for i in (1..5).rev() {
            let from = i.to_string();
            let to = (i + 1).to_string();
            if let Some(val) = self.state.marks.get(&from).cloned() {
                self.state.marks.insert(to, val);
            }
        }
        // Set mark 1 to current dir, ' to current dir
        self.state.marks.insert("1".to_string(), cwd.clone());
        self.state.marks.insert("'".to_string(), cwd);
    }

    pub fn show_marks(&mut self) {
        self.show_marks_display();
    }

    fn show_marks_display(&mut self) {
        let mut lines: Vec<String> = Vec::new();
        lines.push(style::fg("Directory Marks", 81));
        lines.push("=".repeat(50));
        lines.push(String::new());
        lines.push(style::fg("Current marks:", 46));
        lines.push(String::new());

        if self.state.marks.is_empty() {
            lines.push(style::fg("  No marks set", 245));
        } else {
            let mut keys: Vec<&String> = self.state.marks.keys().collect();
            keys.sort();
            // Special marks first: ' and 0-5
            let special: Vec<&&String> = keys.iter().filter(|k| {
                let c = k.chars().next().unwrap_or(' ');
                c == '\'' || c.is_ascii_digit()
            }).collect();
            let user: Vec<&&String> = keys.iter().filter(|k| {
                let c = k.chars().next().unwrap_or(' ');
                c != '\'' && !c.is_ascii_digit()
            }).collect();

            for key in &special {
                let path = &self.state.marks[**key];
                lines.push(format!("  {}  \u{2192}  {}",
                    style::fg(key, 220), path));
            }
            if !special.is_empty() && !user.is_empty() {
                lines.push(String::new());
            }
            for key in &user {
                let path = &self.state.marks[**key];
                lines.push(format!("  {}  \u{2192}  {}",
                    style::fg(key, 220), path));
            }
        }

        lines.push(String::new());
        lines.push(format!("Press {} + letter to jump", style::fg("'", 220)));

        self.show_in_right(&lines.join("\n"));
    }

    pub fn go_home(&mut self) {
        let home = env::var("HOME").unwrap_or_default();
        if !home.is_empty() {
            self.save_dir_index();
            self.rotate_mark_history();
            let _ = env::set_current_dir(&home);
            self.index = 0;
            self.scroll_ix = 0;
            self.prev_selected = None;
            self.load_dir();
        }
    }

    pub fn follow_symlink(&mut self) {
        let Some(entry) = self.files.get(self.index) else { return };
        if !entry.is_symlink { return; }
        let Ok(target) = std::fs::read_link(&entry.path) else { return };
        let resolved = if target.is_absolute() {
            target
        } else {
            entry.path.parent().unwrap_or(std::path::Path::new("/")).join(&target)
        };
        if resolved.is_dir() {
            self.save_dir_index();
            let _ = env::set_current_dir(&resolved);
            self.index = 0;
            self.scroll_ix = 0;
            self.prev_selected = None;
            self.load_dir();
        } else if let Some(parent) = resolved.parent() {
            self.save_dir_index();
            let _ = env::set_current_dir(parent);
            self.index = 0;
            self.scroll_ix = 0;
            self.prev_selected = None;
            self.load_dir();
            let name = resolved.file_name().map(|n| n.to_string_lossy().to_string());
            if let Some(name) = name {
                if let Some(pos) = self.files.iter().position(|e| e.name == name) {
                    self.index = pos;
                }
            }
        }
    }
}
