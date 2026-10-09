//! "Open with": pick the program for a file, from the ones that say
//! they open its type. The desktop files are read when the key is
//! pressed and at no other time.

use crate::app::App;
use crust::{style, Input};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// The desktop file's name, `org.gnome.TextEditor.desktop`.
    pub desktop: String,
    pub name: String,
    pub exec: String,
    /// It wants the terminal for itself.
    pub terminal: bool,
}

/// The folders desktop files are in, a user's own first.
pub fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let own = std::env::var("XDG_DATA_HOME").ok().filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{home}/.local/share"));
    let shared = std::env::var("XDG_DATA_DIRS").ok().filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    std::iter::once(own.as_str()).chain(shared.split(':'))
        .filter(|d| !d.is_empty())
        .map(|d| Path::new(d).join("applications"))
        .collect()
}

/// One desktop file: the program, and the types it says it opens.
/// `None` for a file that is hidden or is no program.
fn parse(desktop: &str, text: &str) -> Option<(Program, Vec<String>)> {
    let (mut name, mut exec, mut types) = (String::new(), String::new(), Vec::new());
    let (mut terminal, mut in_entry) = (false, false);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry { continue; }
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "Name" => name = value.to_string(),
            "Exec" => exec = value.to_string(),
            "Terminal" => terminal = value == "true",
            "MimeType" => types = value.split(';').filter(|t| !t.is_empty()).map(str::to_string).collect(),
            "Hidden" if value == "true" => return None,
            "Type" if value != "Application" => return None,
            _ => {}
        }
    }
    if exec.is_empty() { return None; }
    if name.is_empty() { name = desktop.trim_end_matches(".desktop").to_string(); }
    Some((Program { desktop: desktop.to_string(), name, exec, terminal }, types))
}

/// The programs that list one of `types`, by name. A desktop file in an
/// earlier folder hides one of the same name in a later folder.
pub fn programs_for(types: &[String], dirs: &[PathBuf]) -> Vec<Program> {
    let mut seen = std::collections::HashSet::new();
    let mut found = Vec::new();
    for dir in dirs {
        let Ok(read) = std::fs::read_dir(dir) else { continue };
        for entry in read.flatten() {
            let file = entry.file_name().to_string_lossy().to_string();
            if !file.ends_with(".desktop") || !seen.insert(file.clone()) { continue; }
            let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
            let Some((program, opens)) = parse(&file, &text) else { continue };
            if opens.iter().any(|t| types.contains(t)) { found.push(program); }
        }
    }
    found.sort_by_key(|p| p.name.to_lowercase());
    found
}

/// A path as one word for the shell.
fn quoted(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

/// The command for one file, from a desktop file's Exec line: `%f`,
/// `%F`, `%u` and `%U` become the file, the other codes go. A line that
/// names no place for the file gets it at the end.
pub fn command(exec: &str, path: &Path) -> String {
    let file = quoted(path);
    let (mut out, mut placed) = (String::new(), false);
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('f' | 'F' | 'u' | 'U') => { out.push_str(&file); placed = true; }
            Some('%') => out.push('%'),
            _ => {}
        }
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if placed { out } else { format!("{out} {file}") }
}

/// The types to look for: the file's own, and plain text for every kind
/// of text, so a text editor is offered for source code.
fn types_of(kind: &str) -> Vec<String> {
    let mut types = vec![kind.to_string()];
    if kind.starts_with("text/") && kind != "text/plain" { types.push("text/plain".into()); }
    types
}

