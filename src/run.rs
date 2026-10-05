use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::config::{claude_user_skills_off, get_configured_args};
use crate::format_agy::AgyStreamFormatter;
use crate::format_claude::ClaudeStreamFormatter;
use crate::format_codex::CodexStreamFormatter;
use crate::parse::{Invocation, Mode};
use crate::preview::render_launch_preview;
use crate::scenes::{get_scene_text, Engine};

const CLAUDE_BIN: &str = "claude";
const CODEX_BIN: &str = "codex";
const AGY_BIN: &str = "agy";

/// Sent as the priming turn when `agy` is launched as a REPL: the scene has no
/// prompt to ride along with, and `-i` is the only slot that can carry it.
const AGY_REPL_PRIMER: &str =
    "以上是本次会话的系统级设定, 全程生效. 现在只回一行「就位」, 然后等我的指令.";

#[derive(Default)]
pub struct RunOptions {
    /// Accumulate agent-emitted text (formatter output in stream mode, raw bytes
    /// in print mode) so the caller can scan for the handoff sentinel.
    pub capture_agent_text: bool,
    /// Override the prompt embedded in this turn (used by `--loop relay`).
    pub prompt_override: Option<String>,
    /// Extra system-prompt suffix appended to the scene text (handoff protocol).
    pub system_suffix: Option<String>,
}

pub struct RunOutcome {
    pub exit_code: i32,
    pub agent_text: Vec<u8>,
}

enum FormatterKind {
    Claude,
    Codex,
    Agy,
}

enum Fmt {
    Claude(ClaudeStreamFormatter),
    Codex(CodexStreamFormatter),
    Agy(AgyStreamFormatter),
}

struct LaunchPlan {
    args: Vec<String>,
    binary: &'static str,
    formatter: Option<FormatterKind>,
    interactive: bool,
    pre_cmd: Option<String>,
}

pub fn run_invocation(inv: &Invocation, opts: RunOptions) -> Result<RunOutcome, String> {
    let plan = build_launch_plan(inv, &opts)?;
    run_command(plan, opts)
}

fn build_launch_plan(inv: &Invocation, opts: &RunOptions) -> Result<LaunchPlan, String> {
    let mut scene_text = get_scene_text(&inv.scene_id)?;
    if let Some(suffix) = &opts.system_suffix {
        scene_text.push_str(suffix);
    }

    let config_args = get_configured_args(inv.engine, inv.mode);

    let formatter = if matches!(inv.mode, Mode::Stream) {
        Some(match inv.engine {
            Engine::Claude => FormatterKind::Claude,
            Engine::Codex => FormatterKind::Codex,
            Engine::Agy => FormatterKind::Agy,
        })
    } else {
        None
    };

    let user_text = opts
        .prompt_override
        .clone()
        .or_else(|| inv.user_text.clone());
    let project_args = match inv.engine {
        Engine::Codex => codex_project_args(inv),
        Engine::Claude => claude_skills_args(inv, &config_args),
        Engine::Agy => Vec::new(),
    };
    let args = build_final_args(
        inv,
        config_args,
        project_args,
        &scene_text,
        user_text.as_deref(),
    );

    Ok(LaunchPlan {
        args,
        binary: match inv.engine {
            Engine::Claude => CLAUDE_BIN,
            Engine::Codex => CODEX_BIN,
            Engine::Agy => AGY_BIN,
        },
        formatter,
        interactive: matches!(inv.mode, Mode::Interactive),
        pre_cmd: inv.pre_cmd.clone(),
    })
}

/// Project `[[skills.config]]` forwarded to Codex (see `codex_project`). Under
/// `--pre` the cwd is unknown until the pre command ran, so a placeholder is
/// left for `shell::build_script` to resolve at exec time.
fn codex_project_args(inv: &Invocation) -> Vec<String> {
    if inv.pre_cmd.is_some() {
        return vec![crate::shell::CODEX_SKILLS_DEFERRED.to_string()];
    }
    std::env::current_dir()
        .ok()
        .and_then(|cwd| crate::codex_project::skills_override(&cwd))
        .map(|v| vec!["-c".to_string(), v])
        .unwrap_or_default()
}

