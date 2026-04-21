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
    pub saved_tagged: Vec<PathBuf>,  // outer tags to restore on exit
    pub origin_tagged: Vec<PathBuf>, // outer tags used as source for paste-into-archive
    pub saved_index: usize,          // preserve selection index
    pub origin_dir: PathBuf,         // cwd when archive was entered (extract destination)
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
        let origin_tagged = self.tagged.clone();
        let saved_index = self.index;
        let origin_dir = std::env::current_dir().unwrap_or_default();
        self.archive_state = Some(ArchiveState {
            archive_path: path.to_path_buf(),
            current_dir: String::new(),
            saved_tagged,
            origin_tagged,
            saved_index,
            origin_dir,
            entries: all_entries,
        });
        // Scope tags to archive mode — outer tags are restored on exit.
        self.tagged.clear();
        self.tagged_size_cache = None;
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

    /// Collect virtual paths to operate on: tagged (inside archive) or selected.
    /// Never returns "..".
    fn archive_op_paths(&self) -> Vec<String> {
        if !self.tagged.is_empty() {
            return self.tagged.iter()
                .map(|p| p.to_string_lossy().to_string())
                .filter(|s| s != "..")
                .collect();
        }
        if let Some(entry) = self.files.get(self.index) {
            if entry.name != ".." {
                return vec![entry.path.to_string_lossy().to_string()];
            }
        }
        Vec::new()
    }

    /// Re-parse archive contents from disk and reload current virtual dir.
    pub fn archive_refresh(&mut self) {
        let Some(state) = self.archive_state.as_ref() else { return };
        let archive_path = state.archive_path.clone();
        let ext = archive_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        let new_entries = parse_archive(&archive_path, &ext);
        if let Some(state) = self.archive_state.as_mut() {
            state.entries = new_entries;
        }
        self.tagged.clear();
        self.tagged_size_cache = None;
        self.load_archive_dir();
    }

    /// Delete tagged/selected entries from the archive on disk.
    pub fn archive_delete_entries(&mut self) {
        if !self.is_archive_mode() { return; }
        let paths = self.archive_op_paths();
        if paths.is_empty() { return; }
        let archive_path = self.archive_state.as_ref().unwrap().archive_path.clone();

        let mut lines = vec![
            style::fg("Delete from Archive", 196),
            "=".repeat(40),
            String::new(),
            format!("Archive: {}", archive_path.file_name().unwrap_or_default().to_string_lossy()),
            String::new(),
            style::fg("Items to delete:", 226),
        ];
        for p in paths.iter().take(10) { lines.push(format!("  {}", p)); }
        if paths.len() > 10 { lines.push(format!("  ... and {} more", paths.len() - 10)); }
        lines.push(String::new());
        lines.push(style::fg("This modifies the archive file permanently!", 196));
        self.show_in_right(&lines.join("\n"));
        self.status.say(&style::fg(&format!(" Delete {} item(s) from archive? (y/n)", paths.len()), 196));

        let Some(key) = crust::Input::getchr(None) else { return };
        if key != "y" && key != "Y" {
            self.msg_cancel();
            return;
        }

        let ext = archive_ext(&archive_path);
        let success = match ext.as_str() {
            "zip" | "jar" | "war" => run_cmd("zip", &[&["-d".into(), archive_path.to_string_lossy().to_string()][..], &paths[..]].concat()),
            "rar" => run_cmd("rar", &[&["d".into(), archive_path.to_string_lossy().to_string()][..], &paths[..]].concat()),
            "7z" => run_cmd("7z", &[&["d".into(), archive_path.to_string_lossy().to_string()][..], &paths[..]].concat()),
            _ => archive_tar_modify(&archive_path, TarAction::Delete(paths.clone()), ""),
        };

        if success {
            self.msg_success(&format!("Deleted {} item(s) from archive", paths.len()));
            self.archive_refresh();
        } else {
            self.msg_error("Delete failed");
        }
    }

    /// Extract tagged/selected entries to the directory where we entered the archive.
    pub fn archive_extract_entries(&mut self) {
        if !self.is_archive_mode() { return; }
        let paths = self.archive_op_paths();
        if paths.is_empty() { return; }
        let state = self.archive_state.as_ref().unwrap();
        let archive_path = state.archive_path.clone();
        let dest = state.origin_dir.clone();
        let archive_s = archive_path.to_string_lossy().to_string();
        let dest_s = dest.to_string_lossy().to_string();

        self.msg_info(&format!("Extracting {} item(s) to {}...", paths.len(), dest.display()));

        let ext = archive_ext(&archive_path);
        let success = match ext.as_str() {
            "zip" | "jar" | "war" => {
                let mut args: Vec<String> = vec!["-o".into(), archive_s.clone()];
                args.extend(paths.iter().cloned());
                args.push("-d".into());
                args.push(dest_s.clone());
                run_cmd("unzip", &args)
            }
            "rar" => {
                let mut args: Vec<String> = vec!["x".into(), "-o+".into(), archive_s.clone()];
                args.extend(paths.iter().cloned());
                args.push(format!("{}/", dest_s));
                run_cmd("unrar", &args)
            }
            "7z" => {
                let mut args: Vec<String> = vec!["x".into(), archive_s.clone()];
                args.extend(paths.iter().cloned());
                args.push(format!("-o{}", dest_s));
                args.push("-y".into());
                run_cmd("7z", &args)
            }
            _ => {
                // tar variants: tar xf ARCHIVE -C DEST PATHS...
                let flag = tar_decompress_flag(&archive_path);
                let mut args: Vec<String> = Vec::new();
                args.push(format!("x{}f", flag));
                args.push(archive_s.clone());
                args.push("-C".into());
                args.push(dest_s.clone());
                args.extend(paths.iter().cloned());
                run_cmd("tar", &args)
            }
        };

        if success {
            self.msg_success(&format!("Extracted {} item(s) to {}", paths.len(), dest.display()));
            self.tagged.clear();
            self.tagged_size_cache = None;
        } else {
            self.msg_error("Extraction failed");
        }
    }

    /// Add files (tagged before entering archive) into the current virtual directory.
    pub fn archive_add_files(&mut self) {
        if !self.is_archive_mode() { return; }
        let state = self.archive_state.as_ref().unwrap();
        if state.origin_tagged.is_empty() {
            self.msg_warn("Tag files first, then enter the archive to add them");
            return;
        }
        let archive_path = state.archive_path.clone();
        let current_dir = state.current_dir.clone();
        let files: Vec<PathBuf> = state.origin_tagged.iter()
            .filter(|p| p.exists())
            .cloned()
            .collect();
        if files.is_empty() {
            self.msg_error("Tagged files no longer exist");
            return;
        }

        let target = if current_dir.is_empty() { "archive root".into() } else { current_dir.clone() };
        let mut lines = vec![
            style::fg("Add Files to Archive", 226),
            "=".repeat(40),
            String::new(),
            format!("Archive: {}", archive_path.file_name().unwrap_or_default().to_string_lossy()),
            format!("Target:  {}", target),
            String::new(),
            style::fg("Files to add:", 226),
        ];
        for f in files.iter().take(10) {
            lines.push(format!("  {}", f.file_name().unwrap_or_default().to_string_lossy()));
        }
        if files.len() > 10 { lines.push(format!("  ... and {} more", files.len() - 10)); }
        self.show_in_right(&lines.join("\n"));
        self.status.say(&style::fg(&format!(" Add {} file(s) to archive? (y/n)", files.len()), 226));

        let Some(key) = crust::Input::getchr(None) else { return };
        if key != "y" && key != "Y" {
            self.msg_cancel();
            return;
        }

        let ext = archive_ext(&archive_path);
        let success = match ext.as_str() {
            "zip" | "jar" | "war" => {
                if current_dir.is_empty() {
                    // Junk paths so files land in archive root without leading dirs
                    let mut args: Vec<String> = vec!["-j".into(), archive_path.to_string_lossy().to_string()];
                    for f in &files { args.push(f.to_string_lossy().to_string()); }
                    run_cmd("zip", &args)
                } else {
                    add_to_subdir_via_tempdir(&archive_path, &current_dir, &files, AddKind::Zip)
                }
            }
            "rar" => {
                let mut args: Vec<String> = vec!["a".into()];
                if !current_dir.is_empty() {
                    args.push(format!("-ap{}", current_dir));
                }
                args.push(archive_path.to_string_lossy().to_string());
                for f in &files { args.push(f.to_string_lossy().to_string()); }
                run_cmd("rar", &args)
            }
            "7z" => {
                if current_dir.is_empty() {
                    let mut args: Vec<String> = vec!["a".into(), archive_path.to_string_lossy().to_string()];
                    for f in &files { args.push(f.to_string_lossy().to_string()); }
                    run_cmd("7z", &args)
                } else {
                    add_to_subdir_via_tempdir(&archive_path, &current_dir, &files, AddKind::SevenZ)
                }
            }
            _ => archive_tar_modify(&archive_path, TarAction::Add { files: files.clone(), target_dir: current_dir.clone() }, ""),
        };

        if success {
            self.msg_success(&format!("Added {} file(s) to archive", files.len()));
            // Clear origin_tagged so a second paste doesn't duplicate
            if let Some(state) = self.archive_state.as_mut() {
                state.origin_tagged.clear();
                state.saved_tagged.clear();
            }
            self.archive_refresh();
        } else {
            self.msg_error("Add failed");
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

/// Return lowercased archive extension, handling .tar.gz / .tar.bz2 / .tar.xz / .tar.zst.
fn archive_ext(path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    for s in [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst"] {
        if name.ends_with(s) { return s.trim_start_matches('.').to_string(); }
    }
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}

fn tar_decompress_flag(path: &Path) -> &'static str {
    match archive_ext(path).as_str() {
        "tar.gz" | "tgz" | "gz"     => "z",
        "tar.bz2" | "tbz2" | "tbz" | "bz2" => "j",
        "tar.xz"  | "txz"  | "xz"   => "J",
        "tar.zst"                   => "",
        _ => "",
    }
}

fn tar_compress_flag(path: &Path) -> &'static str { tar_decompress_flag(path) }

/// Uses zstd flag separately since it's a long option, not a single letter.
fn tar_is_zstd(path: &Path) -> bool {
    archive_ext(path).as_str() == "tar.zst"
}

fn run_cmd(bin: &str, args: &[String]) -> bool {
    Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

enum TarAction {
    Delete(Vec<String>),
    Add { files: Vec<PathBuf>, target_dir: String },
}

/// tar doesn't support in-place edits for compressed archives: extract to a
/// temp dir, mutate there, re-archive to the original path.
fn archive_tar_modify(archive: &Path, action: TarAction, _reserved: &str) -> bool {
    let tmp = match tempdir() {
        Some(t) => t,
        None => return false,
    };
    let tmp_path = tmp.clone();
    let archive_s = archive.to_string_lossy().to_string();
    let tmp_s = tmp_path.to_string_lossy().to_string();

    // Extract everything into tmp
    let ok = if tar_is_zstd(archive) {
        run_cmd("tar", &["--zstd".into(), "-xf".into(), archive_s.clone(), "-C".into(), tmp_s.clone()])
    } else {
        let flag = tar_decompress_flag(archive);
        run_cmd("tar", &[format!("x{}f", flag), archive_s.clone(), "-C".into(), tmp_s.clone()])
    };
    if !ok {
        let _ = std::fs::remove_dir_all(&tmp_path);
        return false;
    }

    match action {
        TarAction::Delete(paths) => {
            for p in paths {
                let target = tmp_path.join(&p);
                let _ = std::fs::remove_dir_all(&target);
                let _ = std::fs::remove_file(&target);
            }
        }
        TarAction::Add { files, target_dir } => {
            let dest = if target_dir.is_empty() { tmp_path.clone() } else { tmp_path.join(&target_dir) };
            if std::fs::create_dir_all(&dest).is_err() {
                let _ = std::fs::remove_dir_all(&tmp_path);
                return false;
            }
            for f in files {
                let name = f.file_name().unwrap_or_default();
                let target = dest.join(name);
                if f.is_dir() {
                    let _ = copy_dir_recursive(&f, &target);
                } else {
                    let _ = std::fs::copy(&f, &target);
                }
            }
        }
    }

    // Re-archive everything in tmp into the original archive path.
    // We cd into tmp and bundle all top-level entries, so paths stay relative.
    let entries: Vec<String> = match std::fs::read_dir(&tmp_path) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string())).collect(),
        Err(_) => { let _ = std::fs::remove_dir_all(&tmp_path); return false; }
    };
    if entries.is_empty() {
        // Nothing to archive — still write an empty tar so the file stays valid.
        let _ = std::fs::remove_dir_all(&tmp_path);
        return false;
    }

    let ok = if tar_is_zstd(archive) {
        let mut args: Vec<String> = vec!["--zstd".into(), "-cf".into(), archive_s.clone(), "-C".into(), tmp_s.clone()];
        args.extend(entries);
        run_cmd("tar", &args)
    } else {
        let flag = tar_compress_flag(archive);
        let mut args: Vec<String> = vec![format!("c{}f", flag), archive_s.clone(), "-C".into(), tmp_s.clone()];
        args.extend(entries);
        run_cmd("tar", &args)
    };

    let _ = std::fs::remove_dir_all(&tmp_path);
    ok
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if ft.is_symlink() {
            let link = std::fs::read_link(&src_path)?;
            let _ = std::os::unix::fs::symlink(link, &dst_path);
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

enum AddKind { Zip, SevenZ }

/// zip/7z add files to a subdirectory by building the layout in a tempdir
/// and then pointing the archiver at the resulting relative paths.
fn add_to_subdir_via_tempdir(archive: &Path, subdir: &str, files: &[PathBuf], kind: AddKind) -> bool {
    let Some(tmp) = tempdir() else { return false; };
    let target = tmp.join(subdir);
    if std::fs::create_dir_all(&target).is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
        return false;
    }
    for f in files {
        let name = f.file_name().unwrap_or_default();
        let dest = target.join(name);
        if f.is_dir() {
            let _ = copy_dir_recursive(f, &dest);
        } else {
            let _ = std::fs::copy(f, &dest);
        }
    }
    // Run the archiver with cwd=tmp so relative paths like "subdir/name" match.
    let archive_s = archive.to_string_lossy().to_string();
    let mut args: Vec<String> = match kind {
        AddKind::Zip => vec!["-r".into(), archive_s.clone()],
        AddKind::SevenZ => vec!["a".into(), archive_s.clone()],
    };
    for f in files {
        let name = f.file_name().unwrap_or_default().to_string_lossy().to_string();
        args.push(format!("{}/{}", subdir, name));
    }
    let bin = match kind { AddKind::Zip => "zip", AddKind::SevenZ => "7z" };
    let ok = Command::new(bin)
        .args(&args)
        .current_dir(&tmp)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = std::fs::remove_dir_all(&tmp);
    ok
}

fn tempdir() -> Option<PathBuf> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_nanos();
    let pid = std::process::id();
    let base = std::env::temp_dir().join(format!("pointer_arc_{}_{}", pid, nanos));
    std::fs::create_dir_all(&base).ok()?;
    Some(base)
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
