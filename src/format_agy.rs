// Formats Antigravity CLI (`agy --output-format stream-json`) events into
// human-readable terminal text. `agy` publishes no protocol schema, so fields
// are duck-typed; shapes verified 2026-08-17 against a live run:
//   {"event":"init","init":{"model","cwd","tools":[…],"permission_mode"}}
//   {"event":"step_update","step_update":{"step_index","state":"ACTIVE"|"DONE",
//      "step_type":"user_input"|"agent_response"|"tool"|"checkpoint"|…,
//      "text_delta"?,"tool_name"?,"tool_info":{"name","parameters","output"}?}}
//   {"event":"result","result":{"status","response","duration_seconds",
//      "num_turns","usage":{…}}}
// `result.response` repeats the whole answer, so only its metadata is rendered.

use std::collections::HashSet;

use serde_json::Value;

use crate::format_utils::{fmt_num, preview_opt, preview_value};

fn gs<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}
fn gn(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}
fn gr<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|x| x.is_object())
}
fn nonempty(s: Option<&str>) -> Option<&str> {
    s.filter(|x| !x.is_empty())
}

pub struct AgyStreamFormatter {
    /// step whose text stream is open: its deltas concatenate, any other output
    /// closes it so the next delta starts on a fresh line.
    open_text_step: Option<i64>,
    /// Tool steps already announced — a tool emits ACTIVE then DONE with the same
    /// parameters, and the header belongs to the first of the two.
    announced_tools: HashSet<i64>,
}

impl AgyStreamFormatter {
    pub fn new() -> Self {
        AgyStreamFormatter {
            open_text_step: None,
            announced_tools: HashSet::new(),
        }
    }

    pub fn format(&mut self, raw: &Value) -> String {
        if !raw.is_object() {
            return String::new();
        }
        match gs(raw, "event") {
            Some("init") => {
                self.open_text_step = None;
                match gr(raw, "init") {
                    // `model` is empty unless `--model` was passed — agy resolves
                    // its own default without reporting it, so skip the blank line.
                    Some(init) => {
                        let mut lines: Vec<String> = Vec::new();
                        if let Some(model) = nonempty(gs(init, "model")) {
                            lines.push(format!("model: {model}"));
                        }
                        lines.push(format!("cwd: {}", gs(init, "cwd").unwrap_or("")));
                        lines.push(format!(
                            "tools: {}",
                            init.get("tools")
                                .and_then(Value::as_array)
                                .map(|a| a.len())
                                .unwrap_or(0)
                        ));
                        format!("\n---\n{}\n---\n", lines.join("\n"))
                    }
                    None => String::new(),
                }
            }
            Some("step_update") => match gr(raw, "step_update") {
                Some(step) => self.format_step(step),
                None => String::new(),
            },
            Some("result") => match gr(raw, "result") {
                Some(result) => self.format_result(result),
                None => String::new(),
            },
            other => format!(
                "\n[unknown_event:{}] {}\n",
                other.unwrap_or("unknown"),
                preview_value(raw, 800)
            ),
        }
    }

    fn format_step(&mut self, step: &Value) -> String {
        let index = gn(step, "step_index").map(|n| n as i64).unwrap_or(-1);
        match gs(step, "step_type") {
            Some("agent_response") => self.emit_text(index, gs(step, "text_delta").unwrap_or("")),
            Some("tool") => {
                self.open_text_step = None;
                self.format_tool(index, gs(step, "state").unwrap_or(""), step)
            }
            // Everything else (user_input / checkpoint / bookkeeping steps, and
            // any step type a future agy adds) carries nothing to show.
            _ => String::new(),
        }
    }

    fn emit_text(&mut self, index: i64, delta: &str) -> String {
        if delta.is_empty() {
            return String::new();
        }
        if self.open_text_step == Some(index) {
            return delta.to_string();
        }
        self.open_text_step = Some(index);
        format!("\n{delta}")
    }

    fn format_tool(&mut self, index: i64, state: &str, step: &Value) -> String {
        let info = gr(step, "tool_info");
        let name = nonempty(gs(step, "tool_name"))
            .or_else(|| info.and_then(|i| nonempty(gs(i, "name"))))
            .unwrap_or("unknown");

        let mut out = String::new();
        if self.announced_tools.insert(index) {
            out += &format!(
                "\n[tool: {name}] {}\n",
                preview_opt(info.and_then(|i| i.get("parameters")), 200)
            );
        }
        if state == "DONE" {
            if let Some(output) = info.and_then(|i| i.get("output")).filter(|v| match v {
                Value::Null => false,
                Value::String(s) => !s.is_empty(),
                _ => true,
            }) {
                out += &format!("[tool_result] {}\n", preview_value(output, 500));
            }
        }
        out
    }

