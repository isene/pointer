use crate::app::App;
use crate::entry::DirEntry;
use crust::style;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

pub struct SshState {
    pub user: String,
    pub host: String,
    pub path: String,
    pub key: Option<String>,
}

impl App {
    pub fn is_ssh_mode(&self) -> bool {
        self.ssh_state.is_some()
    }

    /// SSH browse (C-E key)
    pub fn ssh_browse(&mut self) {
        if self.is_ssh_mode() {
            // Exit SSH mode
            self.ssh_state = None;
            self.load_dir();
            self.msg_info("Exited SSH mode");
            return;
        }

        let input = self.prompt("SSH connect: ", "");
        if input.is_empty() { return; }

        // Save to history
        if !self.state.ssh_history.contains(&input) {
            self.state.ssh_history.insert(0, input.clone());
            if self.state.ssh_history.len() > 20 {
                self.state.ssh_history.truncate(20);
            }
        }

        // Parse: [user@]host[:path] or [-i key] user@host[:path]
        let (key, rest) = if input.starts_with("-i ") {
            let parts: Vec<&str> = input.splitn(3, ' ').collect();
            if parts.len() >= 3 {
                (Some(parts[1].to_string()), parts[2].to_string())
            } else {
                (None, input.clone())
            }
        } else {
            (None, input.clone())
        };

        let (user_host, path) = if let Some((uh, p)) = rest.split_once(':') {
            (uh.to_string(), p.to_string())
        } else {
            (rest.clone(), "~".to_string())
        };

        let (user, host) = if let Some((u, h)) = user_host.split_once('@') {
            (u.to_string(), h.to_string())
        } else {
            (std::env::var("USER").unwrap_or_default(), user_host)
        };

        // Test connection
        self.msg_info(&format!("Connecting to {}@{}...", user, host));
        let mut cmd = Command::new("ssh");
        if let Some(ref k) = key { cmd.args(["-i", k]); }
        cmd.args(["-o", "ConnectTimeout=5", &format!("{}@{}", user, host), "echo ok"]);
        let test = cmd.output();

        match test {
            Ok(o) if o.status.success() => {
                self.ssh_state = Some(SshState { user, host, path, key });
                self.load_ssh_dir();
                self.msg_success("Connected via SSH");
            }
            _ => self.msg_error("SSH connection failed"),
        }
    }

    /// Load remote directory listing
    pub fn load_ssh_dir(&mut self) {
        let Some(ref state) = self.ssh_state else { return };
        let mut cmd = Command::new("ssh");
        if let Some(ref k) = state.key { cmd.args(["-i", k]); }
        cmd.arg(format!("{}@{}", state.user, state.host));
        cmd.arg(format!("ls -1a --group-directories-first -p {}", state.path));

        let output = cmd.output();
        let Ok(o) = output else { self.msg_error("SSH ls failed"); return };
        let text = String::from_utf8_lossy(&o.stdout);

        self.files.clear();
        for line in text.lines() {
            let name = line.trim().to_string();
            if name.is_empty() || name == "." { continue; }
            let is_dir = name.ends_with('/');
            let display_name = name.trim_end_matches('/').to_string();
            self.files.push(DirEntry {
                name: display_name,
                path: PathBuf::from(&name),
                is_dir,
                is_symlink: name.contains('@'),
                is_exec: false,
                size: 0,
                modified: SystemTime::UNIX_EPOCH,
                mode: 0,
                uid: 0,
                gid: 0,
                color_code: if is_dir { "\x1b[38;5;12m".into() } else { String::new() },
                tagged: false,
                search_hit: false,
            });
        }
        self.index = 0;
        self.scroll_ix = 0;
    }

    /// SSH history (C-; key)
    pub fn ssh_history(&mut self) {
        if self.state.ssh_history.is_empty() {
            self.msg_info("No SSH history");
            return;
        }
        let mut lines = vec![
            style::fg("SSH History", 81),
            "=".repeat(50),
            String::new(),
        ];
        for (i, entry) in self.state.ssh_history.iter().enumerate() {
            lines.push(format!("  {} {}", style::fg(&format!("{:2}", i + 1), 220), entry));
        }
        self.right.set_text(&lines.join("\n"));
        self.right.ix = 0;
        self.right.refresh();
        self.prev_selected = Some(PathBuf::new());
    }
}
