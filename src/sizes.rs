//! Folder sizes: how much disk each folder takes, biggest first.
//!
//! `B` walks the tree under the folder on screen, once, on a thread of
//! its own, and keeps the total of every folder in it. While the sizes
//! are on, the list is sorted by them, and moving into a folder costs
//! nothing: its numbers are known already.

use crate::app::App;
use crate::entry::format_size;
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// The totals of one walk.
pub struct Sizes {
    pub root: PathBuf,
    /// Bytes on disk of every folder under `root`, and of `root`.
    pub dirs: HashMap<PathBuf, u64>,
}

/// A walk that is still running.
pub struct Count {
    root: PathBuf,
    seen: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<HashMap<PathBuf, u64>>,
}

/// The bytes a file takes on disk: its blocks, the way `du` counts.
pub fn on_disk(meta: &std::fs::Metadata) -> u64 {
    meta.blocks() * 512
}

/// Add up the tree under `root`. Links are not followed, and a folder on
/// another disk is not entered, so a walk from `/` stays off `/proc` and
/// off network drives.
pub fn walk(root: &Path, seen: &AtomicU64, stop: &AtomicBool) -> HashMap<PathBuf, u64> {
    let mut out = HashMap::new();
    if let Ok(meta) = std::fs::symlink_metadata(root) {
        walk_dir(root, meta.dev(), on_disk(&meta), seen, stop, &mut out);
    }
    out
}

fn walk_dir(
    dir: &Path, dev: u64, own: u64,
    seen: &AtomicU64, stop: &AtomicBool,
    out: &mut HashMap<PathBuf, u64>,
) -> u64 {
    let mut total = own;
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.flatten() {
            if stop.load(Ordering::Relaxed) { break; }
            // The entry's own data: a link counts as a link.
            let Ok(meta) = entry.metadata() else { continue };
            seen.fetch_add(1, Ordering::Relaxed);
            if !meta.is_dir() {
                total += on_disk(&meta);
            } else if meta.dev() == dev {
                total += walk_dir(&entry.path(), dev, on_disk(&meta), seen, stop, out);
            }
        }
    }
    out.insert(dir.to_path_buf(), total);
    total
}

/// A count with a dot between the thousands: 1.234.567.
fn dotted(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 { out.push('.'); }
        out.push(c);
    }
    out
}

impl App {
    /// `B`: count the folder on screen, or turn the sizes off again.
    pub fn toggle_sizes(&mut self) {
        if let Some(count) = self.counting.take() {
            count.stop.store(true, Ordering::Relaxed);
            self.msg_info("Counting stopped");
            return;
        }
        if self.sizes.take().is_some() {
            self.reload_and_render();
            self.msg_info("Folder sizes off");
            return;
        }
        let root = std::env::current_dir().unwrap_or_default();
        let seen = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (walk_root, walk_seen, walk_stop) = (root.clone(), seen.clone(), stop.clone());
        let thread = std::thread::spawn(move || walk(&walk_root, &walk_seen, &walk_stop));
        self.counting = Some(Count { root, seen, stop, thread });
        self.msg_info("Counting folder sizes (B stops)");
    }

    pub fn sizes_running(&self) -> bool {
        self.counting.is_some()
    }

    /// Called from the main loop while a count runs: show how far it is,
    /// and take its totals when it is done.
    pub fn check_sizes(&mut self) {
        let Some(count) = self.counting.as_ref() else { return };
        let seen = count.seen.load(Ordering::Relaxed);
        if !count.thread.is_finished() {
            self.msg_info(&format!("Counting folder sizes: {} files (B stops)", dotted(seen)));
            return;
        }
        let Some(count) = self.counting.take() else { return };
        let Ok(dirs) = count.thread.join() else { return };
        let total = dirs.get(&count.root).copied().unwrap_or(0);
        self.sizes = Some(Sizes { root: count.root, dirs });
        self.reload_and_render();
        self.msg_success(&format!(
            "{} in {} files, biggest first (B turns the sizes off)", format_size(total), dotted(seen)));
    }

    /// Give the rows of the folder on screen their size on disk and put
    /// the biggest first. Does nothing outside the tree that was counted.
    pub fn apply_sizes(&mut self, cwd: &Path) {
        self.sizes_shown = false;
        let Some(sizes) = self.sizes.as_ref() else { return };
        if !cwd.starts_with(&sizes.root) { return; }
        for entry in &mut self.files {
            entry.size = if entry.is_dir && !entry.is_symlink {
                sizes.dirs.get(&entry.path).copied().unwrap_or(0)
            } else {
                std::fs::symlink_metadata(&entry.path).map(|m| on_disk(&m)).unwrap_or(0)
            };
        }
        let invert = self.sort_invert;
        self.files.sort_by(|a, b| {
            let order = b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name));
            if invert { order.reverse() } else { order }
        });
        self.sizes_shown = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_the_sum_of_what_is_in_it_and_a_link_is_not_followed() {
        let root = std::env::temp_dir().join(format!("pointer-sizes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("big/deep")).unwrap();
        std::fs::create_dir_all(root.join("small")).unwrap();
        std::fs::write(root.join("big/deep/a.bin"), vec![1u8; 300_000]).unwrap();
        std::fs::write(root.join("big/b.bin"), vec![1u8; 100_000]).unwrap();
        std::fs::write(root.join("small/c.bin"), vec![1u8; 10_000]).unwrap();
        // A link back to the big folder must not count it twice.
        std::os::unix::fs::symlink(root.join("big"), root.join("small/link")).unwrap();

        let seen = AtomicU64::new(0);
        let dirs = walk(&root, &seen, &AtomicBool::new(false));
        let (big, deep, small, all) = (dirs[&root.join("big")], dirs[&root.join("big/deep")],
                                       dirs[&root.join("small")], dirs[&root]);
        assert!(deep >= 300_000 && deep < 400_000, "{deep}");
        assert!(big >= deep + 100_000, "{big}");
        assert!(small >= 10_000 && small < 100_000, "{small}");
        assert!(all >= big + small, "{all}");
        assert!(!dirs.contains_key(&root.join("small/link")));
        assert_eq!(seen.load(Ordering::Relaxed), 7);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_stopped_walk_ends_and_a_count_reads_with_dots() {
        let seen = AtomicU64::new(0);
        let dirs = walk(Path::new("/usr"), &seen, &AtomicBool::new(true));
        assert!(seen.load(Ordering::Relaxed) == 0 && dirs.len() == 1);
        assert_eq!((dotted(7).as_str(), dotted(1234).as_str(), dotted(1_234_567).as_str()),
                   ("7", "1.234", "1.234.567"));
    }
}