/// `claude.user_skills_off`: `--settings` hiding user skills the project did not
/// opt into (see `skills`). Deferred under `--pre` like the Codex rules. A
/// user-supplied `--settings` wins; the feature steps aside, loudly.
fn claude_skills_args(inv: &Invocation, config_args: &[String]) -> Vec<String> {
    if !claude_user_skills_off() {
        return Vec::new();
    }
    let user_settings = config_args
        .iter()
        .chain(inv.passthrough_args.as_deref().unwrap_or_default())
        .any(|a| a == "--settings" || a.starts_with("--settings="));
    if user_settings {
        eprintln!("[warn] claude.user_skills_off ignored: --settings already given.");
        return Vec::new();
    }
    if inv.pre_cmd.is_some() {
        return vec![crate::shell::CLAUDE_SKILLS_DEFERRED.to_string()];
    }
    crate::skills::settings_json()
        .map(|json| vec!["--settings".to_string(), json])
        .unwrap_or_default()
}

fn build_final_args(
    inv: &Invocation,
    config_args: Vec<String>,
    project_args: Vec<String>,
    scene_text: &str,
    user_text: Option<&str>,
) -> Vec<String> {
    let interactive = matches!(inv.mode, Mode::Interactive);
    let is_codex_noninteractive = matches!(inv.engine, Engine::Codex) && !interactive;
    let user_text = user_text.filter(|s| !s.is_empty());

    let mut args: Vec<String> = Vec::new();

    if is_codex_noninteractive {
        args.push("exec".to_string());
    }

    if matches!(inv.engine, Engine::Claude) {
        args.extend(sanitize_mcp_config(config_args, inv.pre_cmd.is_some()));
    } else {
        args.extend(config_args);
    }
    // Before scene + passthrough, so a user's own `-- -c skills.config=...` wins.
    args.extend(project_args);

    // Scene injection — structural binding, not user-configurable.
    match inv.engine {
        Engine::Claude => {
            args.push("--append-system-prompt".to_string());
            args.push(scene_text.to_string());
        }
        Engine::Codex => {
            args.push("-c".to_string());
            args.push(format!(
                "developer_instructions={}",
                serde_json::to_string(scene_text).unwrap_or_else(|_| "\"\"".to_string())
            ));
        }
        // agy has no system-prompt flag at all, so the scene rides in the prompt.
        Engine::Agy => {}
    }

    // User passthrough (tokens after `--`) — verbatim, after scene, before prompt.
    if let Some(pt) = &inv.passthrough_args {
        args.extend(pt.iter().cloned());
    }

    if matches!(inv.engine, Engine::Agy) {
        // agy carries the prompt as the *value* of `-p` / `-i` (Go flag parsing),
        // so the two must stay adjacent — that is why the flag is emitted here
        // instead of living in config.json like the rest of the mode args.
        args.push(if interactive { "-i" } else { "-p" }.to_string());
        args.push(build_agy_prompt(scene_text, user_text));
        return args;
    }

    match user_text {
        Some(ut) => args.push(ut.to_string()),
        None => {
            if is_codex_noninteractive {
                args.push(String::new());
            }
        }
    }

    args
}

/// The agy prompt: scene text tagged as session-level setup, then the request
/// verbatim. Claude gets `--append-system-prompt` and Codex
/// `-c developer_instructions=…`; agy exposes no equivalent (verified against
/// `agy --help` and antigravity.google/docs/cli/headless, 2026-08-17), so prompt
/// text is the only channel the scene can reach the model through.
fn build_agy_prompt(scene_text: &str, user_text: Option<&str>) -> String {
    let scene = scene_text.trim_end();
    let tail = user_text.unwrap_or(AGY_REPL_PRIMER);
    format!("<system_instructions>\n{scene}\n</system_instructions>\n\n{tail}")
}

