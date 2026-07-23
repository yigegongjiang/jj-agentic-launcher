// Formats Codex `exec --json` protocol events into human-readable terminal text.
// Field decisions mirror the former TS formatter, which duck-typed on the
// protocol-shaped JSON emitted by `codex exec --json`. Sources of truth for the
// fields (verified 2026-03-14):
//   https://raw.githubusercontent.com/openai/codex/main/codex-rs/exec/src/exec_events.rs
//   https://raw.githubusercontent.com/openai/codex/main/codex-rs/protocol/src/models.rs

use std::collections::HashMap;

use serde_json::Value;

use crate::format_utils::{fmt_num, preview_opt, preview_value, truncate};

// --- Value accessors (record = JSON object; missing/mistyped -> None) ---

fn gs<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}
fn gn(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}
fn ga<'a>(v: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    v.get(key).and_then(Value::as_array)
}
fn gr<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|x| x.is_object())
}
/// JS truthiness for strings: absent OR empty -> falsy (None).
fn nonempty(s: Option<&str>) -> Option<&str> {
    s.filter(|x| !x.is_empty())
}
fn safe_json(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| v.to_string())
}

pub struct CodexStreamFormatter {
    command_output_state: HashMap<String, String>,
    item_snapshot_state: HashMap<String, String>,
    text_state: HashMap<String, String>,
}

impl CodexStreamFormatter {
    pub fn new() -> Self {
        CodexStreamFormatter {
            command_output_state: HashMap::new(),
            item_snapshot_state: HashMap::new(),
            text_state: HashMap::new(),
        }
    }

    pub fn format(&mut self, event: &Value) -> String {
        if !event.is_object() {
            return self.format_unknown("unknown_event", event);
        }
        match gs(event, "type") {
            Some("thread.started") => {
                match nonempty(gs(event, "thread_id")) {
                    Some(tid) => format!("\n[thread] {tid}\n"),
                    None => String::new(),
                }
            }
            Some("turn.started") => String::new(),
            Some("turn.completed") => self.format_turn_completed(event),
            Some("turn.failed") => {
                let error = gr(event, "error");
                let message = error.and_then(|e| gs(e, "message"));
                format!(
                    "\n[turn_failed] {}\n",
                    message
                        .map(String::from)
                        .unwrap_or_else(|| preview_value(event, 400))
                )
            }
            Some("error") => format!(
                "\n[error] {}\n",
                gs(event, "message")
                    .map(String::from)
                    .unwrap_or_else(|| preview_value(event, 400))
            ),
            Some("item.started") => self.format_item_event("started", event),
            Some("item.updated") => self.format_item_event("updated", event),
            Some("item.completed") => self.format_item_event("completed", event),
            other => self.format_unknown(
                &format!("unknown_event:{}", other.unwrap_or("unknown")),
                event,
            ),
        }
    }

    fn format_turn_completed(&self, event: &Value) -> String {
        let Some(usage) = gr(event, "usage") else {
            return "\n\n---\nturn: completed\n---\n".to_string();
        };
        let input = fmt_num(gn(usage, "input_tokens").unwrap_or(0.0));
        let cached = fmt_num(gn(usage, "cached_input_tokens").unwrap_or(0.0));
        let output = fmt_num(gn(usage, "output_tokens").unwrap_or(0.0));
        format!("\n\n---\ninput_tokens: {input}\ncached_input_tokens: {cached}\noutput_tokens: {output}\n---\n")
    }

    fn format_item_event(&mut self, event_type: &str, event: &Value) -> String {
        let Some(item) = gr(event, "item") else {
            return self.format_unknown(&format!("unknown_item_event:{event_type}"), event);
        };

        match gs(item, "type") {
            Some("agent_message") => self.format_agent_message(item),
            Some("reasoning") => self.format_reasoning(item),
            Some("command_execution") => self.format_command(event_type, item),
            Some("file_change") => self.format_file_change(item),
            Some("mcp_tool_call") => self.format_mcp_call(event_type, item),
            Some("collab_tool_call") => self.format_collab_tool_call(event_type, item),
            Some("web_search") => self.format_web_search(item),
            Some("todo_list") => self.format_todo_list(item),
            Some("error") => format!(
                "\n[error] {}\n",
                gs(item, "message")
                    .map(String::from)
                    .unwrap_or_else(|| preview_value(item, 400))
            ),
            other => self.format_unknown(
                &format!("unknown_item:{}", other.unwrap_or("unknown")),
                item,
            ),
        }
    }

    fn format_agent_message(&mut self, item: &Value) -> String {
        let id = nonempty(gs(item, "id"));
        let text = gs(item, "text").unwrap_or("");
        match id {
            None => {
                if text.is_empty() {
                    String::new()
                } else {
                    format!("\n{text}")
                }
            }
            Some(id) => self.format_text_delta(id, text, false, "\n", "message"),
        }
    }

