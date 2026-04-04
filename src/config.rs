use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn pointer_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".pointer")
}

fn config_path() -> PathBuf { pointer_dir().join("conf.json") }
fn state_path() -> PathBuf { pointer_dir().join("state.json") }
pub fn trash_dir() -> PathBuf { pointer_dir().join("trash") }
pub fn lastdir_path() -> PathBuf { pointer_dir().join("lastdir") }

pub fn ensure_dirs() {
    let dir = pointer_dir();
    let _ = fs::create_dir_all(&dir);
    let _ = fs::create_dir_all(trash_dir());
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_width")]
    pub width: u8,
    #[serde(default = "default_border")]
    pub border: u8,
    #[serde(default = "default_true")]
    pub preview: bool,
    #[serde(default)]
    pub trash: bool,
    #[serde(default = "default_true")]
    pub bat: bool,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default)]
    pub long_format: bool,
    #[serde(default = "default_sort")]
    pub sort_mode: String,
    #[serde(default)]
    pub sort_invert: bool,
    #[serde(default = "default_interactive")]
    pub interactive: Vec<String>,
    #[serde(default = "default_top_fg")]
    pub c_top_fg: u16,
    #[serde(default = "default_top_bg")]
    pub c_top_bg: u16,
    #[serde(default = "default_status_fg")]
    pub c_status_fg: u16,
    #[serde(default = "default_status_bg")]
    pub c_status_bg: u16,
    #[serde(default)]
    pub preview_handlers: HashMap<String, String>,
    #[serde(default = "default_topmatch")]
    pub topmatch: Vec<(String, u16)>,
    #[serde(default)]
    pub ai_key: String,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
}

fn default_width() -> u8 { 4 }
fn default_border() -> u8 { 2 }
fn default_true() -> bool { true }
fn default_sort() -> String { "name".into() }
fn default_interactive() -> Vec<String> {
    vec!["fzf", "navi", "top", "htop", "less", "vi", "vim", "ncdu", "sh", "zsh", "bash", "fish", "mplayer", "nano"]
        .into_iter().map(String::from).collect()
}
fn default_topmatch() -> Vec<(String, u16)> { vec![("".into(), 249)] }
fn default_ai_model() -> String { "gpt-4o-mini".into() }
fn default_top_fg() -> u16 { 0 }
fn default_top_bg() -> u16 { 249 }
fn default_status_fg() -> u16 { 252 }
fn default_status_bg() -> u16 { 236 }

impl Default for Config {
    fn default() -> Self {
        Config {
            width: default_width(),
            border: default_border(),
            preview: true,
            trash: true,
            bat: true,
            show_hidden: false,
            long_format: false,
            sort_mode: default_sort(),
            sort_invert: false,
            interactive: default_interactive(),
            c_top_fg: 0,
            c_top_bg: 249,
            c_status_fg: 252,
            c_status_bg: 236,
            preview_handlers: HashMap::new(),
            topmatch: default_topmatch(),
            ai_key: String::new(),
            ai_model: default_ai_model(),
        }
    }
}

pub fn config_path_str() -> String {
    config_path().to_string_lossy().to_string()
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            let cfg = Config::default();
            cfg.save();
            cfg
        }
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(config_path(), json);
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct State {
    #[serde(default)]
    pub marks: HashMap<String, String>,
    #[serde(default)]
    pub history: Vec<String>,
    #[serde(default)]
    pub dir_index: HashMap<String, usize>,
    #[serde(default)]
    pub recent_files: Vec<String>,
    #[serde(default)]
    pub recent_dirs: Vec<String>,
    #[serde(default)]
    pub dir_hashes: HashMap<String, (String, String)>,
    #[serde(default)]
    pub ssh_history: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            marks: HashMap::new(),
            history: Vec::new(),
            dir_index: HashMap::new(),
            recent_files: Vec::new(),
            recent_dirs: Vec::new(),
            dir_hashes: HashMap::new(),
            ssh_history: Vec::new(),
        }
    }
}

impl State {
    pub fn load() -> Self {
        let path = state_path();
        if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            State::default()
        }
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(state_path(), json);
        }
    }
}
