use crate::app::App;
use crust::Crust;
use std::process::Command;

impl App {
    /// `I` — describe the selected file/directory. One-shot through the
    /// Claude Code CLI (`claude -p`): uses the user's existing Claude
    /// auth, no API key in pointer's config. Blocks while it runs (5-30s),
    /// then shows the answer in the right pane. Mirrors kastrup's `c`.
    pub fn ai_describe(&mut self) {
        let Some(entry) = self.files.get(self.index) else { return };
        let path = entry.path.clone();
        let name = entry.name.clone();

        self.msg_info("Asking claude…");
        // Force the status line to paint before the blocking call.
        use std::io::Write as _;
        let _ = std::io::stdout().flush();

        let preview_text = crate::preview::preview(
            &path, 100, false, self.show_hidden, self.sort_mode, self.sort_invert,
        );
        let plain = crust::strip_ansi(&preview_text);
        // chars().take, not byte slicing — a 4000-byte cut could land mid
        // UTF-8 char and panic on a file with multibyte content.
        let context: String = plain.chars().take(4000).collect();

        let prompt = format!(
            "Summarize the purpose of this file/directory: {} ({}). Content preview:\n{}",
            name, path.display(), context
        );

        let result = Command::new("claude")
            .arg("-p")
            .arg(&prompt)
            .stdin(std::process::Stdio::null())
            .output();
        match result {
            Ok(o) if o.status.success() => {
                let resp = String::from_utf8_lossy(&o.stdout).trim_end().to_string();
                if resp.is_empty() {
                    self.msg_warn("claude returned an empty response");
                } else {
                    self.show_in_right(&resp);
                    self.msg_info("claude response in right pane");
                }
            }
            _ => self.msg_error("claude -p failed (is the claude CLI installed?)"),
        }
    }

    /// `Ctrl+a` — hand the terminal to a full interactive Claude Code
    /// session, seeded with the current directory + selection. `/exit`
    /// returns to pointer. Mirrors kastrup's `:chat`.
    pub fn ai_chat(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_default();
        let sel_line = match self.files.get(self.index) {
            Some(e) => format!(" The selected entry is {}.", e.path.display()),
            None => String::new(),
        };
        let initial = format!(
            "I'm browsing files in pointer (a terminal file manager). The current \
             directory is {}.{} Help me with whatever I ask about these files. \
             When you're done, /exit returns me to pointer.",
            cwd.display(), sel_line
        );

        // Bracketed-paste mode interferes with claude's input handling;
        // disable it for the duration (re-enabled on return). Same handoff
        // as run_interactive: cleanup → status() → init → reload.
        use std::io::Write as _;
        print!("\x1b[?2004l");
        let _ = std::io::stdout().flush();
        Crust::cleanup();
        Crust::clear_screen();

        let _ = Command::new("claude").arg(&initial).status();

        Crust::init();
        Crust::clear_screen();
        print!("\x1b[?2004h");
        let _ = std::io::stdout().flush();
        self.load_dir(); // claude may have created / deleted files
        self.resize();   // rebuild panes + full redraw (like plugin.rs)
        self.msg_info("Back from claude");
    }
}
