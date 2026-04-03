mod ai;
mod app;
mod archive;
mod command;
mod config;
mod entry;
mod git;
mod help;
mod image;
mod marks;
mod ops;
mod plugin;
mod preview;
mod search;
mod ssh;
mod tabs;
mod undo;

use crust::{Crust, Input};

fn main() {
    config::ensure_dirs();

    // Parse --pick argument
    let mut pick_output = None;
    let mut start_dir = None;
    for arg in std::env::args().skip(1) {
        if arg.starts_with("--pick=") {
            pick_output = Some(arg[7..].to_string());
        } else if !arg.starts_with('-') {
            start_dir = Some(arg);
        }
    }
    if let Some(ref dir) = start_dir {
        let _ = std::env::set_current_dir(dir);
    }

    Crust::init();

    let mut app = app::App::new();
    app.pick_output = pick_output;
    app.render();

    loop {
        app.check_file_op();
        // 2-second idle refresh (1s during async ops) to catch filesystem changes
        let timeout = if app.file_op_running() { Some(1) } else { Some(2) };
        let key = match Input::getchr(timeout) {
            Some(k) => k,
            None => { app.render(); continue; } // Idle timeout: re-render and wait again
        };

        match key.as_str() {
            // --- BASIC ---
            "?" => { app.show_help(); }
            "v" => { app.show_version(); }
            "r" => { app.refresh(); app.render(); }
            "R" => { app.reload_config(); app.render(); }
            "C" => { app.show_config(); }
            "W" => { app.write_config(); }
            "V" => { app.plugin_manager(); }
            "q" => {
                app.save_and_quit();
                // Write pick output if in pick mode
                if let Some(ref path) = app.pick_output {
                    let lines: Vec<String> = app.tagged.iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();
                    let _ = std::fs::write(path, lines.join("\n"));
                }
                break;
            }
            "Q" => break,

            // --- LAYOUT ---
            "w" => { app.change_width(); app.render(); }
            "B" => { app.toggle_border(); app.render(); }
            "-" => { app.toggle_preview(); app.render(); }
            "_" => { app.toggle_image(); app.render(); }
            "b" => { app.toggle_bat(); app.force_render_right(); }

            // --- MOTION ---
            "j" | "DOWN" | "C-DOWN" => { app.move_down(); app.render(); }
            "k" | "UP" | "C-UP" => { app.move_up(); app.render(); }
            "h" | "LEFT" | "C-LEFT" | "BACK" => { app.go_up_dir(); app.render(); }
            "l" | "RIGHT" | "C-RIGHT" => { app.enter(); app.render(); }
            "x" => { app.open_selected_force(); app.render(); }
            "PgDOWN" => { app.page_down(); app.render(); }
            "PgUP" => { app.page_up(); app.render(); }
            "END" => { app.go_bottom(); app.render(); }
            "HOME" => { app.go_top(); app.render(); }

            // --- MARKS ---
            "m" => { app.set_mark(); }
            "M" => { app.show_marks(); }
            "'" => { app.jump_to_mark(); app.render(); }
            "~" => { app.go_home(); app.render(); }
            ">" => { app.follow_symlink(); app.render(); }

            // --- VIEW ---
            "a" => { app.toggle_hidden(); app.render(); }
            "A" => { app.toggle_long_format(); app.render(); }
            "o" => { app.cycle_sort(); app.render(); }
            "i" => { app.toggle_sort_invert(); app.render(); }
            "O" => { app.show_sort_command(); }

            // --- TAGS ---
            "t" => { app.tag_toggle(); app.render(); }
            "C-T" => { app.tag_pattern(); app.render(); }
            "T" => { app.tag_show(); }
            "u" => { app.tag_clear(); app.render(); }

            // --- UNDO ---
            "U" => { app.undo(); app.render(); }

            // --- RECENT ---
            "C-R" => { app.show_recent(); }

            // --- TABS ---
            "]" => { app.tab_new(); app.render(); }
            "[" => { app.tab_close(); app.render(); }
            "J" => { app.tab_prev(); app.render(); }
            "K" => { app.tab_next(); app.render(); }
            "}" => { app.tab_duplicate(); app.render(); }
            "{" => { app.tab_rename(); app.render(); }
            "1" => { app.tab_switch(1); app.render(); }
            "2" => { app.tab_switch(2); app.render(); }
            "3" => { app.tab_switch(3); app.render(); }
            "4" => { app.tab_switch(4); app.render(); }
            "5" => { app.tab_switch(5); app.render(); }
            "6" => { app.tab_switch(6); app.render(); }
            "7" => { app.tab_switch(7); app.render(); }
            "8" => { app.tab_switch(8); app.render(); }
            "9" => { app.tab_switch(9); app.render(); }

            // --- FILE OPERATIONS ---
            "p" => { app.copy_items(); app.render(); }
            "P" => { app.move_items(); app.render(); }
            "c" => { app.rename_item(); app.render(); }
            "E" => { app.bulk_rename(); app.render(); }
            "X" => { app.compare_files(); }
            "s" => { app.link_items(); app.render(); }
            "d" => { app.delete_items(); app.render(); }
            "D" => { app.trash_browse(); }
            "C-D" => { app.toggle_trash(); }
            "C-P" => { app.chmod(); app.render(); }
            "C-O" => { app.chown(); app.render(); }
            "=" => { app.mkdir(); app.render(); }

            // --- SEARCH & FILTER ---
            "f" => { app.filter_ext_prompt(); app.render(); }
            "F" => { app.filter_regex_prompt(); app.render(); }
            "C-F" => { app.filter_clear(); app.render(); }
            "/" => { app.search_prompt(); app.render(); }
            "\\" => { app.search_clear(); app.render(); }
            "n" => { app.search_next(); app.render(); }
            "N" => { app.search_prev(); app.render(); }
            "g" => { app.grep_files(); }
            "L" => { app.locate_files(); }
            "#" => { app.jump_locate(); app.render(); }
            "C-L" => { app.fzf_jump(); app.render(); }
            "C-N" => { app.navi_invoke(); app.render(); }

            // --- ARCHIVES ---
            "z" => { app.archive_extract(); app.render(); }
            "Z" => { app.archive_create(); app.render(); }

            // --- GIT / INFO ---
            "G" => { app.git_status(); }
            "H" => { app.hash_directory(); }
            "S" => { app.system_info(); }
            "e" => { app.file_properties(); }

            // --- AI ---
            "I" => { app.ai_describe(); }
            "C-A" => { app.ai_chat(); }

            // --- SSH ---
            "C-E" => { app.ssh_browse(); app.render(); }
            // C-; is tricky in terminals, may not be detected

            // --- RIGHT PANE ---
            "ENTER" => { app.force_render_right(); }
            "S-DOWN" => { app.right.linedown(); }
            "S-UP" => { app.right.lineup(); }
            "S-RIGHT" | "TAB" => { app.right.pagedown(); }
            "S-LEFT" | "S-TAB" => { app.right.pageup(); }

            // --- CLIPBOARD ---
            "y" => {
                let name = app.files.get(app.index).map(|e| e.path.to_string_lossy().to_string()).unwrap_or_default();
                app.yank_primary();
                app.msg_info(&format!("Yanked to primary: {}", name));
            }
            "Y" => {
                let name = app.files.get(app.index).map(|e| e.path.to_string_lossy().to_string()).unwrap_or_default();
                app.yank_clipboard();
                app.msg_info(&format!("Yanked to clipboard: {}", name));
            }
            "C-Y" => { app.yank_right_pane(); }

            // --- COMMAND MODE ---
            ":" => { app.command_mode(); app.render(); }
            ";" => { app.command_history(); }
            "+" => { app.add_interactive(); }
            "@" => { app.eval_mode(); }

            // --- RESIZE ---
            "RESIZE" => { app.resize(); app.render(); }

            _ => {}
        }
    }

    Crust::cleanup();
}
