mod config;
mod format_claude;
mod format_codex;
mod format_utils;
mod handoff;
mod init;
mod meta;
mod parse;
mod preview;
mod run;
mod scenes;
mod server;
mod shell;
mod update;

use std::io::Write;
use std::sync::{Arc, Mutex};

use handoff::{
    build_continue_prompt, build_relay_protocol_prompt, build_refine_protocol_prompt, now_millis,
    parse_handoff, LoopState,
};
use meta::{build_help_text, NAME, VERSION};
use parse::{parse_invocation, AppError, Invocation, LoopSpec, DEFAULT_MAX_ITER};
use run::{run_invocation, RunOptions};

fn flush_exit(code: i32) -> ! {
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(code);
}

fn get_raw_args() -> Vec<String> {
    // The compiled binary receives a clean argv; unlike the former Bun build,
    // there is no embedded script path to skip.
    std::env::args_os()
        .skip(1)
        .map(|s| s.to_string_lossy().into_owned())
        .collect()
}

fn handle_meta_command(arg: Option<&str>) -> Option<i32> {
    match arg {
        Some("help") | Some("--help") | Some("-h") => {
            print!("{}", build_help_text());
            Some(0)
        }
        Some("version") | Some("--version") | Some("-v") => {
            println!("{NAME} {VERSION}");
            Some(0)
        }
        Some("update") | Some("upgrade") => match update::update() {
            Ok(code) => Some(code),
            Err(msg) => {
                eprintln!("error: update failed: {msg}");
                Some(1)
            }
        },
        Some("uninstall") => match update::uninstall() {
            Ok(code) => Some(code),
            Err(msg) => {
                eprintln!("error: uninstall failed: {msg}");
                Some(1)
            }
        },
        _ => None,
    }
}

fn parse_positive(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match s.parse::<u32>() {
        Ok(v) if v >= 1 => Some(v),
        _ => None,
    }
}

/// Flags peeled off argv before scene / prompt resolution.
struct Flags {
    args: Vec<String>,
    want_print: bool,
    loop_spec: LoopSpec,
    passthrough: Vec<String>,
    pre_cmd: Option<String>,
}

fn parse_flags(args: &[String]) -> Result<Flags, AppError> {
    let mut want_print = false;
    let mut loop_value: Option<String> = None;
    let mut max_iter = DEFAULT_MAX_ITER;
    let mut saw_max_iter = false;
    let mut pre_cmd: Option<String> = None;
    let mut filtered: Vec<String> = Vec::new();
    let mut passthrough: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        // Everything after a literal `--` is forwarded verbatim to the child.
        if arg == "--" {
            passthrough.extend(args[i + 1..].iter().cloned());
            break;
        }
        if arg == "--print" || arg == "-p" || arg == "print" {
            want_print = true;
            i += 1;
            continue;
        }
        if arg == "--loop" {
            let Some(next) = args.get(i + 1) else {
                return Err(AppError::Usage(
                    "`--loop` requires a value (positive integer, \"relay\", or \"refine\")."
                        .to_string(),
                ));
            };
            loop_value = Some(next.clone());
            i += 2;
            continue;
        }
        if arg == "--max-iter" {
            let Some(next) = args.get(i + 1) else {
                return Err(AppError::Usage(
                    "`--max-iter` requires a positive integer value.".to_string(),
                ));
            };
            match parse_positive(next) {
                Some(v) => max_iter = v,
                None => {
                    return Err(AppError::Usage(format!(
                        "Invalid --max-iter value \"{next}\". Expected a positive integer."
                    )))
                }
            }
            saw_max_iter = true;
            i += 2;
            continue;
        }
        if arg == "--pre" {
            let Some(next) = args.get(i + 1) else {
                return Err(AppError::Usage(
                    "`--pre` requires a shell command value.".to_string(),
                ));
            };
            if next.trim().is_empty() {
                return Err(AppError::Usage(
                    "`--pre` command must not be empty.".to_string(),
                ));
            }
            pre_cmd = Some(next.clone());
            i += 2;
            continue;
        }
        filtered.push(arg.clone());
        i += 1;
    }

    let mut loop_spec = LoopSpec::Fixed(1);
    if let Some(lv) = loop_value {
        if lv == "relay" {
            loop_spec = LoopSpec::Relay(max_iter);
        } else if lv == "refine" {
            loop_spec = LoopSpec::Refine(max_iter);
        } else {
            let Some(v) = parse_positive(&lv) else {
                return Err(AppError::Usage(format!(
                    "Invalid --loop value \"{lv}\". Expected a positive integer, \"relay\", or \"refine\"."
                )));
            };
            if saw_max_iter {
                return Err(AppError::Usage(
                    "`--max-iter` only applies to `--loop relay` / `--loop refine`.".to_string(),
                ));
            }
            loop_spec = LoopSpec::Fixed(v);
        }
    } else if saw_max_iter {
        return Err(AppError::Usage(
            "`--max-iter` requires `--loop relay` or `--loop refine`.".to_string(),
        ));
    }

    Ok(Flags {
        args: filtered,
        want_print,
        loop_spec,
        passthrough,
        pre_cmd,
    })
}

