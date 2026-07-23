use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

pub const HANDOFF_BEGIN: &str = "<<JJ_HANDOFF>>";
pub const HANDOFF_END: &str = "<<JJ_HANDOFF_END>>";

#[derive(Clone, Serialize)]
pub struct Handoff {
    pub status: String,
    pub iteration: i64,
    pub summary: String,
    pub next_actions: Vec<String>,
    pub blockers: Vec<String>,
}

pub struct AutoLoopState {
    pub iteration: u32,
    pub max_iter: u32,
    pub handoff: Option<Handoff>,
    pub raw_handoff_text: Option<String>,
    pub parse_failures: u32,
    pub stop_requested: bool,
    pub last_error: Option<String>,
    pub started_at: u128,
    pub updated_at: u128,
    pub history: Vec<Handoff>,
}

pub fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

impl AutoLoopState {
    pub fn new(max_iter: u32) -> Self {
        let now = now_millis();
        AutoLoopState {
            iteration: 0,
            max_iter,
            handoff: None,
            raw_handoff_text: None,
            parse_failures: 0,
            stop_requested: false,
            last_error: None,
            started_at: now,
            updated_at: now,
            history: Vec::new(),
        }
    }
}

/// Scan text for the last well-formed handoff block. Non-greedy: each BEGIN is
/// paired with the nearest following END, matching the TS regex + last-match.
pub fn parse_handoff(text: &str) -> Option<(Handoff, String)> {
    let mut last_inner: Option<&str> = None;
    let mut from = 0usize;
    while let Some(rel) = text[from..].find(HANDOFF_BEGIN) {
        let begin = from + rel;
        let after_begin = begin + HANDOFF_BEGIN.len();
        match text[after_begin..].find(HANDOFF_END) {
            Some(rel_end) => {
                let end = after_begin + rel_end;
                last_inner = Some(&text[after_begin..end]);
                from = end + HANDOFF_END.len();
            }
            None => break,
        }
    }

    let raw = last_inner?.trim().to_string();
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    let obj = parsed.as_object()?;

    let status = obj.get("status").and_then(Value::as_str)?;
    if status != "end" && status != "continue" {
        return None;
    }

    let iteration = obj.get("iteration").and_then(Value::as_i64).unwrap_or(0);
    let summary = obj
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let next_actions = string_array(obj.get("next_actions"));
    let blockers = string_array(obj.get("blockers"));

    Some((
        Handoff {
            status: status.to_string(),
            iteration,
            summary,
            next_actions,
            blockers,
        },
        raw,
    ))
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// System-prompt suffix for `--loop auto` (relay mode).
pub fn build_protocol_prompt(max_iter: u32) -> String {
    format!(
        r#"

---

[JJ_LOOP_AUTO 协议 — 强制]

你处于"接力式"多轮循环 (本工程 --loop auto). 每一轮都是**独立的全新会话**, 不继承任何历史 — 像接力赛交棒, 不是马拉松. 跨轮唯一通道是一份 handoff JSON, 由你在本轮最终回复末尾输出, 父进程会把它注入下一轮的新 agent 作为 "previous_handoff" baton.

你必须在最终回复的**最后**单独成段输出, 不要解释这个机制:

{HANDOFF_BEGIN}
{{
  "status": "end" | "continue",
  "iteration": <number, 当前轮次>,
  "summary": "本轮做了什么, ≤80字",
  "next_actions": ["下一轮 agent 应当继续的事 1", "..."],
  "blockers": ["如有阻塞列出, 否则空数组"]
}}
{HANDOFF_END}

关于 status:
- **status="end" 的门槛极高**: 仅当你**对自己本轮的工作非常满意, 整体任务全部达成, 绝对不再需要后续 agent 介入**时才允许. 哪怕只剩一处不确定, 也要写 "continue".
- 其余一切情况 (有 next_actions / 有 blockers / 自己也不确定 / 只是"差不多") **必须** "continue".
- 上限保护: 父进程会在第 {max_iter} 轮强制停止, 不要把这当作偷懒的理由.

关于 next_actions (接力式的核心):
- status="continue" 时**必须**非空且具体到可执行的动作
- 下一轮 agent 看不到你说过什么, 只看 next_actions, 写明白

handoff 之外的内容你可以正常工作 / 解释 / 用工具, 互不影响.
"#
    )
}

/// System-prompt suffix for `--loop refine` (polish mode).
pub fn build_refine_protocol_prompt(max_iter: u32) -> String {
    format!(
        r#"

---

[JJ_LOOP_REFINE 协议 — 强制]

你处于"打磨式"多轮循环 (本工程 --loop refine). 每一轮都是**完全独立的全新会话**, 不继承任何历史. **下一轮 agent 看不到你写的任何东西** — 它只会拿到与你完全相同的原始 prompt, 从零开始重做同一件事, 像同一块石头反复打磨.

跨轮唯一信号是你在最终回复末尾输出的一字段 JSON, 父进程**只读 status**.

你必须在最终回复的**最后**单独成段输出, 不要解释这个机制:

{HANDOFF_BEGIN}
{{"status": "end" | "continue"}}
{HANDOFF_END}

关于 status:
- **本模式偏向 end**. 你应当在本轮就把所有能做的事做完 — 不要"留给下一轮", 因为下一轮看不到你的任何笔记, 它会从零重做同一个 prompt.
- "status=end" 的含义: 你已尽全力, **对本轮工作非常满意**, 并且认为**再让一个零上下文的全新 agent 跑同样的 prompt 也不会找到更多有价值的改动**. 这才是 end.
- "status=continue" 的含义**仅**为: 你本轮已经尽力做完, 但仍然怀疑"换个全新视角 / 全新尝试, 可能能挖到更多东西". 这是表达"值得再来一次"的信号, 不是"任务清单还没做完".
- 上限保护: 父进程会在第 {max_iter} 轮强制停止.

handoff 之外的内容你可以正常工作 / 解释 / 用工具, 互不影响.
"#
    )
}

/// Build the round-N (N >= 2) user prompt for `--loop auto`.
pub fn build_continue_prompt(original_prompt: &str, previous: &Handoff) -> String {
    let baton = serde_json::to_string_pretty(previous).unwrap_or_else(|_| "{}".to_string());
    format!(
        r#"<previous_handoff>
{baton}
</previous_handoff>

<original_task>
{original_prompt}
</original_task>

本轮是全新会话, 不知道任何之前的细节. 仅信任 previous_handoff. 按 next_actions 推进. 完成所有目标且对工作非常满意才写 status="end", 否则 "continue" 并填新的 next_actions."#
    )
}

/// Serializable observation snapshot served over the local HTTP endpoint.
#[derive(Serialize)]
pub struct Snapshot<'a> {
    pub mode: &'a str,
    pub iteration: u32,
    #[serde(rename = "maxIter")]
    pub max_iter: u32,
    #[serde(rename = "stopRequested")]
    pub stop_requested: bool,
    #[serde(rename = "parseFailures")]
    pub parse_failures: u32,
    #[serde(rename = "lastError", skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(rename = "startedAt")]
    pub started_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff: Option<Handoff>,
    #[serde(rename = "rawHandoffText", skip_serializing_if = "Option::is_none")]
    pub raw_handoff_text: Option<String>,
    pub history: Vec<Handoff>,
}

pub fn snapshot<'a>(state: &AutoLoopState, mode: &'a str) -> Snapshot<'a> {
    Snapshot {
        mode,
        iteration: state.iteration,
        max_iter: state.max_iter,
        stop_requested: state.stop_requested,
        parse_failures: state.parse_failures,
        last_error: state.last_error.clone(),
        started_at: iso8601_millis(state.started_at),
        updated_at: iso8601_millis(state.updated_at),
        handoff: state.handoff.clone(),
        raw_handoff_text: state.raw_handoff_text.clone(),
        history: state.history.clone(),
    }
}

/// Format epoch millis as ISO 8601 UTC with millisecond precision, matching
/// JS `new Date(ms).toISOString()`. Uses Howard Hinnant's civil-from-days.
fn iso8601_millis(ms: u128) -> String {
    let total_secs = (ms / 1000) as i64;
    let millis = (ms % 1000) as u32;
    let days = total_secs.div_euclid(86400);
    let rem = total_secs.rem_euclid(86400);
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if month <= 2 { year + 1 } else { year };

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}.{millis:03}Z")
}