/// Drop non-existent `--mcp-config` file paths so a missing project `.mcp.json`
/// doesn't abort `claude` startup. Inline JSON (starts with `{`) and existing
/// files are kept; a `--mcp-config` left with no surviving source is removed.
///
/// `defer_relative` is set under `--pre`: the engine then runs in whatever cwd
/// the pre command left behind, which this process cannot know, so relative
/// paths are kept verbatim here and gated by `[ -f ]` in the generated script
/// instead (see `shell::build_script`). Absolute paths stay cwd-independent and
/// are still checked here.
fn sanitize_mcp_config(args: Vec<String>, defer_relative: bool) -> Vec<String> {
    let keep = |s: &str| {
        s.trim_start().starts_with('{')
            || (defer_relative && !Path::new(s).is_absolute())
            || Path::new(s).exists()
    };
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if let Some(source) = arg.strip_prefix("--mcp-config=") {
            if keep(source) {
                out.push("--mcp-config".to_string());
                out.push(source.to_string());
            }
            i += 1;
            continue;
        }
        if arg != "--mcp-config" {
            out.push(arg.clone());
            i += 1;
            continue;
        }
        // `--mcp-config <configs...>` is variadic: gather following non-flag tokens.
        let mut sources: Vec<String> = Vec::new();
        let mut j = i + 1;
        while j < args.len() && !args[j].starts_with('-') {
            sources.push(args[j].clone());
            j += 1;
        }
        let kept: Vec<String> = sources.into_iter().filter(|s| keep(s)).collect();
        if !kept.is_empty() {
            out.push("--mcp-config".to_string());
            out.extend(kept);
        }
        i = j;
    }
    out
}

fn render_line(fmt: &mut Fmt, line: &str) -> Option<String> {
    match serde_json::from_str::<Value>(line) {
        Ok(v) => {
            let out = match fmt {
                Fmt::Claude(f) => f.format(&v),
                Fmt::Codex(f) => f.format(&v),
                Fmt::Agy(f) => f.format(&v),
            };
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
        // Malformed line: pass through verbatim (+newline), same as the TS fallback.
        Err(_) => Some(format!("{line}\n")),
    }
}

/// Shell hosting `--pre`. `$SHELL` rather than a fixed `/bin/sh`: jump and picker
/// helpers like `j` are functions defined by the user's rc file, not binaries on
/// PATH, so they only exist in that particular shell.
fn shell_path() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_string())
}

/// The program actually spawned — the engine itself, or the shell that runs
/// `--pre` and then `exec`s into the engine.
fn spawn_program(plan: &LaunchPlan) -> String {
    match plan.pre_cmd {
        Some(_) => shell_path(),
        None => plan.binary.to_string(),
    }
}

fn build_command(plan: &LaunchPlan) -> Command {
    let Some(pre) = &plan.pre_cmd else {
        let mut cmd = Command::new(plan.binary);
        cmd.args(&plan.args);
        return cmd;
    };

    // The engine's stdin is closed inside the script at `exec` time only, so the
    // pre command still sees the inherited stdin and interactive pickers can read
    // their candidate list.
    let script = crate::shell::build_script(pre, plan.binary, &plan.args, !plan.interactive);
    let mut cmd = Command::new(shell_path());
    // `-i` loads the user's rc; without it shell functions and aliases (`j`, `z`,
    // …) simply do not exist.
    cmd.arg("-i").arg("-c").arg(&script);
    // Prompt integrations loaded from rc (iTerm2 et al.) write OSC escapes to
    // stdout at startup, which would corrupt the stream-JSON pipe. `dumb` keeps
    // them silent; the script restores the real TERM before running the pre
    // command, so TUIs there still work.
    cmd.env("TERM", "dumb");
    cmd
}

