use crate::app::{App, FileOpState};
use crate::config;
use crate::undo::UndoOp;
use crust::style;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

impl App {
    /// Get items to operate on: tagged items, or selected item
    fn op_items(&self) -> Vec<PathBuf> {
        if !self.tagged.is_empty() {
            self.tagged.clone()
        } else if let Some(entry) = self.files.get(self.index) {
            if entry.name != ".." {
                vec![entry.path.clone()]
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    }

    /// Tag/untag current item
    pub fn tag_toggle(&mut self) {
        let Some(entry) = self.files.get(self.index) else { return };
        if entry.name == ".." { return; }
        let path = entry.path.clone();
        if let Some(pos) = self.tagged.iter().position(|p| p == &path) {
            self.tagged.remove(pos);
        } else {
            self.tagged.push(path);
        }
        self.tagged_size_cache = None;
        // Update tag flag in files
        if let Some(entry) = self.files.get_mut(self.index) {
            entry.tagged = !entry.tagged;
        }
        // Advance to next
        if self.index < self.files.len().saturating_sub(1) {
            self.index += 1;
        }
    }

    /// Show tagged items in right pane
    pub fn tag_show(&mut self) {
        if self.tagged.is_empty() {
            self.show_in_right(" No tagged items");
            return;
        }
        let mut lines = vec![style::bold(&format!("Tagged: {} items", self.tagged.len())), String::new()];
        let total_size: u64 = self.tagged.iter()
            .filter_map(|p| fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        lines.push(format!("Total size: {}", crate::entry::format_size(total_size)));
        lines.push(String::new());
        for path in &self.tagged {
            lines.push(format!("  {}", path.display()));
        }
        self.show_in_right(&lines.join("\n"));
    }

    /// Clear all tags
    pub fn tag_clear(&mut self) {
        self.tagged.clear();
        self.tagged_size_cache = None;
        for entry in &mut self.files {
            entry.tagged = false;
        }
    }

    /// Tag by pattern
    pub fn tag_pattern(&mut self) {
        let pattern = self.prompt("Tag pattern: ", "");
        if pattern.is_empty() { return; }
        self.tagged_size_cache = None;
        if pattern == "." {
            // Tag all visible files (not dirs, not ..)
            for entry in &mut self.files {
                if !entry.is_dir && entry.name != ".." && !entry.tagged {
                    entry.tagged = true;
                    self.tagged.push(entry.path.clone());
                }
            }
            return;
        }
        if let Ok(re) = regex::Regex::new(&pattern) {
            for entry in &mut self.files {
                if entry.name != ".." && !entry.tagged && re.is_match(&entry.name) {
                    entry.tagged = true;
                    self.tagged.push(entry.path.clone());
                }
            }
        }
    }

    /// Copy tagged/selected items to current directory (async for multi-item)
    pub fn copy_items(&mut self) {
        if self.file_op_running() {
            self.msg_error("Another file operation is in progress");
            return;
        }
        let items = self.op_items();
        if items.is_empty() { return; }
        let cwd = std::env::current_dir().unwrap_or_default();
        let total = items.len();

        // Small operations: synchronous
        if total == 1 && !items[0].is_dir() {
            let src = &items[0];
            let mut dest = cwd.join(src.file_name().unwrap_or_default());
            dest = unique_dest(dest);
            if fs::copy(src, &dest).is_ok() {
                self.undo_stack.push(UndoOp::Copy { created: vec![dest] });
                self.msg_success("Copied 1 item");
            } else {
                self.msg_error("Copy failed");
            }
            self.load_dir();
            return;
        }

        // Large operations: async with progress
        let state = self.file_op.clone();
        self.msg_info(&format!("Copying {} item(s)...", total));
        self.file_op_thread = Some(std::thread::spawn(move || {
            let mut created = Vec::new();
            for (i, src) in items.iter().enumerate() {
                {
                    let mut s = state.lock().unwrap();
                    s.progress = format!("Copying {}/{}: {}", i + 1, total,
                        src.file_name().unwrap_or_default().to_string_lossy());
                }
                let name = src.file_name().unwrap_or_default();
                let mut dest = cwd.join(name);
                dest = unique_dest(dest);
                if src.is_dir() {
                    if copy_dir_recursive(src, &dest).is_ok() {
                        created.push(dest);
                    }
                } else if fs::copy(src, &dest).is_ok() {
                    created.push(dest);
                }
            }
            let mut s = state.lock().unwrap();
            s.complete = true;
            s.result_ok = created.len() == total;
            s.result_msg = Some(if created.len() == total {
                format!("Copied {} item(s)", total)
            } else {
                format!("Copied {}/{} item(s)", created.len(), total)
            });
            if !created.is_empty() {
                s.undo_op = Some(UndoOp::Copy { created });
            }
        }));
        self.tagged.clear();
    }

    /// Move tagged/selected items to current directory (async for multi-item)
    pub fn move_items(&mut self) {
        if self.file_op_running() {
            self.msg_error("Another file operation is in progress");
            return;
        }
        let items = self.op_items();
        if items.is_empty() { return; }
        let cwd = std::env::current_dir().unwrap_or_default();
        let total = items.len();

        // Small operations: synchronous
        if total == 1 {
            let src = &items[0];
            let mut dest = cwd.join(src.file_name().unwrap_or_default());
            dest = unique_dest(dest);
            if fs::rename(src, &dest).is_ok() {
                self.undo_stack.push(UndoOp::Move { moves: vec![(dest, src.clone())] });
                self.msg_success("Moved 1 item");
            } else {
                self.msg_error("Move failed");
            }
            self.tagged.clear();
            self.load_dir();
            return;
        }

        // Large operations: async
        let state = self.file_op.clone();
        self.msg_info(&format!("Moving {} item(s)...", total));
        self.file_op_thread = Some(std::thread::spawn(move || {
            let mut moves = Vec::new();
            for (i, src) in items.iter().enumerate() {
                {
                    let mut s = state.lock().unwrap();
                    s.progress = format!("Moving {}/{}: {}", i + 1, total,
                        src.file_name().unwrap_or_default().to_string_lossy());
                }
                let name = src.file_name().unwrap_or_default();
                let mut dest = cwd.join(name);
                dest = unique_dest(dest);
                if fs::rename(src, &dest).is_ok() {
                    moves.push((dest, src.clone()));
                }
            }
            let mut s = state.lock().unwrap();
            s.complete = true;
            s.result_ok = moves.len() == total;
            s.result_msg = Some(if moves.len() == total {
                format!("Moved {} item(s)", total)
            } else {
                format!("Moved {}/{} item(s)", moves.len(), total)
            });
            if !moves.is_empty() {
                s.undo_op = Some(UndoOp::Move { moves });
            }
        }));
        self.tagged.clear();
    }

    /// Delete tagged/selected items (RTFM style: show info in right pane, single-key confirm)
    pub fn delete_items(&mut self) {
        let items = self.op_items();
        if items.is_empty() { return; }

        // Show tagged items info in right pane like RTFM
        let total_size: u64 = items.iter()
            .filter_map(|p| fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        let mut info_lines = vec![
            style::fg("Tagged Items", 196),
            "=".repeat(50),
            String::new(),
            style::fg("Summary:", 46),
            format!("  Items:      {}", items.len()),
            format!("  Total size: {:.2} MB", total_size as f64 / 1_000_000.0),
            String::new(),
        ];
        if items.is_empty() {
            info_lines.push(style::fg("No tagged items", 245));
        } else {
            for p in &items {
                info_lines.push(format!("  {}", p.file_name().unwrap_or_default().to_string_lossy()));
            }
        }
        info_lines.push(String::new());
        info_lines.push(format!("Currently selected:"));
        if let Some(entry) = self.files.get(self.index) {
            info_lines.push(format!("  \u{2192} {}", entry.name));
        }
        self.show_in_right(&info_lines.join("\n"));

        // Show prompt and wait for single keypress (no Enter needed)
        let action = if self.config.trash { "Move to trash" } else { "Delete permanently" };
        self.status.say(&style::fg(&format!(" {}? (y/n)", action), 220));

        // Single key confirm
        let Some(key) = crust::Input::getchr(None) else { return };
        if key != "y" && key != "Y" {
            self.msg_cancel();
            self.prev_selected = None;
            return;
        }

        let trash = self.config.trash;
        let trash_dir = config::trash_dir();
        let mut trash_paths = Vec::new();
        let mut deleted = Vec::new();

        for src in &items {
            if trash {
                let name = src.file_name().unwrap_or_default().to_string_lossy().to_string();
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let trash_name = format!("{}_{}", ts, name);
                let trash_path = trash_dir.join(&trash_name);
                if fs::rename(src, &trash_path).is_ok() {
                    trash_paths.push((trash_path, src.clone()));
                    deleted.push(src.clone());
                }
            } else if src.is_dir() {
                if fs::remove_dir_all(src).is_ok() {
                    deleted.push(src.clone());
                }
            } else if fs::remove_file(src).is_ok() {
                deleted.push(src.clone());
            }
        }

        if !deleted.is_empty() {
            self.undo_stack.push(UndoOp::Delete {
                paths: deleted.clone(),
                trash_paths,
            });
        }
        self.tagged.retain(|p| !deleted.contains(p));
        if trash {
            self.msg_success(&format!("Moved {} item(s) to trash", deleted.len()));
        } else {
            self.msg_success(&format!("Deleted {} item(s)", deleted.len()));
        }
        self.prev_selected = None;
        self.load_dir();
        // Force right pane clear + re-render to prevent artifacts from delete dialog
        self.right.clear();
        self.force_render_right();
    }

    /// Rename current item
    pub fn rename_item(&mut self) {
        let Some(entry) = self.files.get(self.index) else { return };
        if entry.name == ".." { return; }
        let old_path = entry.path.clone();
        let old_name = entry.name.clone();
        let new_name = self.prompt("Rename: ", &old_name);
        if new_name.is_empty() || new_name == old_name { return; }

        let new_path = old_path.parent().unwrap_or(Path::new(".")).join(&new_name);
        if fs::rename(&old_path, &new_path).is_ok() {
            self.undo_stack.push(UndoOp::Rename { old: old_path, new: new_path });
            self.load_dir();
            // Try to select renamed item
            if let Some(pos) = self.files.iter().position(|e| e.name == new_name) {
                self.index = pos;
            }
        } else {
            self.msg_error("Rename failed");
        }
    }

    /// Create directory
    pub fn mkdir(&mut self) {
        let name = self.prompt("New directory: ", "");
        if name.is_empty() { return; }
        let cwd = std::env::current_dir().unwrap_or_default();
        let path = cwd.join(&name);
        if fs::create_dir_all(&path).is_ok() {
            self.load_dir();
            if let Some(pos) = self.files.iter().position(|e| e.name == name) {
                self.index = pos;
            }
        } else {
            self.msg_error("Failed to create directory");
        }
    }

    /// Create symlinks to tagged/selected items
    pub fn link_items(&mut self) {
        let items = self.op_items();
        if items.is_empty() { return; }
        let cwd = std::env::current_dir().unwrap_or_default();
        let mut created = Vec::new();

        for src in &items {
            let name = src.file_name().unwrap_or_default();
            let mut dest = cwd.join(name);
            dest = unique_dest(dest);
            if std::os::unix::fs::symlink(src, &dest).is_ok() {
                created.push(dest);
            }
        }

        if !created.is_empty() {
            self.undo_stack.push(UndoOp::Link { created: created.clone() });
        }
        self.msg_success(&format!("Created {} symlink(s)", created.len()));
        self.load_dir();
    }

    /// Undo last operation
    pub fn undo(&mut self) {
        let Some(op) = self.undo_stack.pop() else {
            self.msg_info("Nothing to undo");
            return;
        };
        match op {
            UndoOp::Copy { created } => {
                for path in &created {
                    if path.is_dir() {
                        let _ = fs::remove_dir_all(path);
                    } else {
                        let _ = fs::remove_file(path);
                    }
                }
                self.msg_success(&format!("Undid copy ({} items)", created.len()));
            }
            UndoOp::Move { moves } => {
                for (dest, original) in &moves {
                    let _ = fs::rename(dest, original);
                }
                self.msg_success(&format!("Undid move ({} items)", moves.len()));
            }
            UndoOp::Rename { old, new } => {
                let _ = fs::rename(&new, &old);
                self.msg_success("Undid rename");
            }
            UndoOp::Delete { trash_paths, .. } => {
                for (trash_path, original) in &trash_paths {
                    let _ = fs::rename(trash_path, original);
                }
                if trash_paths.is_empty() {
                    self.msg_error("Cannot undo permanent delete");
                } else {
                    self.msg_success(&format!("Restored {} item(s) from trash", trash_paths.len()));
                }
            }
            UndoOp::Link { created } => {
                for path in &created {
                    let _ = fs::remove_file(path);
                }
                self.msg_success(&format!("Undid symlink ({} items)", created.len()));
            }
            UndoOp::BulkRename { renames } => {
                for (new, old) in &renames {
                    let _ = fs::rename(new, old);
                }
                self.msg_success(&format!("Undid bulk rename ({} items)", renames.len()));
            }
            UndoOp::Permissions { path, old_mode } => {
                let _ = fs::set_permissions(&path, fs::Permissions::from_mode(old_mode));
                self.msg_success("Undid permission change");
            }
        }
        self.load_dir();
    }

    /// Change ownership (C-O key)
    pub fn chown(&mut self) {
        let Some(entry) = self.files.get(self.index) else { return };
        let path = entry.path.clone();
        let current = format!("{}:{}", crate::app::uid_to_name(entry.uid), crate::app::gid_to_name(entry.gid));
        let input = self.prompt("Owner:group: ", &current);
        if input.is_empty() || input == current { return; }
        let result = std::process::Command::new("chown")
            .arg(&input)
            .arg(&path)
            .output();
        match result {
            Ok(o) if o.status.success() => {
                self.msg_success(&format!("Changed ownership to {}", input));
                self.load_dir();
            }
            _ => self.msg_error("Failed to change ownership (may need sudo)"),
        }
    }

    /// Bulk rename (E key)
    pub fn bulk_rename(&mut self) {
        let items = self.op_items();
        if items.is_empty() { return; }
        let pattern = self.prompt("Rename pattern (s/old/new/, upper, lower, *.ext): ", "");
        if pattern.is_empty() { return; }

        let mut renames: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut preview_lines = vec![
            crust::style::fg("Bulk Rename Preview", 81),
            "=".repeat(50),
            String::new(),
        ];

        for src in &items {
            let name = src.file_name().unwrap_or_default().to_string_lossy().to_string();
            let new_name = apply_rename_pattern(&name, &pattern);
            if new_name != name {
                let new_path = src.parent().unwrap_or(std::path::Path::new(".")).join(&new_name);
                preview_lines.push(format!("  {} -> {}", name, crust::style::fg(&new_name, 46)));
                renames.push((src.clone(), new_path));
            }
        }

        if renames.is_empty() {
            self.msg_info("No renames to apply");
            return;
        }

        preview_lines.push(String::new());
        preview_lines.push(format!("{} rename(s). Confirm? (y/n)", renames.len()));
        self.show_in_right(&preview_lines.join("\n"));

        let Some(key) = crust::Input::getchr(None) else { return };
        if key != "y" && key != "Y" { self.msg_cancel(); return; }

        let mut done = 0;
        let mut undo_renames = Vec::new();
        for (old, new) in &renames {
            if fs::rename(old, new).is_ok() {
                undo_renames.push((new.clone(), old.clone()));
                done += 1;
            }
        }
        if !undo_renames.is_empty() {
            self.undo_stack.push(UndoOp::BulkRename { renames: undo_renames });
        }
        self.tagged.clear();
        self.msg_success(&format!("Renamed {} item(s)", done));
        self.prev_selected = None;
        self.load_dir();
    }

    /// Compare two tagged files (X key)
    pub fn compare_files(&mut self) {
        if self.tagged.len() != 2 {
            self.msg_warn("Tag exactly 2 files to compare");
            return;
        }
        let file1 = &self.tagged[0];
        let file2 = &self.tagged[1];
        let output = std::process::Command::new("diff")
            .args(["--color=always", "-u"])
            .arg(file1)
            .arg(file2)
            .output();
        match output {
            Ok(o) => {
                let result = String::from_utf8_lossy(&o.stdout).to_string();
                if result.is_empty() {
                    self.msg_info("Files are identical");
                } else {
                    self.show_in_right(&result);
                }
            }
            Err(e) => self.msg_error(&format!("diff failed: {}", e)),
        }
    }

    /// Change permissions
    pub fn chmod(&mut self) {
        let Some(entry) = self.files.get(self.index) else { return };
        if entry.name == ".." { return; }
        let path = entry.path.clone();
        let old_mode = entry.mode;
        let current = crate::entry::format_mode(old_mode);
        let input = self.prompt("Permissions: ", &current);
        if input.is_empty() { return; }

        let new_mode = parse_permissions(&input, old_mode);
        if let Some(mode) = new_mode {
            if fs::set_permissions(&path, fs::Permissions::from_mode(mode)).is_ok() {
                self.undo_stack.push(UndoOp::Permissions { path, old_mode });
                self.load_dir();
            } else {
                self.msg_error("Failed to change permissions");
            }
        } else {
            self.msg_error("Invalid permission format");
        }
    }
}

use std::os::unix::fs::PermissionsExt;

fn unique_dest(mut path: PathBuf) -> PathBuf {
    if !path.exists() { return path; }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let parent = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut n = 1;
    loop {
        path = parent.join(format!("{}_{}{}", stem, n, ext));
        if !path.exists() { return path; }
        n += 1;
    }
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dst = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dst)?;
        } else {
            fs::copy(entry.path(), &dst)?;
        }
    }
    Ok(())
}

fn parse_permissions(input: &str, current: u32) -> Option<u32> {
    // Try octal: "755"
    if let Ok(mode) = u32::from_str_radix(input.trim(), 8) {
        return Some(mode);
    }
    // Try +x, -w, etc.
    let trimmed = input.trim();
    if trimmed.starts_with('+') || trimmed.starts_with('-') {
        let add = trimmed.starts_with('+');
        let mut bits = 0u32;
        for ch in trimmed[1..].chars() {
            match ch {
                'r' => bits |= 0o444,
                'w' => bits |= 0o222,
                'x' => bits |= 0o111,
                _ => return None,
            }
        }
        return Some(if add { current | bits } else { current & !bits });
    }
    // Try rwx string: "rwxr-xr-x"
    if trimmed.len() == 9 {
        let flags = [0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001];
        let expected = ['r', 'w', 'x', 'r', 'w', 'x', 'r', 'w', 'x'];
        let mut mode = 0u32;
        for (i, ch) in trimmed.chars().enumerate() {
            if ch == expected[i] {
                mode |= flags[i];
            } else if ch != '-' {
                return None;
            }
        }
        return Some(mode);
    }
    None
}

/// Apply rename pattern to a filename
fn apply_rename_pattern(name: &str, pattern: &str) -> String {
    let p = pattern.trim();
    // s/old/new/ regex substitution
    if p.starts_with("s/") {
        let parts: Vec<&str> = p[2..].splitn(3, '/').collect();
        if parts.len() >= 2 {
            if let Ok(re) = regex::Regex::new(parts[0]) {
                return re.replace_all(name, parts[1]).to_string();
            }
        }
        return name.to_string();
    }
    // upper / lower
    if p == "upper" { return name.to_uppercase(); }
    if p == "lower" { return name.to_lowercase(); }
    // *.ext - change extension
    if p.starts_with("*.") {
        let new_ext = &p[2..];
        let stem = std::path::Path::new(name).file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| name.to_string());
        return format!("{}.{}", stem, new_ext);
    }
    // PREFIX_* - prepend
    if p.ends_with("*") {
        let prefix = &p[..p.len()-1];
        return format!("{}{}", prefix, name);
    }
    // *_SUFFIX - append before extension
    if p.starts_with("*") {
        let suffix = &p[1..];
        let path = std::path::Path::new(name);
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        return format!("{}{}{}", stem, suffix, ext);
    }
    name.to_string()
}