// Stability rule for all loop modes: a single iteration never aborts the loop.
// child exit != 0, spawn errors, stream errors, handoff parse failures — all are
// logged as `[warn]` and the loop proceeds. Only `--max-iter` (relay/refine) or
// the configured count (fixed) terminates the loop. status="end" stops early.

fn run_serial_loop(inv: &Invocation, prompts: &[String]) -> i32 {
    let mut last_exit = 0;
    let total = prompts.len();
    for (idx, prompt) in prompts.iter().enumerate() {
        let step = idx + 1;
        eprintln!("==> step {step}/{total}");
        let opts = RunOptions {
            prompt_override: Some(prompt.clone()),
            ..Default::default()
        };
        match run_invocation(inv, opts) {
            Ok(outcome) => {
                last_exit = outcome.exit_code;
                if last_exit != 0 {
                    let cont = if step < total {
                        "; proceeding to next step."
                    } else {
                        "."
                    };
                    eprintln!("[warn] step {step}/{total}: child exited with code {last_exit}{cont}");
                }
            }
            Err(msg) => {
                last_exit = 1;
                let cont = if step < total {
                    "; proceeding to next step."
                } else {
                    "."
                };
                eprintln!("[warn] step {step}/{total}: runInvocation threw: {msg}{cont}");
            }
        }
    }
    last_exit
}

fn run_fixed_loop(inv: &Invocation, count: u32) -> i32 {
    let mut last_exit = 0;
    for i in 1..=count {
        if count > 1 {
            eprintln!("==> loop {i}/{count}");
        }
        match run_invocation(inv, RunOptions::default()) {
            Ok(outcome) => {
                last_exit = outcome.exit_code;
                if last_exit != 0 {
                    let cont = if i < count {
                        "; proceeding to next iteration."
                    } else {
                        "."
                    };
                    eprintln!("[warn] loop {i}/{count}: child exited with code {last_exit}{cont}");
                }
            }
            Err(msg) => {
                last_exit = 1;
                let cont = if i < count {
                    "; proceeding to next iteration."
                } else {
                    "."
                };
                eprintln!("[warn] loop {i}/{count}: runInvocation threw: {msg}{cont}");
            }
        }
    }
    last_exit
}

fn tail_400(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let start = chars.len().saturating_sub(400);
    chars[start..].iter().collect::<String>().replace('\n', "\\n")
}

