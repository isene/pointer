use crate::app::App;
use crate::preview::{is_image_ext, is_video_ext};

impl App {
    /// Toggle image preview
    pub fn toggle_image(&mut self) {
        if self.image_display.is_some() {
            self.clear_image();
            self.image_display = None;
            self.msg_info("Image preview: off");
        } else {
            self.image_display = Some(glow::Display::new());
            if self.image_display.as_ref().map(|d| d.supported()).unwrap_or(false) {
                self.msg_success("Image preview: on");
                // Catch up: enabling mid-dir, kick the full precache
                // so the rest of the dir is hot when the cursor moves.
                self.precache_dir_images();
            } else {
                self.msg_warn("Image preview: no supported protocol");
            }
            self.prev_selected = None;
        }
    }

    /// Show image if selected file is an image (or a video, via
    /// ffmpegthumbnailer frame extraction) and display is active.
    pub fn show_image_if_applicable(&mut self) {
        // Burst suppression. On rapid Up/Down (key autorepeat), each
        // keystroke pushes a kitty transmit-plus-place down the pty.
        // Even with monotonic ids and the double-tap place, kitty's
        // input buffer can back up enough that a chunked transmit
        // hasn't finished assembling when the next clear arrives —
        // the image briefly appears then vanishes, or never appears.
        // Skip the show entirely while there's another key already
        // waiting in the queue. The render after the LAST key in the
        // burst will paint the actual image. Saves a lot of kitty
        // bandwidth too: typing through a 50-file directory used to
        // stream 50 PNGs the user never sees.
        if crust::Input::peek_pending() {
            return;
        }

        let Some(ref mut display) = self.image_display else { return };
        if !display.supported() { return; }

        let Some(entry) = self.files.get(self.index) else { return };
        let ext = entry.path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // Resolve to absolute path.
        let path = if entry.path.is_absolute() {
            entry.path.clone()
        } else {
            std::env::current_dir().unwrap_or_default().join(&entry.path)
        };

        let shown_path = if is_image_ext(ext) {
            path.to_string_lossy().to_string()
        } else if is_video_ext(ext) {
            // Generate a thumbnail with ffmpegthumbnailer (same as RTFM).
            // Cached per-source via simple hash so repeats are instant.
            let hash = simple_hash(&path.to_string_lossy());
            let tn = std::path::PathBuf::from(format!("/tmp/pointer_video_tn_{}.jpg", hash));
            if !tn.exists() {
                let ok = std::process::Command::new("ffmpegthumbnailer")
                    .args(["-s", "1200", "-q", "10", "-i"])
                    .arg(&path)
                    .arg("-o")
                    .arg(&tn)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !ok || !tn.exists() { return; }
            }
            tn.to_string_lossy().to_string()
        } else {
            return;
        };

        // Clear text content before showing image to prevent bleed-through.
        self.right.set_text("");
        self.right.full_refresh();

        // Kitty pipeline forks `convert` for the initial resize and
        // for the cell-alignment pad step. Either subprocess can fail
        // transiently under resource pressure — most often while the
        // background `preconvert_adjacent_images` thread is also
        // forking `convert` on neighbouring images. `display.show`
        // returns false on any such failure; the image silently fails
        // to appear until the user presses Enter (which re-runs the
        // same call when the contention's gone).
        //
        // Retry once with a small delay — cheap insurance that catches
        // the ~1/20 race without affecting the happy path.
        let x = self.right.x;
        let y = self.right.y;
        let w = self.right.w;
        let h = self.right.h;
        if !display.show(&shown_path, x, y, w, h) {
            std::thread::sleep(std::time::Duration::from_millis(30));
            let _ = display.show(&shown_path, x, y, w, h);
        }

        self.preconvert_adjacent_images();
    }