    fn format_reasoning(&mut self, item: &Value) -> String {
        let id = nonempty(gs(item, "id"));
        let text = gs(item, "text").unwrap_or("");
        match id {
            None => {
                if text.is_empty() {
                    "\n[thinking] ".to_string()
                } else {
                    format!("\n[thinking] {text}")
                }
            }
            Some(id) => self.format_text_delta(id, text, true, "\n[thinking] ", "thinking"),
        }
    }

    fn format_command(&mut self, event_type: &str, item: &Value) -> String {
        let id = nonempty(gs(item, "id"));
        let output = gs(item, "aggregated_output").unwrap_or("");
        let status = gs(item, "status").unwrap_or("unknown");

        let mut result = String::new();
        if event_type == "started" {
            let command = gs(item, "command").unwrap_or("(unknown command)");
            result += &format!("\n[command:{status}] {command}\n");
        }

        if let Some(id) = id {
            let delta = self.format_command_output_delta(id, output);
            if !delta.is_empty() {
                result += &delta;
            }
        } else if !output.is_empty() {
            result += output;
        }

        if event_type == "completed" {
            let tail = match gn(item, "exit_code") {
                None => status.to_string(),
                Some(code) => format!("{} ({status})", fmt_num(code)),
            };
            result += &format!("\n[command_exit] {tail}\n");
        }

        result
    }

