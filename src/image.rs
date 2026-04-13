use crate::app::App;
use crate::preview::is_image_ext;

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
            } else {
                self.msg_warn("Image preview: no supported protocol");
            }
            self.prev_selected = None;
        }
    }

    /// Show image if selected file is an image and display is active
    pub fn show_image_if_applicable(&mut self) {
        let Some(ref mut display) = self.image_display else { return };
        if !display.supported() { return; }

        let Some(entry) = self.files.get(self.index) else { return };
        let ext = entry.path.extension().and_then(|e| e.to_str()).unwrap_or("");

        if !is_image_ext(ext) {
            return;
        }

        // Resolve to absolute path for ImageMagick
        let path = if entry.path.is_absolute() {
            entry.path.clone()
        } else {
            std::env::current_dir().unwrap_or_default().join(&entry.path)
        };
        let path_str = path.to_string_lossy().to_string();

        // Content area IS the pane area (border is outside)
        display.show(&path_str, self.right.x, self.right.y, self.right.w, self.right.h);

        // Pre-convert adjacent images in background
        self.preconvert_adjacent_images();
    }

    /// Pre-convert nearby images so scrolling through image dirs is instant
    fn preconvert_adjacent_images(&self) {
        use std::sync::atomic::Ordering;
        if self.preload_busy.load(Ordering::Relaxed) { return; }

        let Some(ref display) = self.image_display else { return };
        if !display.supported() { return; }

        let (cell_w, _) = glow::get_cell_size();
        if cell_w == 0 { return; }
        let pixel_w = self.right.w as u32 * cell_w as u32;

        let cwd = std::env::current_dir().unwrap_or_default();
        let mut paths = Vec::new();
        for offset in [1i32, 2, -1] {
            let idx = self.index as i32 + offset;
            if idx >= 0 && (idx as usize) < self.files.len() {
                let e = &self.files[idx as usize];
                let ext = e.path.extension().and_then(|x| x.to_str()).unwrap_or("");
                if is_image_ext(ext) {
                    let p = if e.path.is_absolute() { e.path.clone() } else { cwd.join(&e.path) };
                    paths.push(p.to_string_lossy().to_string());
                }
            }
        }
        if paths.is_empty() { return; }

        let cache = display.png_cache.clone();
        let busy = self.preload_busy.clone();
        busy.store(true, Ordering::Relaxed);
        std::thread::spawn(move || {
            glow::preconvert_images(&paths, pixel_w, &cache);
            busy.store(false, Ordering::Relaxed);
        });
    }

    /// Clear displayed image
    pub fn clear_image(&mut self) {
        let Some(ref mut display) = self.image_display else { return };
        display.clear(self.right.x, self.right.y, self.right.w, self.right.h, self.cols, self.rows);
    }
}
