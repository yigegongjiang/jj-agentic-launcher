use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::config::get_configured_args;
use crate::format_claude::ClaudeStreamFormatter;
use crate::format_codex::CodexStreamFormatter;
use crate::parse::{Invocation, Mode};
use crate::preview::render_launch_preview;
use crate::scenes::{get_scene_text, Engine};

const CLAUDE_BIN: &str = "claude";
const CODEX_BIN: &str = "codex";

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
}

enum Fmt {
    Claude(ClaudeStreamFormatter),
    Codex(CodexStreamFormatter),
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

    let is_claude = matches!(inv.engine, Engine::Claude);
    let config_args = get_configured_args(inv.engine, inv.mode);

    let formatter = if matches!(inv.mode, Mode::Stream) {
        Some(if is_claude {
            FormatterKind::Claude
        } else {
            FormatterKind::Codex
        })
    } else {
        None
    };

    let user_text = opts
        .prompt_override
        .clone()
        .or_else(|| inv.user_text.clone());
    let args = build_final_args(inv, config_args, &scene_text, user_text.as_deref());

    Ok(LaunchPlan {
        args,
        binary: if is_claude { CLAUDE_BIN } else { CODEX_BIN },
        formatter,
        interactive: matches!(inv.mode, Mode::Interactive),
        pre_cmd: inv.pre_cmd.clone(),
    })
}

fn build_final_args(
    inv: &Invocation,
    config_args: Vec<String>,
    scene_text: &str,
    user_text: Option<&str>,
) -> Vec<String> {
    let is_claude = matches!(inv.engine, Engine::Claude);
    let is_codex_noninteractive = !is_claude && !matches!(inv.mode, Mode::Interactive);

    let mut args: Vec<String> = Vec::new();

    if is_codex_noninteractive {
        args.push("exec".to_string());
    }

    if is_claude {
        args.extend(sanitize_mcp_config(config_args, inv.pre_cmd.is_some()));
    } else {
        args.extend(config_args);
    }

    // Scene injection — structural binding, not user-configurable.
    if is_claude {
        args.push("--append-system-prompt".to_string());
        args.push(scene_text.to_string());
    } else {
        args.push("-c".to_string());
        args.push(format!(
            "developer_instructions={}",
            serde_json::to_string(scene_text).unwrap_or_else(|_| "\"\"".to_string())
        ));
    }

    // User passthrough (tokens after `--`) — verbatim, after scene, before prompt.
    if let Some(pt) = &inv.passthrough_args {
        args.extend(pt.iter().cloned());
    }

    match user_text.filter(|s| !s.is_empty()) {
        Some(ut) => args.push(ut.to_string()),
        None => {
            if is_codex_noninteractive {
                args.push(String::new());
            }
        }
    }

    args
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
