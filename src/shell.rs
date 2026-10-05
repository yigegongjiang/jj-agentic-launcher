//! `--pre` support: run a user-supplied command inside a real interactive shell,
//! then `exec` the engine in that same process so it inherits everything the
//! command left behind — cwd, exported vars, sourced state, shell functions.
//!
//! `exec` is what keeps this honest: the shell is replaced rather than kept as a
//! parent, so the process tree stays flat and signals / exit codes pass straight
//! through to the engine.
//!
//! Everything emitted here is POSIX sh syntax; `$SHELL` must be sh/bash/zsh.

const MCP_FLAG: &str = "--mcp-config";

/// Argv placeholder for Codex project skill rules under `--pre`; expanded by the
/// script into `-c skills.config=[...]` once `pre_cmd` has settled the cwd.
pub const CODEX_SKILLS_DEFERRED: &str = "<codex-project-skills>";

/// Argv placeholder for `claude.user_skills_off` under `--pre`; expanded by the
/// script into `--settings {...}` once `pre_cmd` has settled the cwd.
pub const CLAUDE_SKILLS_DEFERRED: &str = "<claude-user-skills-off>";

/// POSIX single-quote for safe interpolation into a generated script. Inside
/// single quotes every byte is literal except `'` itself, so closing/escaping/
/// reopening around each `'` is the entire rule — newlines, `$`, backticks and
/// backslashes all pass through untouched. Deliberately *not* the `shell_quote`
/// in `preview.rs`: that one escapes control chars for display, which would
/// mangle multi-line scene text.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// `--mcp-config` accepts inline JSON as well as file paths; only the latter can
/// be existence-checked.
fn is_inline_json(s: &str) -> bool {
    s.trim_start().starts_with('{')
}

/// Relative `--mcp-config` sources whose existence this process cannot decide,
/// because the cwd is whatever `--pre` leaves behind. Surfaced in the launch
/// preview so the printed command line is not mistaken for what the engine
/// actually receives. Absolute paths are excluded — those were already checked
/// before the preview was built and cwd cannot change their verdict.
pub fn deferred_mcp_paths(args: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] != MCP_FLAG {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < args.len() && !args[j].starts_with('-') {
            let s = &args[j];
            if !is_inline_json(s) && !std::path::Path::new(s).is_absolute() {
                out.push(s.clone());
            }
            j += 1;
        }
        i = j;
    }
    out
}

/// Build the script handed to `$SHELL -i -c`.
///
/// `stdin_null` closes the engine's stdin the way the direct-spawn path does,
/// but only at `exec` time — `pre_cmd` itself keeps the inherited stdin so
/// interactive pickers (fzf and friends) can read their candidate list.
pub fn build_script(pre_cmd: &str, binary: &str, args: &[String], stdin_null: bool) -> String {
    let mut script = String::new();

    // TERM is forced to `dumb` at spawn time so shell-integration hooks loaded
    // from rc (iTerm2 et al.) stay quiet instead of emitting OSC escapes onto
    // stdout, where they would corrupt the stream-JSON pipe. rc has run by now,
    // so restore the real value: TUIs inside `pre_cmd` need a terminfo entry.
    if let Some(term) = std::env::var_os("TERM") {
        let term = term.to_string_lossy();
        if !term.is_empty() {
            script.push_str(&format!("export TERM={}\n", quote(&term)));
        }
    }

    // The pre command's stdout is redirected to stderr so its chatter never
    // lands in the engine's output stream; a non-zero exit aborts before the
    // engine starts. Newlines around `pre_cmd` (rather than `;`) keep multi-line
    // and trailing-semicolon commands working unchanged.
    script.push_str("{\n");
    script.push_str(pre_cmd);
    script.push_str("\n} 1>&2 || exit $?\n");

    // Engine argv goes through `set --` so each arg keeps exact quoting and the
    // existence of relative --mcp-config paths is decided *after* `pre_cmd` has
    // moved the cwd.
    script.push_str("set --\n");
    emit_argv(&mut script, args);

    script.push_str(&format!("exec {} \"$@\"", quote(binary)));
    if stdin_null {
        script.push_str(" </dev/null");
    }
    script.push('\n');
    script
}

fn flush_plain(script: &mut String, plain: &mut Vec<String>) {
    if plain.is_empty() {
        return;
    }
    script.push_str("set -- \"$@\"");
    for a in plain.iter() {
        script.push(' ');
        script.push_str(&quote(a));
    }
    script.push('\n');
    plain.clear();
}

