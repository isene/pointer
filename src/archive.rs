use crate::app::App;
use crate::entry::DirEntry;
use crust::style;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[derive(Clone)]
pub struct ArchiveState {
    pub archive_path: PathBuf,
    pub current_dir: String,    // virtual dir inside archive, "" = root
    pub entries: Vec<ArchiveEntry>,
    pub saved_tagged: Vec<PathBuf>,  // preserve tagged from before archive mode
    pub saved_index: usize,          // preserve selection index
}

#[derive(Clone)]
pub struct ArchiveEntry {
    pub name: String,
    pub full_path: String,
    pub is_dir: bool,
    pub size: u64,
}

impl App {
    pub fn is_archive_mode(&self) -> bool {
        self.archive_state.is_some()
    }

    /// Enter archive browsing mode
    pub fn enter_archive(&mut self, path: &Path) {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let all_entries = parse_archive(path, ext);
        if all_entries.is_empty() {
            self.msg_error("Cannot read archive");
            return;
        }

        // Save current state before entering archive
        let saved_tagged = self.tagged.clone();
        let saved_index = self.index;
        self.archive_state = Some(ArchiveState {
            archive_path: path.to_path_buf(),
            current_dir: String::new(),
            saved_tagged,
            saved_index,
            entries: all_entries,
        });
        self.index = 0;
        self.scroll_ix = 0;
        self.load_archive_dir();
    }

    /// Load files for current virtual directory
    pub fn load_archive_dir(&mut self) {
        let Some(state) = &self.archive_state else { return };
        let current = &state.current_dir;

        let mut dirs_seen = std::collections::HashSet::new();
        let mut entries: Vec<DirEntry> = Vec::new();

        // Add parent entry
        entries.push(DirEntry {
            name: "..".into(),
            path: PathBuf::from(".."),
            is_dir: true,
            is_symlink: false,
            is_exec: false,
            size: 0,
            modified: SystemTime::UNIX_EPOCH,
            mode: 0,
            uid: 0,
            gid: 0,
            color_code: "\x1b[38;5;12m".into(),
            tagged: false,
            search_hit: false,
        });

        for ae in &state.entries {
            let relative = if current.is_empty() {
                ae.full_path.clone()
            } else if ae.full_path.starts_with(&format!("{}/", current)) {
                ae.full_path[current.len() + 1..].to_string()
            } else {
                continue;
            };

            // Skip entries in subdirectories
            if let Some(slash_pos) = relative.find('/') {
                let dir_name = &relative[..slash_pos];
                if dirs_seen.insert(dir_name.to_string()) {
                    entries.push(DirEntry {
                        name: dir_name.to_string(),
                        path: PathBuf::from(&ae.full_path[..current.len() + if current.is_empty() { 0 } else { 1 } + slash_pos]),
                        is_dir: true,
                        is_symlink: false,
                        is_exec: false,
                        size: 0,
                        modified: SystemTime::UNIX_EPOCH,
                        mode: 0,
                        uid: 0,
                        gid: 0,
                        color_code: "\x1b[38;5;12m".into(),
                        tagged: false,
                        search_hit: false,
                    });
                }
            } else if !relative.is_empty() {
                entries.push(DirEntry {
                    name: relative.clone(),
                    path: PathBuf::from(&ae.full_path),
                    is_dir: ae.is_dir,
                    is_symlink: false,
                    is_exec: false,
                    size: ae.size,
                    modified: SystemTime::UNIX_EPOCH,
                    mode: 0,
                    uid: 0,
                    gid: 0,
                    color_code: String::new(),
                    tagged: false,
                    search_hit: false,
                });
            }
        }

        self.files = entries;
        if self.index >= self.files.len() {
            self.index = self.files.len().saturating_sub(1);
        }
    }

