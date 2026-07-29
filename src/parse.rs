use crate::scenes::{default_scene, resolve_scene_token, Engine, ResolvedScene};

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Interactive,
    Print,
    Stream,
}

#[derive(Clone, Copy)]
pub enum LoopSpec {
    Fixed(u32),
    Relay(u32),
    Refine(u32),
}

pub struct Invocation {
    pub engine: Engine,
    pub mode: Mode,
    pub scene_id: String,
    pub user_text: Option<String>,
    // When the original prompt contains `<<>>`, it is split into >=2 non-empty
    // segments run sequentially as independent single-shots. `user_text` holds
    // the first segment; `user_texts` is the actual trigger for serial exec.
    pub user_texts: Option<Vec<String>>,
    pub loop_spec: LoopSpec,
    // Tokens after a literal `--`, forwarded verbatim to the child engine.
    pub passthrough_args: Option<Vec<String>>,
    // `--pre <cmd>`: shell command run in the same shell session the engine is
    // then `exec`d into, so the engine inherits its cwd / env / sourced state.
    // Runs before every child spawn (loop iterations and `<<>>` steps included) —
    // shell state cannot outlive its process, so a once-only run would leave
    // rounds 2..N in the original cwd.
    pub pre_cmd: Option<String>,
}

/// Top-level error type. `Usage` prints help + exit 2; `Other` prints + exit 1.
pub enum AppError {
    Usage(String),
    Other(String),
}

pub const DEFAULT_MAX_ITER: u32 = 100;
pub const PROMPT_SEPARATOR: &str = "<<>>";

/// Split on `<<>>`, consuming whitespace adjacent to each separator — mirrors
/// the TS regex `/\s*<<>>\s*/`.
fn split_prompt(prompt: &str) -> Vec<String> {
    if !prompt.contains(PROMPT_SEPARATOR) {
        return vec![prompt.to_string()];
    }
    let pieces: Vec<&str> = prompt.split(PROMPT_SEPARATOR).collect();
    let n = pieces.len();
    pieces
        .iter()
        .enumerate()
        .map(|(i, piece)| {
            let mut s: &str = piece;
            if i > 0 {
                s = s.trim_start();
            }
            if i < n - 1 {
                s = s.trim_end();
            }
            s.to_string()
        })
        .collect()
}

/// Clip a positional argument for a one-line stderr notice (prompts can be huge).
fn truncate_for_log(text: &str) -> String {
    let one_line = text.replace('\n', " ");
    let clipped: String = one_line.chars().take(40).collect();
    if one_line.chars().count() > 40 {
        format!("{clipped}…")
    } else {
        clipped
    }
}

pub fn parse_invocation(
    args: Vec<String>,
    want_print: bool,
    loop_spec: LoopSpec,
    passthrough: Vec<String>,
    pre_cmd: Option<String>,
) -> Result<Invocation, AppError> {
    let passthrough_args = if passthrough.is_empty() {
        None
    } else {
        Some(passthrough)
    };

    if args.len() > 2 {
        return Err(AppError::Usage(
            "Too many arguments. Usage: jj-agentic-launcher [scene] [prompt]".to_string(),
        ));
    }

    // Flags that only make sense with a prompt. Their presence disambiguates a
    // lone positional argument: it can only be the prompt, never a scene.
    let needs_prompt = want_print || !matches!(loop_spec, LoopSpec::Fixed(1));

    // The scene argument is optional everywhere — omitted, empty, or a bare `.`
    // all fall back to `scenes.default` (see scenes::default_scene).
    let (resolved, prompt): (ResolvedScene, Option<String>) = match args.len() {
        0 => {
            if want_print {
                return Err(AppError::Usage("`-p` requires a prompt argument.".to_string()));
            }
            if needs_prompt {
                return Err(AppError::Usage(
                    "`--loop` requires a prompt argument (interactive mode is not loopable)."
                        .to_string(),
                ));
            }
            (default_scene().clone(), None)
        }
        1 if needs_prompt => (default_scene().clone(), Some(args[0].clone())),
        1 => match resolve_scene_token(Some(&args[0])) {
            // A known scene with no prompt -> REPL.
            Some(r) => (r, None),
            // Not a scene -> it is the prompt, run under the default scene. Say
            // so on stderr: a mistyped scene name lands here too.
            None => {
                let d = default_scene();
                eprintln!(
                    "[info] no scene named \"{}\" — treating it as the prompt, scene \"{}\".",
                    truncate_for_log(&args[0]),
                    d.scene_id
                );
                (d.clone(), Some(args[0].clone()))
            }
        },
        _ => {
            let r = resolve_scene_token(Some(&args[0]))
                .ok_or_else(|| AppError::Usage(format!("Unknown scene: \"{}\".", args[0])))?;
            (r, Some(args[1].clone()))
        }
    };

    let Some(prompt) = prompt else {
        return Ok(Invocation {
            engine: resolved.engine,
            mode: Mode::Interactive,
            scene_id: resolved.scene_id,
            user_text: None,
            user_texts: None,
            loop_spec: LoopSpec::Fixed(1),
            passthrough_args,
            pre_cmd,
        });
    };

    if prompt.is_empty() {
        return Err(AppError::Usage("Empty prompt.".to_string()));
    }

    let segments = split_prompt(&prompt);
    let is_split = segments.len() > 1;

    if is_split {
        if segments.iter().any(|s| s.is_empty()) {
            return Err(AppError::Usage(format!(
                "Empty segment between `{PROMPT_SEPARATOR}` markers. Each segment must be non-empty."
            )));
        }
        if !matches!(loop_spec, LoopSpec::Fixed(1)) {
            return Err(AppError::Usage(format!(
                "Prompt with `{PROMPT_SEPARATOR}` is split into sequential steps and cannot combine with `--loop`."
            )));
        }
    }

    Ok(Invocation {
        engine: resolved.engine,
        mode: if want_print { Mode::Print } else { Mode::Stream },
        scene_id: resolved.scene_id,
        user_text: Some(segments[0].clone()),
        user_texts: if is_split { Some(segments) } else { None },
        loop_spec,
        passthrough_args,
        pre_cmd,
    })
}
