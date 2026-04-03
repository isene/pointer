use crate::app::App;
use crust::style;
use std::process::Command;

impl App {
    /// Show git status in right pane
    pub fn git_status(&mut self) {
        // Check if in a git repo
        let in_repo = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !in_repo {
            self.show_in_right(" Not a git repository");
            return;
        }

        let mut lines = vec![style::bold("Git Status"), String::new()];

        // Branch
        if let Some(branch) = run_git(&["branch", "--show-current"]) {
            lines.push(format!("  Branch: {}", style::fg(branch.trim(), 81)));
        }

        // Status
        if let Some(status) = run_git(&["status", "--porcelain"]) {
            if status.trim().is_empty() {
                lines.push(format!("  {}", style::fg("Working tree clean", 46)));
            } else {
                lines.push(String::new());
                lines.push(style::fg("Changes:", 220));
                for line in status.lines() {
                    let (indicator, rest) = line.split_at(3.min(line.len()));
                    let color = match indicator.chars().next() {
                        Some('M') => 220,  // Modified
                        Some('A') => 46,   // Added
                        Some('D') => 196,  // Deleted
                        Some('?') => 245,  // Untracked
                        Some('R') => 81,   // Renamed
                        _ => 255,
                    };
                    lines.push(format!("  {}{}", style::fg(indicator, color), rest));
                }
            }
        }

        // Recent commits
        if let Some(log) = run_git(&["log", "--oneline", "-n", "5"]) {
            lines.push(String::new());
            lines.push(style::fg("Recent commits:", 220));
            for line in log.lines() {
                if let Some((hash, msg)) = line.split_once(' ') {
                    lines.push(format!("  {} {}", style::fg(hash, 81), msg));
                }
            }
        }

        self.show_in_right(&lines.join("\n"));
    }

    /// Show system info (RTFM style with visual bars)
    pub fn system_info(&mut self) {
        let now = crate::entry::format_time(std::time::SystemTime::now());
        let mut l = vec![
            format!("{} {} {}", style::bold("SYSTEM INFORMATION"), style::fg("-", 245), style::fg(&now, 249)),
            style::fg(&"=".repeat(50), 245),
            String::new(),
        ];

        // --- System Overview ---
        section(&mut l, "System Overview");
        if let Ok(hostname) = std::fs::read_to_string("/etc/hostname") {
            kv(&mut l, "Hostname", hostname.trim());
        }
        if let Some(up) = shell_cmd("uptime -p 2>/dev/null") {
            kv(&mut l, "Uptime", up.trim().trim_start_matches("up "));
        }
        if let Some(boot) = shell_cmd("uptime -s 2>/dev/null") {
            kv(&mut l, "Boot time", boot.trim());
        }
        if let Some(os) = shell_cmd("awk -F '\"' '/PRETTY/ {print $2}' /etc/os-release 2>/dev/null") {
            kv(&mut l, "OS", os.trim());
        }
        if let Some(k) = run_cmd("uname", &["-r"]) { kv(&mut l, "Kernel", k.trim()); }
        if let Some(a) = run_cmd("uname", &["-m"]) { kv(&mut l, "Arch", a.trim()); }
        l.push(String::new());

        // --- Hardware ---
        section(&mut l, "Hardware");
        if let Some(cpu) = shell_cmd("lscpu 2>/dev/null | grep 'Model name' | sed 's/.*: *//'") {
            let c = cpu.trim();
            kv(&mut l, "CPU", if c.len() > 45 { &c[..45] } else { c });
        }
        if let Some(cores) = shell_cmd("nproc 2>/dev/null") {
            kv(&mut l, "Cores", &format!("{} cores", cores.trim()));
        }
        if let Some(freq) = shell_cmd("grep 'cpu MHz' /proc/cpuinfo 2>/dev/null | head -1 | awk '{printf \"%.0f\", $4}'") {
            let f = freq.trim();
            if !f.is_empty() && f != "0" { kv(&mut l, "Frequency", &format!("{} MHz", f)); }
        }
        if let Some(load) = shell_cmd("cat /proc/loadavg 2>/dev/null | cut -d' ' -f1-3") {
            kv(&mut l, "Load avg", load.trim());
        }
        if let Ok(temp_str) = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp") {
            let temp = temp_str.trim().parse::<f64>().unwrap_or(0.0) / 1000.0;
            if temp > 0.0 {
                let color: u8 = if temp > 80.0 { 196 } else if temp > 60.0 { 220 } else { 156 };
                kvc(&mut l, "CPU Temp", &format!("{:.1}\u{00B0}C", temp), color);
            }
        }
        l.push(String::new());