    fn format_file_change(&mut self, item: &Value) -> String {
        let status = gs(item, "status").unwrap_or("unknown");
        let changes = ga(item, "changes")
            .map(|arr| {
                arr.iter()
                    .map(|change| {
                        if !change.is_object() {
                            preview_value(change, 120)
                        } else {
                            format!(
                                "{} {}",
                                gs(change, "kind").unwrap_or("unknown"),
                                gs(change, "path").unwrap_or("(unknown path)")
                            )
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();

        let body = if changes.is_empty() { "(empty)" } else { &changes };
        let rendered = format!("\n[file_change:{status}] {body}\n");
        self.format_snapshot(gs(item, "id"), rendered, item, "file_change")
    }

    fn format_mcp_call(&mut self, event_type: &str, item: &Value) -> String {
        let server = gs(item, "server").unwrap_or("(unknown server)");
        let tool = gs(item, "tool").unwrap_or("(unknown tool)");
        let status = gs(item, "status").unwrap_or("unknown");

        if event_type == "updated" {
            return self.format_snapshot(gs(item, "id"), String::new(), item, "mcp_tool_call");
        }

        if event_type == "started" {
            return format!(
                "\n[mcp:{server}/{tool}:{status}] {}\n",
                preview_opt(item.get("arguments"), 200)
            );
        }

        // completed
        if let Some(error) = gr(item, "error") {
            return format!(
                "\n[mcp_error] {}\n",
                gs(error, "message")
                    .map(String::from)
                    .unwrap_or_else(|| preview_value(error, 300))
            );
        }

        let Some(result) = gr(item, "result") else {
            return format!("\n[mcp:{server}/{tool}] status={status}\n");
        };

        let mut output = String::new();
        if let Some(sc) = result.get("structured_content") {
            output += &format!("\n[mcp_result] {}\n", preview_value(sc, 500));
        }
        if let Some(content) = result.get("content") {
            output += &format!("\n[mcp_content] {}\n", preview_value(content, 500));
        }
        if output.is_empty() {
            format!("\n[mcp_result] {}\n", preview_value(result, 500))
        } else {
            output
        }
    }

    fn format_collab_tool_call(&mut self, event_type: &str, item: &Value) -> String {
        let tool = gs(item, "tool").unwrap_or("unknown");
        let status = gs(item, "status").unwrap_or("unknown");
        let sender = nonempty(gs(item, "sender_thread_id"));
        let receivers = ga(item, "receiver_thread_ids").map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        });

        let mut result = format!("\n[collab:{tool}:{status}]");
        if let Some(s) = sender {
            result += &format!(" sender={s}");
        }
        if let Some(r) = &receivers {
            if !r.is_empty() {
                result += &format!(" receivers={r}");
            }
        }
        result += "\n";

        if let Some(prompt) = nonempty(gs(item, "prompt")) {
            result += &format!("prompt: {}\n", truncate(prompt, 240));
        }

        if let Some(states) = gr(item, "agents_states").and_then(Value::as_object) {
            let entries = states
                .iter()
                .map(|(agent_id, state)| {
                    if !state.is_object() {
                        format!("{agent_id}:{}", preview_value(state, 80))
                    } else {
                        let s = gs(state, "status").unwrap_or("unknown");
                        match nonempty(gs(state, "message")) {
                            Some(msg) => format!("{agent_id}:{s} ({})", truncate(msg, 80)),
                            None => format!("{agent_id}:{s}"),
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            if !entries.is_empty() {
                result += &format!("agents: {entries}\n");
            }
        }

        self.format_snapshot(
            gs(item, "id"),
            result,
            item,
            &format!("collab:{tool}:{event_type}"),
        )
    }

    fn format_web_search(&mut self, item: &Value) -> String {
        let action = gr(item, "action");
        let action_type = action.and_then(|a| gs(a, "type"));

        let query = gs(item, "query").unwrap_or("(unknown query)");
        let label = match action_type {
            Some(t) => format!(":{t}"),
            None => String::new(),
        };
        let mut result = format!("\n[web_search{label}] {query}");

        if let Some(action) = action {
            match action_type {
                Some("search") => {
                    if let Some(q) = nonempty(gs(action, "query")) {
                        if Some(q) != gs(item, "query") {
                            result += &format!("\nquery: {q}");
                        }
                    }
                    if let Some(queries) = ga(action, "queries") {
                        let joined = queries
                            .iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(" | ");
                        if !joined.is_empty() {
                            result += &format!("\nqueries: {joined}");
                        }
                    }
                }
                Some("open_page") | Some("find_in_page") => {
                    if let Some(url) = nonempty(gs(action, "url")) {
                        result += &format!("\nurl: {url}");
                    }
                    if action_type == Some("find_in_page") {
                        if let Some(pattern) = nonempty(gs(action, "pattern")) {
                            result += &format!("\npattern: {pattern}");
                        }
                    }
                }
                _ => {
                    result += &format!("\naction: {}", preview_value(action, 300));
                }
            }
        }

        self.format_snapshot(gs(item, "id"), format!("{result}\n"), item, "web_search")
    }

    fn format_todo_list(&mut self, item: &Value) -> String {
        let content = ga(item, "items")
            .map(|todos| {
                todos
                    .iter()
                    .map(|todo| {
                        if !todo.is_object() {
                            format!("[?] {}", preview_value(todo, 120))
                        } else {
                            let mark = if todo.get("completed").and_then(Value::as_bool)
                                == Some(true)
                            {
                                "[x]"
                            } else {
                                "[ ]"
                            };
                            format!("{mark} {}", gs(todo, "text").unwrap_or("(empty)"))
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();

        let rendered = if content.is_empty() {
            String::new()
        } else {
            format!("\n[todo]\n{content}\n")
        };
        self.format_snapshot(gs(item, "id"), rendered, item, "todo_list")
    }

    // --- Delta / snapshot state machines ---

    fn format_text_delta(
        &mut self,
        id: &str,
        next_value: &str,
        emit_prefix_for_empty_first_chunk: bool,
        prefix: &str,
        snapshot_label: &str,
    ) -> String {
        let previous = self.text_state.get(id).cloned();
        self.text_state.insert(id.to_string(), next_value.to_string());

        let Some(previous) = previous else {
            if next_value.is_empty() {
                return if emit_prefix_for_empty_first_chunk {
                    prefix.to_string()
                } else {
                    String::new()
                };
            }
            return format!("{prefix}{next_value}");
        };

        if next_value == previous {
            return String::new();
        }
        if next_value.starts_with(&previous) {
            return next_value[previous.len()..].to_string();
        }
        if next_value.is_empty() {
            return String::new();
        }
        format!("\n[{snapshot_label}_snapshot] {next_value}")
    }

    fn format_snapshot(
        &mut self,
        id: Option<&str>,
        rendered: String,
        raw_item: &Value,
        fallback_label: &str,
    ) -> String {
        let Some(id) = nonempty(id) else {
            return if rendered.is_empty() {
                self.format_unknown(fallback_label, raw_item)
            } else {
                rendered
            };
        };

        let snapshot = safe_json(raw_item);
        let previous = self.item_snapshot_state.get(id).cloned();
        self.item_snapshot_state.insert(id.to_string(), snapshot.clone());
        if Some(snapshot) == previous {
            String::new()
        } else {
            rendered
        }
    }

    fn format_command_output_delta(&mut self, id: &str, next_value: &str) -> String {
        let previous = self.command_output_state.get(id).cloned();
        self.command_output_state
            .insert(id.to_string(), next_value.to_string());

        let Some(previous) = previous else {
            return next_value.to_string();
        };
        if next_value == previous {
            return String::new();
        }
        if next_value.starts_with(&previous) {
            return next_value[previous.len()..].to_string();
        }
        format!("\n[command_output_snapshot]\n{next_value}")
    }

    fn format_unknown(&self, label: &str, value: &Value) -> String {
        format!("\n[{label}] {}\n", preview_value(value, 800))
    }
}
