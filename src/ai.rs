use crate::app::App;
use crust::style;
use std::process::Command;

/// Dispatch a chat completion. Routes to Anthropic when model starts with
/// `claude-`, otherwise OpenAI. Returns the assistant message text.
fn ai_request(model: &str, key: &str, messages: &serde_json::Value) -> Option<String> {
    if model.starts_with("claude-") {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": 1024,
            "messages": messages,
        });
        let output = Command::new("curl")
            .args(["-s", "-X", "POST", "https://api.anthropic.com/v1/messages",
                   "-H", "content-type: application/json",
                   "-H", "anthropic-version: 2023-06-01",
                   "-H", &format!("x-api-key: {}", key),
                   "-d", &body.to_string()])
            .output().ok()?;
        if !output.status.success() { return None; }
        let response = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value = serde_json::from_str(&response).ok()?;
        json["content"][0]["text"].as_str().map(String::from)
    } else {
        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": 600,
        });
        let output = Command::new("curl")
            .args(["-s", "-X", "POST", "https://api.openai.com/v1/chat/completions",
                   "-H", "Content-Type: application/json",
                   "-H", &format!("Authorization: Bearer {}", key),
                   "-d", &body.to_string()])
            .output().ok()?;
        if !output.status.success() { return None; }
        let response = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value = serde_json::from_str(&response).ok()?;
        json["choices"][0]["message"]["content"].as_str().map(String::from)
    }
}

impl App {
    /// AI file description (I key). Routes to Anthropic or OpenAI based on
    /// the configured model name (claude-* → Anthropic).
    pub fn ai_describe(&mut self) {
        if self.config.ai_key.is_empty() {
            self.msg_warn("Set ai_key in ~/.pointer/conf.json");
            return;
        }
        let Some(entry) = self.files.get(self.index) else { return };
        let path = entry.path.clone();
        let name = entry.name.clone();

        self.msg_info("Asking AI...");

        let preview_text = crate::preview::preview(&path, 100, false, self.show_hidden);
        let plain = crust::strip_ansi(&preview_text);
        let context = if plain.len() > 2000 { &plain[..2000] } else { &plain };

        let prompt = format!(
            "Summarize the purpose of this file/directory: {}. Content preview:\n{}",
            name, context
        );

        let messages = serde_json::json!([{"role": "user", "content": prompt}]);
        match ai_request(&self.config.ai_model, &self.config.ai_key, &messages) {
            Some(content) => self.show_in_right(&content),
            None => self.msg_error("AI request failed"),
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

            let messages = serde_json::Value::Array(history.clone());
            match ai_request(&self.config.ai_model, &self.config.ai_key, &messages) {
                Some(content) => {
                    history.push(serde_json::json!({"role": "assistant", "content": content}));
                    let display: Vec<String> = history.iter().map(|m| {
                        let role = m["role"].as_str().unwrap_or("");
                        let text = m["content"].as_str().unwrap_or("");
                        if role == "user" {
                            format!("{}: {}", style::fg("You", 81), text)
                        } else {
                            format!("{}: {}", style::fg("AI", 46), text)
                        }
                    }).collect();
                    self.show_in_right(&display.join("\n\n"));
                }
                None => { self.msg_error("AI request failed"); break; }
            }
        }
    }
}