fn emit_argv(script: &mut String, args: &[String]) {
    let mut plain: Vec<String> = Vec::new();
    let mut i = 0;

    while i < args.len() {
        if args[i] == CODEX_SKILLS_DEFERRED {
            flush_plain(script, &mut plain);
            emit_codex_skills(script);
            i += 1;
            continue;
        }
        if args[i] == CLAUDE_SKILLS_DEFERRED {
            flush_plain(script, &mut plain);
            emit_claude_skills(script);
            i += 1;
            continue;
        }
        if args[i] != MCP_FLAG {
            plain.push(args[i].clone());
            i += 1;
            continue;
        }

        // `--mcp-config <configs...>` is variadic — gather following non-flag
        // tokens, mirroring `sanitize_mcp_config` in run.rs.
        let mut sources: Vec<String> = Vec::new();
        let mut j = i + 1;
        while j < args.len() && !args[j].starts_with('-') {
            sources.push(args[j].clone());
            j += 1;
        }
        i = j;

        if sources.is_empty() {
            // Bare flag with nothing left to point at: drop it.
            continue;
        }

        let (inline, files): (Vec<String>, Vec<String>) =
            sources.into_iter().partition(|s| is_inline_json(s));

        if files.is_empty() {
            plain.push(MCP_FLAG.to_string());
            plain.extend(inline);
            continue;
        }

        flush_plain(script, &mut plain);
        emit_mcp_group(script, &inline, &files);
    }

    flush_plain(script, &mut plain);
}

/// Ask this binary for the project's skill rules in the post-`pre_cmd` cwd; an
/// empty answer adds nothing.
fn emit_codex_skills(script: &mut String) {
    script.push_str(&format!(
        "__jj_sk=$({} {})\n",
        quote(&self_exe()),
        quote(crate::codex_project::SUBCOMMAND)
    ));
    script.push_str("[ -z \"$__jj_sk\" ] || set -- \"$@\" '-c' \"$__jj_sk\"\n");
}

/// Same callback for Claude's `--settings` (user skills the project did not opt into).
fn emit_claude_skills(script: &mut String) {
    script.push_str(&format!(
        "__jj_cs=$({} {})\n",
        quote(&self_exe()),
        quote(crate::skills::SETTINGS_SUBCOMMAND)
    ));
    script.push_str("[ -z \"$__jj_cs\" ] || set -- \"$@\" '--settings' \"$__jj_cs\"\n");
}

fn self_exe() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| crate::meta::NAME.to_string())
}