        // --- Memory ---
        section(&mut l, "Memory");
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            let total = extract_meminfo(&meminfo, "MemTotal:");
            let avail = extract_meminfo(&meminfo, "MemAvailable:");
            if total > 0 {
                let used = total.saturating_sub(avail);
                let pct = (used as f64 / total as f64 * 100.0) as u32;
                let color: u8 = if pct > 90 { 196 } else if pct > 70 { 220 } else { 156 };
                l.push(format!("  {} {:>3}%",
                    style::fg(&bar(pct, 40), color),
                    style::fg(&pct.to_string(), color)));
                kv(&mut l, "Total", &format!("{:.1} GB", total as f64 / 1048576.0));
                kvc(&mut l, "Used", &format!("{:.1} GB", used as f64 / 1048576.0), color);
                kv(&mut l, "Available", &format!("{:.1} GB", avail as f64 / 1048576.0));
            }
        }
        if let Some(swap) = shell_cmd("free -b 2>/dev/null | grep Swap:") {
            let parts: Vec<&str> = swap.split_whitespace().collect();
            if parts.len() >= 3 {
                let total: f64 = parts[1].parse().unwrap_or(0.0);
                let used: f64 = parts[2].parse().unwrap_or(0.0);
                if total > 0.0 {
                    kv(&mut l, "Swap", &format!("{:.1}/{:.1} GB", used / 1073741824.0, total / 1073741824.0));
                }
            }
        }
        l.push(String::new());

        // --- Storage ---
        section(&mut l, "Storage");
        if let Some(df) = shell_cmd("df -BG 2>/dev/null | grep -E '^/dev/' | grep -vE '/snap/|/tmp | /run|/dev/loop'") {
            for line in df.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 6 { continue; }
                let size: u32 = parts[1].trim_end_matches('G').parse().unwrap_or(0);
                let used: u32 = parts[2].trim_end_matches('G').parse().unwrap_or(0);
                let pct: u32 = parts[4].trim_end_matches('%').parse().unwrap_or(0);
                let mount = parts[5];
                if size < 1 { continue; }
                let color: u8 = if pct > 90 { 196 } else if pct > 80 { 220 } else { 156 };
                let mount_short = if mount.len() > 18 { &mount[mount.len()-15..] } else { mount };
                let sizes = format!("{}G/{}G", used, size);
                l.push(style::fg(&format!("  {:<18} {:>9} {} {:>3}%",
                    mount_short, sizes, bar(pct, 10), pct), color));
            }
        }
        l.push(String::new());

        // --- Network ---
        section(&mut l, "Network");
        if let Some(ifaces) = shell_cmd("ip -o link show 2>/dev/null | grep -E 'state UP|UNKNOWN' | awk '{print $2}' | sed 's/://'") {
            for iface in ifaces.lines() {
                let iface = iface.trim();
                if iface.is_empty() || iface == "lo" { continue; }
                let ip = shell_cmd(&format!("ip -4 addr show {} 2>/dev/null | grep -oP '(?<=inet\\s)\\d+(\\.\\d+){{3}}'", iface));
                if let Some(ip) = ip {
                    let ip = ip.trim();
                    if ip.is_empty() { continue; }
                    kv(&mut l, iface, ip);
                    let rx = std::fs::read_to_string(format!("/sys/class/net/{}/statistics/rx_bytes", iface))
                        .ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
                    let tx = std::fs::read_to_string(format!("/sys/class/net/{}/statistics/tx_bytes", iface))
                        .ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
                    l.push(format!("    {}{}", style::fg(&format!("{:<9}", "Traffic:"), 249),
                        style::fg(&format!("\u{2193}{:.0}MB \u{2191}{:.0}MB", rx as f64/1048576.0, tx as f64/1048576.0), 156)));
                }
            }
        }
        l.push(String::new());

        // --- Environment ---
        section(&mut l, "Environment");
        let shell = std::env::var("SHELL").unwrap_or_default();
        let shell_name = shell.rsplit('/').next().unwrap_or("unknown");
        kv(&mut l, "Shell", shell_name);
        kv(&mut l, "Terminal", &std::env::var("TERM").unwrap_or_else(|_| "unknown".into()));
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| std::env::var("DESKTOP_SESSION"))
            .unwrap_or_else(|_| "none".into());
        kv(&mut l, "Desktop", &desktop);
        if let Some(pkgs) = shell_cmd("dpkg-query -l 2>/dev/null | grep -c '^ii' || pacman -Q 2>/dev/null | wc -l || rpm -qa 2>/dev/null | wc -l") {
            let p = pkgs.trim();
            if !p.is_empty() && p != "0" { kv(&mut l, "Packages", p); }
        }
        l.push(String::new());
        l.push(format!("  pointer v{}", env!("CARGO_PKG_VERSION")));

        self.show_in_right(&l.join("\n"));
    }

    /// Toggle trash mode
    pub fn toggle_trash(&mut self) {
        self.config.trash = !self.config.trash;
        self.msg_info(&format!("Trash: {}",
            if self.config.trash { "on" } else { "off" }));
    }

    /// Show recent files (C-R key)
    pub fn show_recent(&mut self) {
        let mut lines = vec![
            style::fg("Recent Files & Directories", 81),
            "=".repeat(50),
            String::new(),
        ];
        if !self.state.recent_files.is_empty() {
            lines.push(style::fg("Recent files:", 46));
            for (i, f) in self.state.recent_files.iter().take(15).enumerate() {
                lines.push(format!("  {} {}", style::fg(&format!("{:2}", i + 1), 220), f));
            }
        } else {
            lines.push(style::fg("No recent files", 245));
        }
        lines.push(String::new());
        if !self.state.recent_dirs.is_empty() {
            lines.push(style::fg("Recent directories:", 46));
            for (i, d) in self.state.recent_dirs.iter().take(10).enumerate() {
                let exists = std::path::Path::new(d).is_dir();
                let mark = if exists { "\u{2713}" } else { "\u{2717}" };
                lines.push(format!("  {} {} {}", style::fg(&format!("{:2}", i + 16), 220), mark, d));
            }
        }
        self.show_in_right(&lines.join("\n"));
    }

    /// Hash directory (H key)
    pub fn hash_directory(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_default();
        let cwd_str = cwd.to_string_lossy().to_string();
        self.msg_info("Hashing directory...");

        let output = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "cd {:?} && (find . -type f | sort | xargs sha1sum 2>/dev/null; find . -type f -o -type d | sort | xargs stat --format='%n %a %U' 2>/dev/null) | sha1sum | cut -c -40",
                cwd
            ))
            .output();

        let hash = match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => { self.msg_error("Hash failed"); return; }
        };

        let now = crate::entry::format_time(std::time::SystemTime::now());

        if let Some((old_time, old_hash)) = self.state.dir_hashes.get(&cwd_str) {
            if *old_hash == hash {
                self.msg_success(&format!("Directory unchanged since {}", old_time));
            } else {
                self.msg_warn(&format!("Directory CHANGED since {} (old: {}..., new: {}...)",
                    old_time, &old_hash[..8.min(old_hash.len())], &hash[..8.min(hash.len())]));
            }
        } else {
            self.msg_success(&format!("Hash recorded: {}...", &hash[..8.min(hash.len())]));
        }
        self.state.dir_hashes.insert(cwd_str, (now, hash));
    }

    /// Browse trash
    pub fn trash_browse(&mut self) {
        let trash_dir = crate::config::trash_dir();
        let entries = std::fs::read_dir(&trash_dir).ok();
        let Some(entries) = entries else {
            self.show_in_right(" Trash is empty");
            return;
        };

        let items: Vec<String> = entries
            .flatten()
            .map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let size = e.metadata().ok().map(|m| m.len()).unwrap_or(0);
                format!("  {} ({})", name, crate::entry::format_size(size))
            })
            .collect();

        if items.is_empty() {
            self.show_in_right(" Trash is empty");
        } else {
            let mut lines = vec![
                style::bold(&format!("Trash ({} items)", items.len())),
                String::new(),
            ];
            lines.extend(items);
            self.show_in_right(&lines.join("\n"));
        }
    }
}

