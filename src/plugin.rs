use crate::app::App;
use crust::style;
use std::fs;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Plugin {
    pub name: String,
    pub description: String,
    pub key: String,
    pub command: String,
    pub interactive: bool,
    pub enabled: bool,
}

impl App {
    /// Plugin manager (V key)
    pub fn plugin_manager(&mut self) {
        let plugins = scan_plugins();
        if plugins.is_empty() {
            self.msg_info("No plugins in ~/.pointer/plugins/");
            return;
        }
        let mut lines = vec![
            style::fg("Plugin Manager", 81),
            "=".repeat(50),
            String::new(),
        ];
        for p in &plugins {
            let status = if p.enabled {
                style::fg("ON ", 46)
            } else {
                style::fg("OFF", 196)
            };
            let key_str = if p.key.is_empty() { "   ".into() } else { format!("[{}]", p.key) };
            lines.push(format!("  {} {} {} - {}", status, key_str, p.name, p.description));
        }
        lines.push(String::new());
        lines.push("Plugins are executable scripts in ~/.pointer/plugins/".into());
        lines.push("Each has a plugin.json manifest with: name, description, key, command".into());

        self.show_in_right(&lines.join("\n"));
    }

    /// Execute a plugin by key
    pub fn run_plugin(&mut self, key: &str) -> bool {
        let plugins = scan_plugins();
        if let Some(plugin) = plugins.iter().find(|p| p.enabled && p.key == key) {
            let selected = self.files.get(self.index).map(|e| e.path.to_string_lossy().to_string()).unwrap_or_default();
            let cwd = std::env::current_dir().unwrap_or_default().to_string_lossy().to_string();
            let tagged: Vec<String> = self.tagged.iter().map(|p| p.to_string_lossy().to_string()).collect();

            let context = serde_json::json!({
                "selected": selected,
                "directory": cwd,
                "tagged": tagged,
            });

            if plugin.interactive {
                // Interactive: give the script the full terminal. It may
                // write a place to come back to in POINTER_PICK.
                let pick = pick_file();
                let _ = fs::remove_file(&pick);
                crust::Crust::cleanup();
                let _ = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&plugin.command)
                    .env("POINTER_CONTEXT", context.to_string())
                    .env("POINTER_SELECTED", &selected)
                    .env("POINTER_DIR", &cwd)
                    .env("POINTER_PICK", &pick)
                    .status();
                crust::Crust::init();
                crust::Crust::clear_screen();
                self.resize();
                if let Some((dir, name, open)) = fs::read_to_string(&pick).ok().as_deref().and_then(read_pick) {
                    let _ = fs::remove_file(&pick);
                    self.go_to(&dir, &name);
                    if open { self.enter(); }
                }
            } else {
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&plugin.command)
                    .env("POINTER_CONTEXT", context.to_string())
                    .env("POINTER_SELECTED", &selected)
                    .env("POINTER_DIR", &cwd)
                    .output();

                if let Ok(o) = output {
                    let result = String::from_utf8_lossy(&o.stdout).to_string();
                    if !result.is_empty() {
                        self.show_in_right(&result);
                    }
                }
            }
            return true;
        }
        false
    }
}

/// Where an interactive plugin may leave a place to go back to: in the
/// user's own runtime folder when there is one, which no one else can
/// write to.
fn pick_file() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(std::env::temp_dir);
    dir.join(format!("pointer-pick-{}", std::process::id()))
}

/// A pick: the folder on the first line, the name to put the cursor on
/// on the second, and `open` on the third when it should open.
fn read_pick(text: &str) -> Option<(PathBuf, String, bool)> {
    let mut lines = text.lines();
    let dir = lines.next().filter(|d| !d.is_empty())?;
    let name = lines.next().unwrap_or("").to_string();
    Some((PathBuf::from(dir), name, lines.next() == Some("open")))
}

fn scan_plugins() -> Vec<Plugin> {
    let dir = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".pointer/plugins");
    let Ok(entries) = fs::read_dir(&dir) else { return Vec::new() };

    let mut plugins = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
                    plugins.push(Plugin {
                        name: json["name"].as_str().unwrap_or("unnamed").to_string(),
                        description: json["description"].as_str().unwrap_or("").to_string(),
                        key: json["key"].as_str().unwrap_or("").to_string(),
                        command: json["command"].as_str().unwrap_or("").to_string(),
                        interactive: json["interactive"].as_bool().unwrap_or(false),
                        enabled: !path.to_string_lossy().ends_with(".off.json"),
                    });
                }
            }
        }
    }
    plugins
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_names_the_folder_the_cursor_and_whether_to_open() {
        assert_eq!(read_pick("/tmp/x\na.rs\nopen\n"), Some((PathBuf::from("/tmp/x"), "a.rs".into(), true)));
        assert_eq!(read_pick("/tmp/x\na.rs\n"), Some((PathBuf::from("/tmp/x"), "a.rs".into(), false)));
        assert_eq!(read_pick("/tmp/x"), Some((PathBuf::from("/tmp/x"), String::new(), false)));
        assert_eq!(read_pick(""), None);
    }
}