fn run_command(plan: LaunchPlan, opts: RunOptions) -> Result<RunOutcome, String> {
    if let Some(pre) = &plan.pre_cmd {
        eprint!(
            "{}",
            render_launch_preview(&["pre:".to_string(), pre.clone()])
        );
    }
    let cmd_display: Vec<String> = std::iter::once(plan.binary.to_string())
        .chain(plan.args.iter().cloned())
        .collect();
    eprint!("{}", render_launch_preview(&cmd_display));

    // The line above is what this process assembled, not necessarily what the
    // engine receives: relative --mcp-config paths are resolved in the cwd the
    // pre command ends up in. Say so rather than let the preview mislead.
    if plan.pre_cmd.is_some() {
        let deferred = crate::shell::deferred_mcp_paths(&plan.args);
        if !deferred.is_empty() {
            eprintln!(
                "[note] --mcp-config {} is resolved after `--pre`; dropped if absent in the resulting cwd.",
                deferred.join(" ")
            );
        }
        if plan
            .args
            .iter()
            .any(|a| a == crate::shell::CODEX_SKILLS_DEFERRED)
        {
            eprintln!(
                "[note] {} = project .codex/config.toml [[skills.config]], read after `--pre` in the resulting cwd.",
                crate::shell::CODEX_SKILLS_DEFERRED
            );
        }
        if plan
            .args
            .iter()
            .any(|a| a == crate::shell::CLAUDE_SKILLS_DEFERRED)
        {
            eprintln!(
                "[note] {} = --settings hiding user skills, computed after `--pre` in the resulting cwd.",
                crate::shell::CLAUDE_SKILLS_DEFERRED
            );
        }
    }

    let program = spawn_program(&plan);

    // Interactive (REPL): inherit all stdio, no piping.
    if plan.interactive {
        let status = build_command(&plan)
            .status()
            .map_err(|e| spawn_error_message(&program, &e))?;
        return Ok(RunOutcome {
            exit_code: status.code().unwrap_or(1),
            agent_text: Vec::new(),
        });
    }

    // Non-interactive: pipe stdout so we can tee to terminal + scan for sentinel.
    let mut child = build_command(&plan)
        .stdin(if plan.pre_cmd.is_some() {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| spawn_error_message(&program, &e))?;

    let mut agent_text: Vec<u8> = Vec::new();
    let capture = opts.capture_agent_text;

    let Some(mut stdout) = child.stdout.take() else {
        let status = child
            .wait()
            .map_err(|e| format!("failed to wait for `{program}`: {e}"))?;
        return Ok(RunOutcome {
            exit_code: status.code().unwrap_or(1),
            agent_text,
        });
    };

    let mut fmt: Option<Fmt> = plan.formatter.map(|k| match k {
        FormatterKind::Claude => Fmt::Claude(ClaudeStreamFormatter::new()),
        FormatterKind::Codex => Fmt::Codex(CodexStreamFormatter::new()),
        FormatterKind::Agy => Fmt::Agy(AgyStreamFormatter::new()),
    });

    let stdout_handle = io::stdout();
    let mut out = stdout_handle.lock();

    let emit = |bytes: &[u8], out: &mut io::StdoutLock, agent_text: &mut Vec<u8>| {
        let _ = out.write_all(bytes);
        let _ = out.flush();
        if capture {
            agent_text.extend_from_slice(bytes);
        }
    };

    let mut buf = [0u8; 65536];
    let mut linebuf: Vec<u8> = Vec::new();

    loop {
        match stdout.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                match &mut fmt {
                    None => {
                        // print mode: raw passthrough + scanner feed
                        emit(chunk, &mut out, &mut agent_text);
                    }
                    Some(f) => {
                        // stream mode: split JSONL on the \n byte (never inside a
                        // multibyte sequence), decode + format each whole line.
                        linebuf.extend_from_slice(chunk);
                        while let Some(pos) = linebuf.iter().position(|&b| b == b'\n') {
                            let line: Vec<u8> = linebuf.drain(..=pos).collect();
                            let s = String::from_utf8_lossy(&line);
                            let trimmed = s.trim();
                            if !trimmed.is_empty() {
                                if let Some(text) = render_line(f, trimmed) {
                                    emit(text.as_bytes(), &mut out, &mut agent_text);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                // A mid-stream error must not abort the outer loop; surface it and
                // let the child finish so the caller sees a real exit code.
                eprintln!("[warn] stdout stream error from `{program}`: {e}");
                break;
            }
        }
    }

    // Flush any trailing partial line (stream mode only; print mode never buffers).
    if let Some(f) = &mut fmt {
        let s = String::from_utf8_lossy(&linebuf);
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            if let Some(text) = render_line(f, trimmed) {
                emit(text.as_bytes(), &mut out, &mut agent_text);
            }
        }
    }

    drop(out);
    let status = child
        .wait()
        .map_err(|e| format!("failed to wait for `{}`: {e}", plan.binary))?;

    Ok(RunOutcome {
        exit_code: status.code().unwrap_or(1),
        agent_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::LoopSpec;

    fn inv(engine: Engine, mode: Mode, prompt: Option<&str>) -> Invocation {
        Invocation {
            engine,
            mode,
            scene_id: "default".to_string(),
            user_text: prompt.map(String::from),
            user_texts: None,
            loop_spec: LoopSpec::Fixed(1),
            passthrough_args: None,
            pre_cmd: None,
        }
    }

    #[test]
    fn agy_folds_the_scene_into_the_prompt() {
        let i = inv(Engine::Agy, Mode::Stream, Some("do it"));
        let args = build_final_args(&i, vec!["--x".into()], vec![], "SCENE\n", Some("do it"));
        assert_eq!(
            args,
            vec![
                "--x".to_string(),
                "-p".to_string(),
                "<system_instructions>\nSCENE\n</system_instructions>\n\ndo it".to_string(),
            ]
        );
    }

    #[test]
    fn agy_repl_sends_the_scene_as_the_priming_turn() {
        let i = inv(Engine::Agy, Mode::Interactive, None);
        let args = build_final_args(&i, vec![], vec![], "SCENE", None);
        assert_eq!(args[0], "-i");
        assert!(args[1].starts_with("<system_instructions>\nSCENE\n</system_instructions>"));
        assert!(args[1].ends_with(AGY_REPL_PRIMER));
    }

    #[test]
    fn agy_passthrough_stays_before_the_prompt_flag() {
        let mut i = inv(Engine::Agy, Mode::Stream, Some("hi"));
        i.passthrough_args = Some(vec!["--model".into(), "m".into()]);
        let args = build_final_args(&i, vec![], vec![], "S", Some("hi"));
        assert_eq!(args[..3], ["--model", "m", "-p"]);
    }

    #[test]
    fn claude_and_codex_argv_is_unchanged() {
        let c = inv(Engine::Claude, Mode::Stream, Some("hi"));
        let args = build_final_args(&c, vec!["-p".into()], vec![], "S", Some("hi"));
        assert_eq!(args, vec!["-p", "--append-system-prompt", "S", "hi"]);

        let x = inv(Engine::Codex, Mode::Stream, Some("hi"));
        let args = build_final_args(&x, vec!["--json".into()], vec![], "S", Some("hi"));
        assert_eq!(
            args,
            vec!["exec", "--json", "-c", "developer_instructions=\"S\"", "hi"]
        );
    }

    #[test]
    fn codex_project_skills_sit_between_config_and_scene() {
        let mut x = inv(Engine::Codex, Mode::Stream, Some("hi"));
        x.passthrough_args = Some(vec!["-c".into(), "user=1".into()]);
        let project = vec!["-c".to_string(), "skills.config=[]".to_string()];
        let args = build_final_args(&x, vec!["--json".into()], project, "S", Some("hi"));
        assert_eq!(
            args,
            vec![
                "exec",
                "--json",
                "-c",
                "skills.config=[]",
                "-c",
                "developer_instructions=\"S\"",
                "-c",
                "user=1",
                "hi"
            ]
        );
    }
}

fn spawn_error_message(binary: &str, err: &io::Error) -> String {
    let msg = err.to_string();
    if err.kind() == io::ErrorKind::NotFound
        || msg.to_lowercase().contains("not found")
        || msg.to_lowercase().contains("no such file")
    {
        format!(
            "Failed to spawn `{binary}`: {msg}. Make sure it is installed and available in PATH."
        )
    } else {
        format!("Failed to spawn `{binary}`: {msg}")
    }
}
