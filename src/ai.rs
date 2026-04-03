use crate::app::App;
use crust::style;
use std::process::Command;

impl App {
    /// OpenAI file description (I key)
    pub fn ai_describe(&mut self) {
        if self.config.ai_key.is_empty() {
            self.msg_warn("Set ai_key in ~/.pointer/conf.json");
            return;
        }
        let Some(entry) = self.files.get(self.index) else { return };
        let path = entry.path.clone();
        let name = entry.name.clone();

        self.msg_info("Asking AI...");

        // Build prompt
        let preview_text = crate::preview::preview(&path, 100, false, self.show_hidden);
        let plain = crust::strip_ansi(&preview_text);
        let context = if plain.len() > 2000 { &plain[..2000] } else { &plain };

        let prompt = format!(
            "Summarize the purpose of this file/directory: {}. Content preview:\n{}",
            name, context
        );

        let body = serde_json::json!({
            "model": self.config.ai_model,
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": 600
        });

        let output = Command::new("curl")
            .args(["-s", "-X", "POST", "https://api.openai.com/v1/chat/completions",
                   "-H", "Content-Type: application/json",
                   "-H", &format!("Authorization: Bearer {}", self.config.ai_key),
                   "-d", &body.to_string()])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let response = String::from_utf8_lossy(&o.stdout);
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response) {
                    let content = json["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or("No response");
                    self.show_in_right(content);
                } else {
                    self.msg_error("Failed to parse AI response");
                }
            }
            _ => self.msg_error("AI request failed"),
        }
    }

    /// AI chat mode (C-A key)
    pub fn ai_chat(&mut self) {
        if self.config.ai_key.is_empty() {
            self.msg_warn("Set ai_key in ~/.pointer/conf.json");
            return;
        }

        let mut history = Vec::new();
        loop {
            let input = self.prompt("AI> ", "");
            if input.is_empty() { break; }

            history.push(serde_json::json!({"role": "user", "content": input}));

            let body = serde_json::json!({
                "model": self.config.ai_model,
                "messages": history,
                "max_tokens": 600
            });

            let output = Command::new("curl")
                .args(["-s", "-X", "POST", "https://api.openai.com/v1/chat/completions",
                       "-H", "Content-Type: application/json",
                       "-H", &format!("Authorization: Bearer {}", self.config.ai_key),
                       "-d", &body.to_string()])
                .output();

            match output {
                Ok(o) if o.status.success() => {
                    let response = String::from_utf8_lossy(&o.stdout);
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response) {
                        let content = json["choices"][0]["message"]["content"]
                            .as_str()
                            .unwrap_or("No response")
                            .to_string();
                        history.push(serde_json::json!({"role": "assistant", "content": content}));
                        // Show full conversation in right pane
                        let display: Vec<String> = history.iter().map(|m| {
                            let role = m["role"].as_str().unwrap_or("");
                            let text = m["content"].as_str().unwrap_or("");
                            if role == "user" {
                                format!("{}: {}", style::fg("You", 81), text)
                            } else {
                                format!("{}: {}", style::fg("AI", 46), text)
                            }
                        }).collect();
                        self.right.set_text(&display.join("\n\n"));
                        self.right.ix = 0;
                        self.right.refresh();
                        self.prev_selected = Some(std::path::PathBuf::new());
                    }
                }
                _ => { self.msg_error("AI request failed"); break; }
            }
        }
    }
}