    /// Navigate within archive
    pub fn archive_enter(&mut self) {
        let Some(entry) = self.files.get(self.index).cloned() else { return };
        if entry.name == ".." {
            self.archive_go_up();
            return;
        }
        if entry.is_dir {
            let state = self.archive_state.as_mut().unwrap();
            if state.current_dir.is_empty() {
                state.current_dir = entry.name.clone();
            } else {
                state.current_dir = format!("{}/{}", state.current_dir, entry.name);
            }
            self.index = 0;
            self.scroll_ix = 0;
            self.load_archive_dir();
        }
    }

    /// Go up in archive, or exit archive mode
    pub fn archive_go_up(&mut self) {
        let state = self.archive_state.as_mut().unwrap();
        if state.current_dir.is_empty() {
            // Exit archive mode: restore saved state
            let archive_name = state.archive_path.file_name()
                .map(|n| n.to_string_lossy().to_string());
            let saved_tagged = state.saved_tagged.clone();
            let saved_index = state.saved_index;
            self.archive_state = None;
            self.tagged = saved_tagged;
            self.load_dir();
            // Select the archive file, or restore original index
            if let Some(name) = archive_name {
                if let Some(pos) = self.files.iter().position(|e| e.name == name) {
                    self.index = pos;
                }
            } else {
                self.index = saved_index.min(self.files.len().saturating_sub(1));
            }
        } else {
            // Go up one virtual dir
            if let Some(pos) = state.current_dir.rfind('/') {
                state.current_dir.truncate(pos);
            } else {
                state.current_dir.clear();
            }
            self.index = 0;
            self.scroll_ix = 0;
            self.load_archive_dir();
        }
    }

    /// Extract archive to current directory
    pub fn archive_extract(&mut self) {
        let Some(entry) = self.files.get(self.index) else { return };
        let path = entry.path.clone();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let cmd = match ext.to_lowercase().as_str() {
            "zip" | "jar" => format!("unzip {:?}", path),
            "tar" => format!("tar -xvf {:?}", path),
            "gz" | "tgz" => format!("tar -xzvf {:?}", path),
            "bz2" | "tbz2" => format!("tar -xjvf {:?}", path),
            "xz" | "txz" => format!("tar -xJvf {:?}", path),
            "rar" => format!("unrar x {:?}", path),
            "7z" => format!("7z x {:?}", path),
            _ => return,
        };
        let output = Command::new("sh").arg("-c").arg(&cmd).output();
        match output {
            Ok(o) if o.status.success() => {
                self.msg_success("Extracted successfully");
                self.load_dir();
            }
            _ => { self.msg_error("Extraction failed"); }
        }
    }

    /// Create archive from tagged items
    pub fn archive_create(&mut self) {
        if self.tagged.is_empty() {
            self.msg_warn("Tag files first");
            return;
        }
        let name = self.prompt("Archive name: ", "archive.tar.gz");
        if name.is_empty() { return; }

        let files: Vec<String> = self.tagged.iter()
            .map(|p| format!("{:?}", p))
            .collect();
        let files_str = files.join(" ");

        let cmd = if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            format!("tar -czvf {:?} {}", name, files_str)
        } else if name.ends_with(".tar.bz2") {
            format!("tar -cjvf {:?} {}", name, files_str)
        } else if name.ends_with(".tar.xz") {
            format!("tar -cJvf {:?} {}", name, files_str)
        } else if name.ends_with(".tar") {
            format!("tar -cvf {:?} {}", name, files_str)
        } else if name.ends_with(".zip") {
            format!("zip -r {:?} {}", name, files_str)
        } else if name.ends_with(".7z") {
            format!("7z a {:?} {}", name, files_str)
        } else {
            format!("tar -czvf {:?} {}", name, files_str)
        };

        let output = Command::new("sh").arg("-c").arg(&cmd).output();
        match output {
            Ok(o) if o.status.success() => {
                self.msg_success(&format!("Created {}", name));
                self.load_dir();
            }
            _ => { self.msg_error("Archive creation failed"); }
        }
    }
}

