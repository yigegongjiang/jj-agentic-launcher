use serde_json::{Map, Value};

use crate::format_utils::{fmt_num, preview_value};

/// Formats Claude Code `stream-json` events into human-readable terminal text.
/// Stateless — every field decision is duck-typed on the parsed JSON, matching
/// the shapes the former TS formatter keyed on.
pub struct ClaudeStreamFormatter;

impl ClaudeStreamFormatter {
    pub fn new() -> Self {
        ClaudeStreamFormatter
    }

    pub fn format(&mut self, raw: &Value) -> String {
        let Some(msg) = raw.as_object() else {
            return String::new();
        };

        match msg.get("type").and_then(Value::as_str) {
            Some("system") => {
                if msg.contains_key("model") {
                    let model = msg.get("model").and_then(Value::as_str).unwrap_or("");
                    let cwd = msg.get("cwd").and_then(Value::as_str).unwrap_or("");
                    let tools = msg
                        .get("tools")
                        .and_then(Value::as_array)
                        .map(|a| a.len())
                        .unwrap_or(0);
                    format!("\n---\nmodel: {model}\ncwd: {cwd}\ntools: {tools}\n---\n")
                } else {
                    String::new()
                }
            }

            Some("stream_event") => format_stream_event(msg.get("event")),

            Some("user") => match msg.get("tool_use_result") {
                Some(Value::Object(map)) => {
                    let formatted = map
                        .iter()
                        .map(|(k, v)| format!("{k}: {}", preview_value(v, 200)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("\n[tool_result] {formatted}")
                }
                _ => String::new(),
            },

            Some("assistant") => String::new(),

            Some("result") => format_result(msg),

            _ => String::new(),
        }
    }
}

fn format_result(msg: &Map<String, Value>) -> String {
    let num = |k: &str| msg.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let subtype = msg.get("subtype").and_then(Value::as_str).unwrap_or("");
    let cost = num("total_cost_usd");

    let mut lines = vec![
        "type: result".to_string(),
        format!("subtype: {subtype}"),
        format!("duration: {}s", fmt_num(num("duration_ms") / 1000.0)),
        format!("api_duration: {}s", fmt_num(num("duration_api_ms") / 1000.0)),
        format!("turns: {}", fmt_num(num("num_turns"))),
        format!("cost: ${cost:.4}"),
    ];
    if msg.get("is_error").and_then(Value::as_bool) == Some(true) {
        lines.push("is_error: true".to_string());
    }
    format!("\n\n---\n{}\n", lines.join("\n"))
}

fn format_stream_event(event: Option<&Value>) -> String {
    let Some(event) = event.and_then(Value::as_object) else {
        return String::new();
    };

    match event.get("type").and_then(Value::as_str) {
        Some("content_block_start") => {
            let block = event.get("content_block").and_then(Value::as_object);
            let block_type = block
                .and_then(|b| b.get("type"))
                .and_then(Value::as_str);
            match block_type {
                Some("thinking") => "\n[thinking] ".to_string(),
                Some("text") => "\n".to_string(),
                Some("tool_use") => {
                    let name = block
                        .and_then(|b| b.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    format!("\n[tool: {name}] ")
                }
                _ => String::new(),
            }
        }

        Some("content_block_delta") => {
            let delta = event.get("delta").and_then(Value::as_object);
            let delta_type = delta.and_then(|d| d.get("type")).and_then(Value::as_str);
            let field = match delta_type {
                Some("thinking_delta") => "thinking",
                Some("text_delta") => "text",
                Some("input_json_delta") => "partial_json",
                _ => return String::new(),
            };
            delta
                .and_then(|d| d.get(field))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        }

        _ => String::new(),
    }
}