fn xdg_mime(args: &[&str], path: Option<&Path>) -> String {
    let mut cmd = std::process::Command::new("xdg-mime");
    cmd.args(args);
    if let Some(path) = path { cmd.arg(path); }
    cmd.output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

impl App {
    /// Ctrl-X: list the programs for the file under the cursor and open
    /// it with the one picked.
    pub fn open_with(&mut self) {
        let Some(entry) = self.files.get(self.index) else { return };
        if entry.is_dir {
            self.msg_info("Open with is for a file");
            return;
        }
        let (path, file_name) = (entry.path.clone(), entry.name.clone());
        let kind = xdg_mime(&["query", "filetype"], Some(&path));
        let default = if kind.is_empty() { String::new() } else { xdg_mime(&["query", "default", &kind], None) };
        let mut programs = programs_for(&types_of(&kind), &data_dirs());
        // The default first, and no more than the nine a digit can pick.
        programs.sort_by_key(|p| p.desktop != default);
        programs.truncate(9);

        let mut lines = vec![
            style::fg(&format!("Open {file_name} with"), 81),
            "=".repeat(50),
            style::fg(if kind.is_empty() { "type not known" } else { &kind }, 240),
            String::new(),
        ];
        for (i, p) in programs.iter().enumerate() {
            let note = if p.desktop == default { "  default" } else { "" };
            lines.push(format!("  {} {}{}",
                style::fg(&format!("{}", i + 1), 220), style::fg(&p.name, 156), style::fg(note, 240)));
        }
        if programs.is_empty() { lines.push(style::fg("  No program lists this type", 245)); }
        lines.push(String::new());
        lines.push(format!("  {} another program: type its command", style::fg("o", 220)));
        if !programs.is_empty() && !kind.is_empty() {
            lines.push(format!("  {} then a number: make it the default for this type", style::fg("d", 220)));
        }
        self.show_in_right(&lines.join("\n"));

        let pick = |key: &str| key.parse::<usize>().ok().filter(|n| (1..=programs.len()).contains(n)).map(|n| n - 1);
        let Some(key) = Input::getchr(None) else { return };
        // `d` reads its number while the list is still on screen.
        let new_default = if key == "d" && !kind.is_empty() {
            self.msg_info("Default: press the number of the program");
            Input::getchr(None).and_then(|k| pick(&k))
        } else {
            None
        };
        // The list is done. The preview comes back first, so what is
        // said below stays on the status line.
        self.show_preview_again();
        match key.as_str() {
            "o" => {
                let typed = self.prompt("Open with: ", "");
                let typed = typed.trim();
                if let Some(prog) = typed.split_whitespace().next() {
                    let own_terminal = self.config.interactive.iter().any(|p| p == prog);
                    self.launch(prog, &format!("{typed} {}", quoted(&path)), own_terminal);
                }
            }
            "d" => {
                if let Some(p) = new_default.map(|i| &programs[i]) {
                    xdg_mime(&["default", &p.desktop, &kind], None);
                    if xdg_mime(&["query", "default", &kind], None) == p.desktop {
                        self.msg_success(&format!("{} now opens {}", p.name, kind));
                    } else {
                        self.msg_error("The default did not change");
                    }
                }
            }
            other => {
                if let Some(i) = pick(other) {
                    let p = programs[i].clone();
                    let base = p.exec.split_whitespace().next().unwrap_or("");
                    let base = Path::new(base).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let own_terminal = p.terminal || self.config.interactive.iter().any(|i| *i == base);
                    self.launch(&p.name, &command(&p.exec, &path), own_terminal);
                }
            }
        }
    }

    fn show_preview_again(&mut self) {
        self.right_pane_locked = false;
        self.prev_selected = None;
        self.render();
    }

    /// Run a command for a file: in this terminal when the program needs
    /// one, else on its own, so it lives on when pointer quits.
    fn launch(&mut self, name: &str, cmd: &str, own_terminal: bool) {
        if own_terminal {
            self.run_interactive(cmd);
            self.show_preview_again();
            return;
        }
        use std::os::unix::process::CommandExt;
        let mut child = std::process::Command::new("sh");
        child.arg("-c").arg(cmd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        unsafe {
            child.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        match child.spawn() {
            Ok(_) => self.msg_info(&format!("Opened with {name}")),
            Err(e) => self.msg_error(&format!("Could not start {name}: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_file_gives_the_program_and_the_types_it_opens() {
        let text = "[Desktop Entry]\nType=Application\nName=Text Editor\nName[nb]=Tekst\n\
                    Exec=gnome-text-editor %U\nMimeType=text/plain;text/markdown;\n\
                    [Desktop Action new]\nName=New Window\nExec=gnome-text-editor --new-window\n";
        let (p, types) = parse("editor.desktop", text).unwrap();
        assert_eq!((p.name.as_str(), p.exec.as_str(), p.terminal), ("Text Editor", "gnome-text-editor %U", false));
        assert_eq!(types, ["text/plain", "text/markdown"]);
        assert!(parse("x.desktop", "[Desktop Entry]\nName=X\nExec=x\nHidden=true\n").is_none());
        assert!(parse("x.desktop", "[Desktop Entry]\nType=Link\nName=X\nExec=x\n").is_none());
        assert!(parse("x.desktop", "[Desktop Entry]\nName=No command\n").is_none());
    }

    #[test]
    fn the_file_goes_where_the_exec_line_asks_for_it() {
        let file = Path::new("/tmp/it's a note.md");
        let q = r"'/tmp/it'\''s a note.md'";
        assert_eq!(command("gimp %U", file), format!("gimp {q}"));
        assert_eq!(command("app --open=%f --class %c %i", file), format!("app --open={q} --class"));
        assert_eq!(command("viewer", file), format!("viewer {q}"));
        assert_eq!(command("pct 100%% %F", file), format!("pct 100% {q}"));
    }

    #[test]
    fn a_users_own_desktop_file_hides_the_systems_and_text_falls_back_to_plain() {
        let root = std::env::temp_dir().join(format!("pointer-openwith-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (own, system) = (root.join("own"), root.join("system"));
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(own.join("edit.desktop"), "[Desktop Entry]\nName=Mine\nExec=mine %f\nMimeType=text/plain;\nTerminal=true\n").unwrap();
        std::fs::write(system.join("edit.desktop"), "[Desktop Entry]\nName=Theirs\nExec=theirs %f\nMimeType=text/plain;\n").unwrap();
        std::fs::write(system.join("paint.desktop"), "[Desktop Entry]\nName=Paint\nExec=paint %f\nMimeType=image/png;\n").unwrap();
        std::fs::write(system.join("code.desktop"), "[Desktop Entry]\nName=Code\nExec=code %f\nMimeType=text/x-rust;\n").unwrap();

        let found = programs_for(&types_of("text/x-rust"), &[own.clone(), system.clone()]);
        let names: Vec<&str> = found.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Code", "Mine"]);
        assert!(found[1].terminal);
        assert_eq!(programs_for(&types_of("image/png"), &[own, system]).len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