/// Emit one `--mcp-config` group with its file sources gated on existence in the
/// post-`pre_cmd` cwd. Exactly one `--mcp-config` flag is produced per group, so
/// a repeated flag can never shadow an earlier one.
fn emit_mcp_group(script: &mut String, inline: &[String], files: &[String]) {
    if inline.is_empty() {
        script.push_str("__jj_mcp=\n");
    } else {
        script.push_str("set -- \"$@\" ");
        script.push_str(&quote(MCP_FLAG));
        for s in inline {
            script.push(' ');
            script.push_str(&quote(s));
        }
        script.push('\n');
        script.push_str("__jj_mcp=1\n");
    }

    script.push_str("for __jj_p in");
    for f in files {
        script.push(' ');
        script.push_str(&quote(f));
    }
    script.push_str("; do\n");
    script.push_str("  [ -f \"$__jj_p\" ] || continue\n");
    script.push_str(&format!(
        "  [ -n \"$__jj_mcp\" ] || {{ set -- \"$@\" {}; __jj_mcp=1; }}\n",
        quote(MCP_FLAG)
    ));
    script.push_str("  set -- \"$@\" \"$__jj_p\"\n");
    script.push_str("done\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn quote_wraps_plain_text() {
        assert_eq!(quote("hello"), "'hello'");
    }

    #[test]
    fn quote_escapes_single_quotes() {
        assert_eq!(quote("I'm here"), r"'I'\''m here'");
    }

    #[test]
    fn quote_keeps_shell_metachars_literal() {
        // Multi-line text with $, backticks and backslashes must survive as-is.
        let raw = "line1 $VAR `cmd` \\ %&*\nline2 \"quoted\"";
        assert_eq!(quote(raw), format!("'{raw}'"));
    }

    #[test]
    fn script_runs_pre_then_execs_engine() {
        let script = build_script("j api", "claude", &s(&["-p", "hello"]), true);
        assert!(script.contains("{\nj api\n} 1>&2 || exit $?\n"));
        assert!(script.contains("set --\nset -- \"$@\" '-p' 'hello'\n"));
        assert!(script.ends_with("exec 'claude' \"$@\" </dev/null\n"));
    }

    #[test]
    fn script_keeps_engine_stdin_when_interactive() {
        let script = build_script("j api", "claude", &s(&["--ide"]), false);
        assert!(script.ends_with("exec 'claude' \"$@\"\n"));
        assert!(!script.contains("/dev/null"));
    }

    #[test]
    fn relative_mcp_config_is_gated_in_the_shell() {
        let script = build_script("j api", "claude", &s(&["--mcp-config", ".mcp.json", "-p"]), true);
        assert!(script.contains("__jj_mcp=\n"));
        assert!(script.contains("for __jj_p in '.mcp.json'; do\n"));
        assert!(script.contains("[ -f \"$__jj_p\" ] || continue\n"));
        // Args after the group keep their order relative to the gated flag.
        assert!(script.contains("done\nset -- \"$@\" '-p'\n"));
    }

    #[test]
    fn inline_mcp_json_needs_no_gate() {
        let script = build_script("j api", "claude", &s(&["--mcp-config", "{\"a\":1}"]), true);
        assert!(script.contains("set -- \"$@\" '--mcp-config' '{\"a\":1}'\n"));
        assert!(!script.contains("__jj_mcp"));
    }

    #[test]
    fn mixed_inline_and_file_sources_emit_one_flag() {
        let script = build_script(
            "j api",
            "claude",
            &s(&["--mcp-config", "{\"a\":1}", ".mcp.json"]),
            true,
        );
        // Inline opens the group, so the loop must not open a second flag.
        assert!(script.contains("set -- \"$@\" '--mcp-config' '{\"a\":1}'\n__jj_mcp=1\n"));
        assert_eq!(script.matches("'--mcp-config'").count(), 2); // group opener + loop guard
        assert!(script.contains("[ -n \"$__jj_mcp\" ] ||"));
    }

    #[test]
    fn bare_mcp_config_flag_is_dropped() {
        let script = build_script("j api", "claude", &s(&["--mcp-config", "--verbose"]), true);
        assert!(!script.contains("mcp-config"));
        assert!(script.contains("set -- \"$@\" '--verbose'\n"));
    }

    #[test]
    fn deferred_paths_report_only_relative_files() {
        let args = s(&[
            "--mcp-config",
            ".mcp.json",
            "/abs/ok.json",
            "{\"a\":1}",
            "-p",
        ]);
        assert_eq!(deferred_mcp_paths(&args), s(&[".mcp.json"]));
    }

    #[test]
    fn deferred_paths_empty_without_mcp_config() {
        assert!(deferred_mcp_paths(&s(&["-p", "hello"])).is_empty());
    }

    #[test]
    fn codex_skills_placeholder_resolves_in_the_shell() {
        let script = build_script(
            ".. j api",
            "codex",
            &s(&["--json", CODEX_SKILLS_DEFERRED, "-c", "x=1"]),
            true,
        );
        assert!(script.contains("set -- \"$@\" '--json'\n__jj_sk=$("));
        assert!(script.contains("'__codex-project-skills')\n"));
        assert!(script.contains("[ -z \"$__jj_sk\" ] || set -- \"$@\" '-c' \"$__jj_sk\"\nset -- \"$@\" '-c' 'x=1'\n"));
        assert!(!script.contains(CODEX_SKILLS_DEFERRED));
    }

    #[test]
    fn claude_skills_placeholder_resolves_in_the_shell() {
        let script = build_script(
            "j api",
            "claude",
            &s(&["-p", CLAUDE_SKILLS_DEFERRED, "--append-system-prompt", "S"]),
            true,
        );
        assert!(script.contains("set -- \"$@\" '-p'\n__jj_cs=$("));
        assert!(script.contains("'__claude-user-skills-settings')\n"));
        assert!(script.contains("[ -z \"$__jj_cs\" ] || set -- \"$@\" '--settings' \"$__jj_cs\"\nset -- \"$@\" '--append-system-prompt' 'S'\n"));
        assert!(!script.contains(CLAUDE_SKILLS_DEFERRED));
    }

    #[test]
    fn multiline_pre_command_survives() {
        let script = build_script("cd ..\nls", "claude", &s(&["-p"]), true);
        assert!(script.contains("{\ncd ..\nls\n} 1>&2 || exit $?\n"));
    }
}
