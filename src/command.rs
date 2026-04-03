use crate::app::App;
use crust::style;
use std::env;
use std::process::Command;

impl App {
    /// Enter command mode
    pub fn command_mode(&mut self) {
        self.status.prompt = ":".into();
        self.status.record = true;
        let cmd = self.prompt(":", "");
        if cmd.is_empty() { return; }

        // Save to history
        if self.state.history.last().map(|h| h != &cmd).unwrap_or(true) {
            self.state.history.push(cmd.clone());
            if self.state.history.len() > 200 {
                self.state.history.remove(0);
            }
        }

        // Expand @s (selected) and @t (tagged)
        let expanded = self.expand_vars(&cmd);

        // Handle cd
        if expanded.starts_with("cd ") {
            let dir = expanded[3..].trim();
            let target = if dir.starts_with('~') {
                let home = env::var("HOME").unwrap_or_default();
                dir.replacen('~', &home, 1)
            } else {
                dir.to_string()
            };
            self.save_dir_index();
            if env::set_current_dir(&target).is_ok() {
                self.index = 0;
                self.scroll_ix = 0;
                self.prev_selected = None;
                self.load_dir();
            } else {
                self.msg_error(&format!("cd: no such directory: {}", target));
            }
            return;
        }

        // Check if interactive
        let prog = expanded.split_whitespace().next().unwrap_or("");
        if self.config.interactive.iter().any(|p| p == prog) {
            self.run_interactive(&expanded);
            self.load_dir();
            return;
        }

        // Run command, show output in right pane
        let output = Command::new("sh")
            .arg("-c")
            .arg(&expanded)
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let mut result = stdout.to_string();
                if !stderr.is_empty() {
                    if !result.is_empty() { result.push('\n'); }
                    result.push_str(&style::fg(&stderr, 196));
                }
                if result.is_empty() {
                    self.msg_info(&format!("Exit: {}", out.status.code().unwrap_or(-1)));
                } else {
                    self.show_in_right(&result);
                }
            }
            Err(e) => {
                self.msg_error(&format!("{}", e));
            }
        }
        self.load_dir();
    }

    /// Show command history in right pane
    pub fn command_history(&mut self) {
        if self.state.history.is_empty() {
            self.show_in_right(" No command history");
            return;
        }
        let lines: Vec<String> = std::iter::once(style::bold("Command History"))
            .chain(std::iter::once(String::new()))
            .chain(self.state.history.iter().rev().enumerate().map(|(i, cmd)| {
                format!("  {} {}", style::fg(&format!("{:3}", i + 1), 245), cmd)
            }))
            .collect();
        self.show_in_right(&lines.join("\n"));
    }

    fn expand_vars(&self, cmd: &str) -> String {
        let mut result = cmd.to_string();
        // @s -> selected file path
        if let Some(entry) = self.files.get(self.index) {
            result = result.replace("@s", &shell_escape(&entry.path.to_string_lossy()));
        }
        // @t -> tagged file paths
        if !self.tagged.is_empty() {
            let tagged_str: String = self.tagged.iter()
                .map(|p| shell_escape(&p.to_string_lossy()))
                .collect::<Vec<_>>()
                .join(" ");
            result = result.replace("@t", &tagged_str);
        }
        result
    }

    /// Copy path to primary selection
    pub fn yank_primary(&self) {
        if let Some(entry) = self.files.get(self.index) {
            let path = entry.path.to_string_lossy().to_string();
            clipboard_copy(&path, "primary");
        }
    }

    /// Copy path to clipboard
    pub fn yank_clipboard(&self) {
        if let Some(entry) = self.files.get(self.index) {
            let path = entry.path.to_string_lossy().to_string();
            clipboard_copy(&path, "clipboard");
        }
    }

    /// Copy right pane content to clipboard
    pub fn yank_right_pane(&mut self) {
        let content = self.right.text().to_string();
        if content.is_empty() {
            self.msg_info("Right pane is empty");
            return;
        }
        let plain = crust::strip_ansi(&content);
        clipboard_copy(&plain, "clipboard");
        self.msg_info("Right pane content copied to clipboard");
    }

    /// Add program to interactive list
    pub fn add_interactive(&mut self) {
        let prog = self.prompt("Add to interactive: ", "");
        if prog.is_empty() { return; }
        if !self.config.interactive.contains(&prog) {
            self.config.interactive.push(prog.clone());
            self.config.save();
            self.msg_success(&format!("Added '{}' to interactive list", prog));
        } else {
            self.msg_info(&format!("'{}' already in interactive list", prog));
        }
    }

    /// Script evaluator (@ mode). Runs a command with pointer context as env vars:
    ///   POINTER_SELECTED  - full path of selected item
    ///   POINTER_DIR       - current working directory
    ///   POINTER_TAGGED    - newline-separated list of tagged paths
    ///   POINTER_INDEX     - selected index (0-based)
    ///   POINTER_COUNT     - number of files in listing
    ///   POINTER_CONTEXT   - JSON object with all of the above
    /// Output is shown in the right pane. If the script writes to stderr,
    /// lines starting with "cd:" trigger a directory change,
    /// "select:" selects a file, "status:" shows a status message.
    pub fn eval_mode(&mut self) {
        let cmd = self.prompt("@ ", "");
        if cmd.is_empty() { return; }

        let selected = self.files.get(self.index)
            .map(|e| e.path.to_string_lossy().to_string())
            .unwrap_or_default();
        let cwd = std::env::current_dir().unwrap_or_default().to_string_lossy().to_string();
        let tagged: Vec<String> = self.tagged.iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        let context = serde_json::json!({
            "selected": selected,
            "directory": cwd,
            "tagged": tagged,
            "index": self.index,
            "count": self.files.len(),
        });

        let output = Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .env("POINTER_SELECTED", &selected)
            .env("POINTER_DIR", &cwd)
            .env("POINTER_TAGGED", tagged.join("\n"))
            .env("POINTER_INDEX", self.index.to_string())
            .env("POINTER_COUNT", self.files.len().to_string())
            .env("POINTER_CONTEXT", context.to_string())
            .output();

        match output {
            Ok(o) => {
                // Process stderr for directives
                let stderr = String::from_utf8_lossy(&o.stderr);
                for line in stderr.lines() {
                    if let Some(dir) = line.strip_prefix("cd:") {
                        let _ = std::env::set_current_dir(dir.trim());
                        self.index = 0;
                        self.scroll_ix = 0;
                        self.load_dir();
                    } else if let Some(name) = line.strip_prefix("select:") {
                        if let Some(pos) = self.files.iter().position(|e| e.name == name.trim()) {
                            self.index = pos;
                        }
                    } else if let Some(msg) = line.strip_prefix("status:") {
                        self.msg_info(msg.trim());
                    }
                }

                let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                if !stdout.is_empty() {
                    self.show_in_right(&stdout);
                }
            }
            Err(e) => self.msg_error(&format!("{}", e)),
        }
    }

    /// Show file properties
    pub fn file_properties(&mut self) {
        let Some(entry) = self.files.get(self.index) else { return };
        let path = &entry.path;
        let mut lines = vec![
            style::bold("File Properties"),
            String::new(),
            format!("  Name:        {}", entry.name),
            format!("  Path:        {}", path.display()),
            format!("  Size:        {}", crate::entry::format_size(entry.size)),
            format!("  Permissions: {}", crate::entry::format_mode(entry.mode)),
            format!("  Modified:    {}", crate::entry::format_time(entry.modified)),
        ];

        if entry.is_symlink {
            if let Ok(target) = std::fs::read_link(path) {
                lines.push(format!("  Link target: {}", target.display()));
            }
        }
        lines.push(format!("  Type:        {}",
            if entry.is_dir { "directory" }
            else if entry.is_symlink { "symlink" }
            else if entry.is_exec { "executable" }
            else { "file" }
        ));

        // MIME type
        let mime = std::process::Command::new("file")
            .args(["--mime-type", "-b"])
            .arg(path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        if !mime.is_empty() {
            lines.push(format!("  MIME:        {}", mime));
        }

        self.show_in_right(&lines.join("\n"));
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Copy text to clipboard using OSC 52 (works in wezterm/kitty/xterm).
/// Falls back to xclip/xsel/wl-copy via non-blocking spawn.
fn clipboard_copy(text: &str, selection: &str) {
    use std::io::Write;

    // Try OSC 52 first (works in modern terminals, no external tool needed)
    let sel_code = if selection == "primary" { "p" } else { "c" };
    let encoded = base64_encode(text.as_bytes());
    print!("\x1b]52;{};{}\x07", sel_code, encoded);
    std::io::stdout().flush().ok();

    // Also try xclip as backup (non-blocking spawn, don't wait)
    let sel_arg = if selection == "primary" { "primary" } else { "clipboard" };
    if let Ok(mut child) = Command::new("xclip")
        .args(["-selection", sel_arg])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        if let Some(ref mut stdin) = child.stdin {
            let _ = stdin.write_all(text.as_bytes());
        }
        // Don't wait - let it finish in background
        std::thread::spawn(move || { let _ = child.wait(); });
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[(n >> 18 & 63) as usize] as char);
        result.push(CHARS[(n >> 12 & 63) as usize] as char);
        if chunk.len() > 1 { result.push(CHARS[(n >> 6 & 63) as usize] as char); } else { result.push('='); }
        if chunk.len() > 2 { result.push(CHARS[(n & 63) as usize] as char); } else { result.push('='); }
    }
    result
}