fn run_agent_loop(inv: &Invocation, max_iter: u32, mode: &'static str) -> Result<i32, AppError> {
    let Some(original_prompt) = inv.user_text.clone() else {
        return Err(AppError::Usage(format!(
            "`--loop {mode}` requires a prompt argument."
        )));
    };

    let protocol = if mode == "relay" {
        build_relay_protocol_prompt(max_iter)
    } else {
        build_refine_protocol_prompt(max_iter)
    };

    let state = Arc::new(Mutex::new(LoopState::new(max_iter)));
    let port = server::start(state.clone(), mode)
        .map_err(|e| AppError::Other(format!("failed to start observation server: {e}")))?;

    eprintln!("==> --loop {mode} (max {max_iter}) — state: http://127.0.0.1:{port}/handoff");

    loop {
        {
            let s = state.lock().unwrap();
            if s.stop_requested {
                break;
            }
            if s.iteration >= max_iter {
                eprintln!("==> --loop {mode}: max-iter {max_iter} reached, stopping.");
                break;
            }
        }

        let iter = {
            let mut s = state.lock().unwrap();
            s.iteration += 1;
            s.updated_at = now_millis();
            s.iteration
        };
        eprintln!("==> loop {iter}/{max_iter} ({mode})");

        // relay: inject previous handoff as baton from round 2 onwards.
        // refine: every round uses the original prompt verbatim.
        let prompt_override = if mode == "relay" {
            let s = state.lock().unwrap();
            s.handoff
                .as_ref()
                .map(|h| build_continue_prompt(&original_prompt, h))
        } else {
            None
        };

        let opts = RunOptions {
            capture_agent_text: true,
            prompt_override,
            system_suffix: Some(protocol.clone()),
        };

        match run_invocation(inv, opts) {
            Err(msg) => {
                let mut s = state.lock().unwrap();
                s.last_error = Some(format!("iteration {iter} threw: {msg}"));
                s.updated_at = now_millis();
                eprintln!(
                    "[warn] loop {iter}/{max_iter} ({mode}): runInvocation threw: {msg}; proceeding to next iteration."
                );
                continue;
            }
            Ok(outcome) if outcome.exit_code != 0 => {
                let mut s = state.lock().unwrap();
                s.last_error = Some(format!("iteration {iter} exited with code {}", outcome.exit_code));
                s.updated_at = now_millis();
                eprintln!(
                    "[warn] loop {iter}/{max_iter} ({mode}): child exited with code {}; proceeding to next iteration.",
                    outcome.exit_code
                );
                continue;
            }
            Ok(outcome) => {
                let text = String::from_utf8_lossy(&outcome.agent_text);
                match parse_handoff(&text) {
                    None => {
                        let mut s = state.lock().unwrap();
                        s.parse_failures += 1;
                        s.last_error = Some(format!("no handoff sentinel found in turn {iter}"));
                        s.updated_at = now_millis();
                        let failures = s.parse_failures;
                        drop(s);
                        let tail = tail_400(&text);
                        eprintln!(
                            "[warn] loop {iter}/{max_iter} ({mode}): no handoff sentinel (consecutive failures={failures}); proceeding to next iteration. agent_output_tail=\"{tail}\""
                        );
                        continue;
                    }
                    Some((handoff, raw)) => {
                        let mut s = state.lock().unwrap();
                        s.parse_failures = 0;
                        s.handoff = Some(handoff.clone());
                        s.raw_handoff_text = Some(raw);
                        s.history.push(handoff.clone());
                        s.updated_at = now_millis();
                        s.last_error = None;
                        eprintln!(
                            "==> handoff status={}  summary=\"{}\"",
                            handoff.status, handoff.summary
                        );
                        if handoff.status == "end" {
                            s.stop_requested = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    // Surface a non-zero exit if the loop never produced a successful handoff.
    let s = state.lock().unwrap();
    if s.history.is_empty() && s.last_error.is_some() {
        eprintln!(
            "[error] --loop {mode}: no successful handoff across {} iteration(s). lastError={}",
            s.iteration,
            s.last_error.as_deref().unwrap_or("")
        );
        return Ok(4);
    }
    Ok(0)
}

fn run(raw: &[String]) -> Result<i32, AppError> {
    let flags = parse_flags(raw)?;
    let inv = parse_invocation(
        flags.args,
        flags.want_print,
        flags.loop_spec,
        flags.passthrough,
        flags.pre_cmd,
    )?;

    if let Some(texts) = inv.user_texts.clone() {
        Ok(run_serial_loop(&inv, &texts))
    } else {
        match inv.loop_spec {
            LoopSpec::Relay(m) => run_agent_loop(&inv, m, "relay"),
            LoopSpec::Refine(m) => run_agent_loop(&inv, m, "refine"),
            LoopSpec::Fixed(c) => Ok(run_fixed_loop(&inv, c)),
        }
    }
}

fn main() {
    let raw = get_raw_args();

    if let Some(code) = handle_meta_command(raw.first().map(String::as_str)) {
        flush_exit(code);
    }

    init::ensure_initialized();

    match run(&raw) {
        Ok(code) => flush_exit(code),
        Err(AppError::Usage(msg)) => {
            eprint!("{msg}\n\n{}", build_help_text());
            flush_exit(2);
        }
        Err(AppError::Other(msg)) => {
            eprintln!("{msg}");
            flush_exit(1);
        }
    }
}
