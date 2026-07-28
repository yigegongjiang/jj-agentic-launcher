use crate::config::get_default_scene_id;
use crate::scenes::{resolve_scene_token, Engine};

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

    let is_loopish = |l: &LoopSpec| !matches!(l, LoopSpec::Fixed(1));

    // 0 args -> REPL with default scene
    if args.is_empty() {
        if want_print {
            return Err(AppError::Usage("`-p` requires a prompt argument.".to_string()));
        }
        if is_loopish(&loop_spec) {
            return Err(AppError::Usage(
                "`--loop` requires a prompt argument (interactive mode is not loopable)."
                    .to_string(),
            ));
        }
        return Ok(Invocation {
            engine: Engine::Claude,
            mode: Mode::Interactive,
            scene_id: get_default_scene_id(),
            user_text: None,
            user_texts: None,
            loop_spec: LoopSpec::Fixed(1),
            passthrough_args,
            pre_cmd,
        });
    }

    let resolved = resolve_scene_token(Some(&args[0]))
        .ok_or_else(|| AppError::Usage(format!("Unknown scene: \"{}\".", args[0])))?;

    // 1 arg -> REPL with given scene
    if args.len() == 1 {
        if want_print {
            return Err(AppError::Usage("`-p` requires a prompt argument.".to_string()));
        }
        if is_loopish(&loop_spec) {
            return Err(AppError::Usage(
                "`--loop` requires a prompt argument (interactive mode is not loopable)."
                    .to_string(),
            ));
        }
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
    }

    // 2 args -> scene + prompt, single-shot or loop
    let prompt = &args[1];
    if prompt.is_empty() {
        return Err(AppError::Usage("Empty prompt.".to_string()));
    }

    let segments = split_prompt(prompt);
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