    /// Pre-convert nearby images AND pre-generate nearby video thumbnails
    /// so scrolling through mixed media dirs is instant.
    fn preconvert_adjacent_images(&self) {
        use std::sync::atomic::Ordering;
        if self.preload_busy.load(Ordering::Relaxed) { return; }

        let Some(ref display) = self.image_display else { return };
        if !display.supported() { return; }

        let (cell_w, cell_h) = glow::get_cell_size();
        if cell_w == 0 || cell_h == 0 { return; }
        let pixel_w = self.right.w as u32 * cell_w as u32;
        let pixel_h = self.right.h as u32 * cell_h as u32;

        let cwd = std::env::current_dir().unwrap_or_default();
        let mut image_paths = Vec::new();
        let mut video_jobs: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
        for offset in [1i32, 2, -1] {
            let idx = self.index as i32 + offset;
            if idx >= 0 && (idx as usize) < self.files.len() {
                let e = &self.files[idx as usize];
                let ext = e.path.extension().and_then(|x| x.to_str()).unwrap_or("");
                let p = if e.path.is_absolute() { e.path.clone() } else { cwd.join(&e.path) };
                if is_image_ext(ext) {
                    image_paths.push(p.to_string_lossy().to_string());
                } else if is_video_ext(ext) {
                    let hash = simple_hash(&p.to_string_lossy());
                    let tn = std::path::PathBuf::from(format!("/tmp/pointer_video_tn_{}.jpg", hash));
                    if !tn.exists() {
                        video_jobs.push((p, tn));
                    } else {
                        // Already cached; feed into the PNG preconvert pass
                        // so glow can have the PNG ready for instant show.
                        image_paths.push(tn.to_string_lossy().to_string());
                    }
                }
            }
        }
        if image_paths.is_empty() && video_jobs.is_empty() { return; }

        let cache = display.png_cache.clone();
        let busy = self.preload_busy.clone();
        busy.store(true, Ordering::Relaxed);
        std::thread::spawn(move || {
            // Generate video thumbnails first (disk ops), then feed the
            // resulting JPGs into the image pre-convert alongside real images.
            let mut all_paths = image_paths;
            for (src, tn) in video_jobs {
                let ok = std::process::Command::new("ffmpegthumbnailer")
                    .args(["-s", "1200", "-q", "10", "-i"])
                    .arg(&src)
                    .arg("-o")
                    .arg(&tn)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if ok && tn.exists() {
                    all_paths.push(tn.to_string_lossy().to_string());
                }
            }
            glow::preconvert_images(&all_paths, pixel_w, pixel_h,
                                    cell_w, cell_h, &cache, None);
            busy.store(false, Ordering::Relaxed);
        });
    }

    /// Walk the whole current directory in proximity-to-cursor order
    /// and background-precache every image into glow's PngCache,
    /// cell-aligned and ready to transmit. Triggered on directory
    /// change. Cancellable: setting `preload_cancel` makes the
    /// worker thread bail at the next path boundary so a fresh
    /// `cd` doesn't keep grinding through the old dir's tail.
    pub fn precache_dir_images(&self) {
        use std::sync::atomic::Ordering;

        let Some(ref display) = self.image_display else { return };
        if !display.supported() { return; }

        let (cell_w, cell_h) = glow::get_cell_size();
        if cell_w == 0 || cell_h == 0 { return; }
        let pixel_w = self.right.w as u32 * cell_w as u32;
        let pixel_h = self.right.h as u32 * cell_h as u32;
        if pixel_w == 0 || pixel_h == 0 { return; }

        // Ask any in-flight precache to stop. The new spawn below
        // will set cancel = false again right before launching, so
        // the prior thread observes "stop" while ours observes "go".
        self.preload_cancel.store(true, Ordering::Relaxed);

        // Don't start a second worker on top of one already running;
        // it would race on the cancel flag. If a prior worker is
        // mid-convert, we let it finish that one image and notice
        // the cancel — but we skip starting our own this tick. The
        // next selection/render tick will retry naturally.
        if self.preload_busy.load(Ordering::Relaxed) { return; }

        let cwd = std::env::current_dir().unwrap_or_default();
        let center = self.index;
        let mut ranked: Vec<(usize, String)> = Vec::new();
        for (idx, entry) in self.files.iter().enumerate() {
            let ext = entry.path.extension().and_then(|x| x.to_str()).unwrap_or("");
            if !is_image_ext(ext) { continue; }
            let p = if entry.path.is_absolute() {
                entry.path.clone()
            } else {
                cwd.join(&entry.path)
            };
            let dist = if idx >= center { idx - center } else { center - idx };
            ranked.push((dist, p.to_string_lossy().to_string()));
        }
        // Cap the work: dirs with > 200 images get the closest 200
        // around the cursor. Beyond that the user is unlikely to
        // scroll-through-all in a session, and we don't want a
        // 10000-image gallery pinning a CPU core for minutes.
        ranked.sort_by_key(|(d, _)| *d);
        ranked.truncate(200);
        if ranked.is_empty() { return; }
        let paths: Vec<String> = ranked.into_iter().map(|(_, p)| p).collect();

        let cache = display.png_cache.clone();
        let busy = self.preload_busy.clone();
        let cancel = self.preload_cancel.clone();
        cancel.store(false, Ordering::Relaxed); // arm for this run
        busy.store(true, Ordering::Relaxed);
        std::thread::spawn(move || {
            glow::preconvert_images(&paths, pixel_w, pixel_h,
                                    cell_w, cell_h, &cache, Some(&cancel));
            busy.store(false, Ordering::Relaxed);
        });
    }

    /// Clear displayed image
    pub fn clear_image(&mut self) {
        let Some(ref mut display) = self.image_display else { return };
        display.clear(self.right.x, self.right.y, self.right.w, self.right.h, self.cols, self.rows);
    }
}

fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