    fn format_result(&mut self, result: &Value) -> String {
        self.open_text_step = None;
        let mut lines = vec![
            format!("status: {}", gs(result, "status").unwrap_or("unknown")),
            format!(
                "duration: {:.1}s",
                gn(result, "duration_seconds").unwrap_or(0.0)
            ),
            format!("turns: {}", fmt_num(gn(result, "num_turns").unwrap_or(0.0))),
        ];
        if let Some(usage) = gr(result, "usage") {
            for key in [
                "input_tokens",
                "cache_read_tokens",
                "output_tokens",
                "thinking_tokens",
            ] {
                lines.push(format!("{key}: {}", fmt_num(gn(usage, key).unwrap_or(0.0))));
            }
        }
        if let Some(err) = nonempty(gs(result, "error")) {
            lines.push(format!("error: {err}"));
        }
        format!("\n\n---\n{}\n---\n", lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn deltas_of_one_step_concatenate() {
        let mut f = AgyStreamFormatter::new();
        let a = ev(r#"{"event":"step_update","step_update":{"step_index":2,"state":"ACTIVE","step_type":"agent_response","text_delta":"he"}}"#);
        let b = ev(r#"{"event":"step_update","step_update":{"step_index":2,"state":"DONE","step_type":"agent_response","text_delta":"llo"}}"#);
        assert_eq!(f.format(&a), "\nhe");
        assert_eq!(f.format(&b), "llo");
    }

    #[test]
    fn tool_header_prints_once_and_result_on_done() {
        let mut f = AgyStreamFormatter::new();
        let active = ev(r#"{"event":"step_update","step_update":{"step_index":3,"state":"ACTIVE","step_type":"tool","tool_name":"run_command","tool_info":{"name":"run_command","parameters":{"CommandLine":"ls"}}}}"#);
        let done = ev(r#"{"event":"step_update","step_update":{"step_index":3,"state":"DONE","step_type":"tool","tool_name":"run_command","tool_info":{"name":"run_command","parameters":{"CommandLine":"ls"},"output":"a.txt\n"}}}"#);
        assert_eq!(
            f.format(&active),
            "\n[tool: run_command] {\"CommandLine\":\"ls\"}\n"
        );
        assert_eq!(f.format(&done), "[tool_result] a.txt\n\n");
    }

    #[test]
    fn text_after_a_tool_starts_a_new_line() {
        let mut f = AgyStreamFormatter::new();
        f.format(&ev(
            r#"{"event":"step_update","step_update":{"step_index":2,"state":"ACTIVE","step_type":"agent_response","text_delta":"x"}}"#,
        ));
        f.format(&ev(
            r#"{"event":"step_update","step_update":{"step_index":3,"state":"ACTIVE","step_type":"tool","tool_name":"t"}}"#,
        ));
        let again = ev(r#"{"event":"step_update","step_update":{"step_index":2,"state":"DONE","step_type":"agent_response","text_delta":"y"}}"#);
        assert_eq!(f.format(&again), "\ny");
    }

    #[test]
    fn bookkeeping_steps_are_silent() {
        let mut f = AgyStreamFormatter::new();
        for t in ["user_input", "checkpoint", "unknown", "something_new"] {
            let e = ev(&format!(
                r#"{{"event":"step_update","step_update":{{"step_index":0,"state":"DONE","step_type":"{t}"}}}}"#
            ));
            assert_eq!(f.format(&e), "");
        }
    }

    #[test]
    fn result_renders_metadata_not_the_response() {
        let mut f = AgyStreamFormatter::new();
        let e = ev(r#"{"event":"result","result":{"status":"SUCCESS","response":"the whole answer","duration_seconds":2.9448,"num_turns":1,"usage":{"input_tokens":21669,"output_tokens":197,"thinking_tokens":106,"cache_read_tokens":12209}}}"#);
        let out = f.format(&e);
        assert!(!out.contains("the whole answer"));
        assert!(out.contains("status: SUCCESS"));
        assert!(out.contains("duration: 2.9s"));
        assert!(out.contains("input_tokens: 21669"));
    }
}