fn parse_archive(path: &Path, ext: &str) -> Vec<ArchiveEntry> {
    let cmd = match ext.to_lowercase().as_str() {
        "zip" | "jar" | "war" => format!("unzip -l {:?}", path),
        "tar" => format!("tar -tf {:?}", path),
        "gz" | "tgz" => format!("tar -tzf {:?}", path),
        "bz2" | "tbz2" => format!("tar -tjf {:?}", path),
        "xz" | "txz" => format!("tar -tJf {:?}", path),
        "rar" => format!("unrar lb {:?}", path),
        "7z" => format!("7z l -slt {:?}", path),
        _ => return Vec::new(),
    };

    let output = Command::new("sh").arg("-c").arg(&cmd).output().ok();
    let Some(out) = output else { return Vec::new() };
    if !out.status.success() { return Vec::new(); }

    let text = String::from_utf8_lossy(&out.stdout);

    match ext.to_lowercase().as_str() {
        "zip" | "jar" | "war" => parse_zip_listing(&text),
        "rar" => parse_simple_listing(&text),
        "7z" => parse_7z_listing(&text),
        _ => parse_tar_listing(&text),
    }
}

fn parse_tar_listing(text: &str) -> Vec<ArchiveEntry> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let name = line.trim_end_matches('/');
            let is_dir = line.ends_with('/');
            ArchiveEntry {
                name: name.rsplit('/').next().unwrap_or(name).to_string(),
                full_path: name.to_string(),
                is_dir,
                size: 0,
            }
        })
        .collect()
}

fn parse_zip_listing(text: &str) -> Vec<ArchiveEntry> {
    // unzip -l format: Length Date Time Name
    text.lines()
        .skip(3) // header lines
        .filter(|l| l.len() > 30 && !l.starts_with("---") && !l.starts_with("  "))
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(4, ' ').filter(|s| !s.is_empty()).collect();
            if parts.len() < 4 { return None; }
            let size: u64 = parts[0].parse().unwrap_or(0);
            let name = parts[3].trim().trim_end_matches('/');
            let is_dir = parts[3].trim().ends_with('/') || size == 0;
            Some(ArchiveEntry {
                name: name.rsplit('/').next().unwrap_or(name).to_string(),
                full_path: name.to_string(),
                is_dir,
                size,
            })
        })
        .collect()
}

fn parse_simple_listing(text: &str) -> Vec<ArchiveEntry> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let name = line.trim().trim_end_matches('/');
            let is_dir = line.trim().ends_with('/');
            ArchiveEntry {
                name: name.rsplit('/').next().unwrap_or(name).to_string(),
                full_path: name.to_string(),
                is_dir,
                size: 0,
            }
        })
        .collect()
}

fn parse_7z_listing(text: &str) -> Vec<ArchiveEntry> {
    let mut entries = Vec::new();
    let mut current_path = String::new();
    let mut current_size = 0u64;
    let mut is_dir = false;

    for line in text.lines() {
        if line.starts_with("Path = ") {
            if !current_path.is_empty() {
                let name = current_path.rsplit('/').next().unwrap_or(&current_path).to_string();
                entries.push(ArchiveEntry {
                    name,
                    full_path: current_path.clone(),
                    is_dir,
                    size: current_size,
                });
            }
            current_path = line[7..].to_string();
            current_size = 0;
            is_dir = false;
        } else if line.starts_with("Size = ") {
            current_size = line[7..].trim().parse().unwrap_or(0);
        } else if line.starts_with("Folder = +") {
            is_dir = true;
        }
    }
    if !current_path.is_empty() {
        let name = current_path.rsplit('/').next().unwrap_or(&current_path).to_string();
        entries.push(ArchiveEntry {
            name,
            full_path: current_path,
            is_dir,
            size: current_size,
        });
    }
    entries
}