fn run_git(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

fn extract_meminfo(info: &str, key: &str) -> u64 {
    info.lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn shell_cmd(cmd: &str) -> Option<String> {
    Command::new("sh").arg("-c").arg(cmd).output().ok()
        .filter(|o| o.status.success() && !o.stdout.is_empty())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

/// Section header
fn section(lines: &mut Vec<String>, title: &str) {
    lines.push(style::fg(&style::bold(title), 220));
    lines.push(style::fg(&"\u{2500}".repeat(20), 245));
}

/// Format a key-value pair: key in gray, value in green, aligned at column 11
fn kv(lines: &mut Vec<String>, key: &str, val: &str) {
    let label = format!("{}:", key);
    lines.push(format!("  {}{}", style::fg(&format!("{:<11}", label), 249), style::fg(val, 156)));
}

/// Format a key-value pair with custom value color
fn kvc(lines: &mut Vec<String>, key: &str, val: &str, color: u8) {
    let label = format!("{}:", key);
    lines.push(format!("  {}{}", style::fg(&format!("{:<11}", label), 249), style::fg(val, color)));
}

/// Create a visual bar: filled/empty blocks
fn bar(percent: u32, width: u32) -> String {
    let filled = (percent * width / 100).min(width);
    let empty = width.saturating_sub(filled);
    format!("{}{}", "\u{2588}".repeat(filled as usize), "\u{2591}".repeat(empty as usize))
}
