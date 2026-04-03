use crate::app::App;
use crate::entry::{DirEntry, SortMode};
use crust::style;
use std::env;
use std::path::PathBuf;

pub struct Tab {
    pub id: usize,
    pub name: String,
    pub dir: PathBuf,
    pub index: usize,
    pub scroll_ix: usize,
    pub tagged: Vec<PathBuf>,
    pub filter_ext: String,
    pub filter_regex: String,
    pub show_hidden: bool,
    pub sort_mode: SortMode,
    pub sort_invert: bool,
}

static mut NEXT_TAB_ID: usize = 1;

fn next_id() -> usize {
    unsafe {
        let id = NEXT_TAB_ID;
        NEXT_TAB_ID += 1;
        id
    }
}

impl App {
    /// Initialize first tab from current state
    pub fn init_tabs(&mut self) {
        let cwd = env::current_dir().unwrap_or_default();
        self.tabs.push(Tab {
            id: next_id(),
            name: dir_name(&cwd),
            dir: cwd,
            index: self.index,
            scroll_ix: self.scroll_ix,
            tagged: self.tagged.clone(),
            filter_ext: self.filter_ext.clone(),
            filter_regex: self.filter_regex.clone(),
            show_hidden: self.show_hidden,
            sort_mode: self.sort_mode,
            sort_invert: self.sort_invert,
        });
        self.current_tab = 0;
    }

    /// Save current state to active tab
    fn save_tab_state(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.current_tab) {
            tab.dir = env::current_dir().unwrap_or_default();
            tab.index = self.index;
            tab.scroll_ix = self.scroll_ix;
            tab.tagged = self.tagged.clone();
            tab.filter_ext = self.filter_ext.clone();
            tab.filter_regex = self.filter_regex.clone();
            tab.show_hidden = self.show_hidden;
            tab.sort_mode = self.sort_mode;
            tab.sort_invert = self.sort_invert;
            tab.name = dir_name(&tab.dir);
        }
    }

    /// Restore state from a tab
    fn restore_tab_state(&mut self) {
        if let Some(tab) = self.tabs.get(self.current_tab) {
            let _ = env::set_current_dir(&tab.dir);
            self.index = tab.index;
            self.scroll_ix = tab.scroll_ix;
            self.tagged = tab.tagged.clone();
            self.filter_ext = tab.filter_ext.clone();
            self.filter_regex = tab.filter_regex.clone();
            self.show_hidden = tab.show_hidden;
            self.sort_mode = tab.sort_mode;
            self.sort_invert = tab.sort_invert;
            self.prev_selected = None;
            self.load_dir();
        }
    }

    /// Create new tab
    pub fn tab_new(&mut self) {
        self.save_tab_state();
        let cwd = env::current_dir().unwrap_or_default();
        self.tabs.push(Tab {
            id: next_id(),
            name: dir_name(&cwd),
            dir: cwd,
            index: 0,
            scroll_ix: 0,
            tagged: Vec::new(),
            filter_ext: String::new(),
            filter_regex: String::new(),
            show_hidden: self.show_hidden,
            sort_mode: self.sort_mode,
            sort_invert: self.sort_invert,
        });
        self.current_tab = self.tabs.len() - 1;
        self.index = 0;
        self.scroll_ix = 0;
        self.tagged.clear();
        self.filter_ext.clear();
        self.filter_regex.clear();
        self.prev_selected = None;
        self.load_dir();
    }

    /// Close current tab
    pub fn tab_close(&mut self) {
        if self.tabs.len() <= 1 { return; }
        self.tabs.remove(self.current_tab);
        if self.current_tab >= self.tabs.len() {
            self.current_tab = self.tabs.len() - 1;
        }
        self.restore_tab_state();
    }

    /// Next tab
    pub fn tab_next(&mut self) {
        if self.tabs.len() <= 1 { return; }
        self.save_tab_state();
        self.current_tab = (self.current_tab + 1) % self.tabs.len();
        self.restore_tab_state();
    }

    /// Previous tab
    pub fn tab_prev(&mut self) {
        if self.tabs.len() <= 1 { return; }
        self.save_tab_state();
        self.current_tab = if self.current_tab == 0 { self.tabs.len() - 1 } else { self.current_tab - 1 };
        self.restore_tab_state();
    }

    /// Switch to tab by number (1-indexed)
    pub fn tab_switch(&mut self, n: usize) {
        let idx = n.saturating_sub(1);
        if idx < self.tabs.len() && idx != self.current_tab {
            self.save_tab_state();
            self.current_tab = idx;
            self.restore_tab_state();
        }
    }

    /// Duplicate current tab
    pub fn tab_duplicate(&mut self) {
        self.save_tab_state();
        let cwd = env::current_dir().unwrap_or_default();
        self.tabs.push(Tab {
            id: next_id(),
            name: dir_name(&cwd),
            dir: cwd,
            index: self.index,
            scroll_ix: self.scroll_ix,
            tagged: self.tagged.clone(),
            filter_ext: self.filter_ext.clone(),
            filter_regex: self.filter_regex.clone(),
            show_hidden: self.show_hidden,
            sort_mode: self.sort_mode,
            sort_invert: self.sort_invert,
        });
        self.current_tab = self.tabs.len() - 1;
    }

    /// Rename current tab
    pub fn tab_rename(&mut self) {
        let current_name = if let Some(tab) = self.tabs.get(self.current_tab) {
            tab.name.clone()
        } else {
            return;
        };
        let new_name = self.prompt("Tab name: ", &current_name);
        if !new_name.is_empty() {
            if let Some(tab) = self.tabs.get_mut(self.current_tab) {
                tab.name = new_name;
            }
        }
    }

    /// Tab indicator for top bar
    pub fn tab_indicator(&self) -> String {
        if self.tabs.len() <= 1 { return String::new(); }
        let parts: Vec<String> = self.tabs.iter().enumerate().map(|(i, tab)| {
            if i == self.current_tab {
                style::reverse(&format!(" {} ", tab.name))
            } else {
                format!(" {} ", tab.name)
            }
        }).collect();
        format!(" | {}", parts.join("|"))
    }
}

fn dir_name(path: &PathBuf) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".into())
}
