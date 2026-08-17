use std::collections::BTreeSet;
use std::sync::OnceLock;

use crate::config::{
    get_config_dir, get_default_scene_token, get_user_scene_aliases, is_initialized,
    load_user_scenes,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Claude,
    Codex,
    /// Antigravity CLI (`agy`, Google Gemini).
    Agy,
}

// Built-in scene texts, embedded at compile time. Used as seed data by init and
// as a fallback before the config dir is initialized.
pub const BUILTIN_SCENE_TEXTS: &[(&str, &str)] = &[
    ("address", include_str!("../scenes/address.md")),
    ("ai-expert", include_str!("../scenes/ai-expert.md")),
    ("code-expert", include_str!("../scenes/code-expert.md")),
    ("default", include_str!("../scenes/default.md")),
    ("it-expert", include_str!("../scenes/it-expert.md")),
];

// Built-in aliases — fallback only when the config dir has not been initialized.
const BUILTIN_ALIASES: &[(&str, &str)] = &[
    ("address", "address"),
    ("ai", "ai-expert"),
    ("ai-expert", "ai-expert"),
    ("code", "code-expert"),
    ("code-expert", "code-expert"),
    ("d", "default"),
    ("default", "default"),
    ("it", "it-expert"),
    ("it-expert", "it-expert"),
];

// Last-resort scene id when `scenes.default` is missing or unusable. A broken
// config value must never make the launcher unusable.
const FALLBACK_SCENE_ID: &str = "default";

#[derive(Clone)]
pub struct ResolvedScene {
    pub engine: Engine,
    pub scene_id: String,
}

fn builtin_scene_text(id: &str) -> Option<&'static str> {
    BUILTIN_SCENE_TEXTS
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, text)| *text)
}

fn builtin_alias(key: &str) -> Option<&'static str> {
    BUILTIN_ALIASES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, target)| *target)
}

/// Engine prefix on a scene token: `.` selects Codex, `,` selects agy
/// (Antigravity CLI); anything else runs on Claude Code. Both prefixes are
/// single ASCII punctuation with no meaning to sh/bash/zsh, so they never need
/// quoting on the command line.
fn split_engine_prefix(token: &str) -> (Engine, &str) {
    if let Some(rest) = token.strip_prefix('.') {
        (Engine::Codex, rest)
    } else if let Some(rest) = token.strip_prefix(',') {
        (Engine::Agy, rest)
    } else {
        (Engine::Claude, token)
    }
}

/// Map a non-empty scene key (no engine prefix) to a scene id.
fn resolve_scene_id(scene_key: &str) -> Option<String> {
    // 1. config aliases (primary after init)
    if let Some(target) = get_user_scene_aliases().get(scene_key) {
        return Some(target.clone());
    }

    // 2. config scene files
    if load_user_scenes().contains_key(scene_key) {
        return Some(scene_key.to_string());
    }

    // 3. built-in fallback (only when not yet initialized)
    if !is_initialized() {
        if let Some(builtin_id) = builtin_alias(scene_key) {
            return Some(builtin_id.to_string());
        }
    }

    None
}

/// The configured default scene (`scenes.default` in config.json), used whenever
/// the scene argument is omitted. Its value is a scene token in the same form as
/// the CLI argument: alias (`it`), scene file name (`it-expert`), or a `.` prefix
/// to make Codex the default engine (`.it`).
///
/// Resolved once per process. An empty / unknown value warns and falls back to
/// the built-in `default` scene instead of failing the launch.
pub fn default_scene() -> &'static ResolvedScene {
    static DEFAULT: OnceLock<ResolvedScene> = OnceLock::new();
    DEFAULT.get_or_init(|| {
        let token = get_default_scene_token();
        let token = token.trim();
        let (engine, scene_key) = split_engine_prefix(token);

        if scene_key.is_empty() {
            if !token.is_empty() {
                eprintln!(
                    "[warn] scenes.default = \"{token}\" names no scene; using \"{FALLBACK_SCENE_ID}\"."
                );
            }
            return ResolvedScene {
                engine,
                scene_id: FALLBACK_SCENE_ID.to_string(),
            };
        }

        match resolve_scene_id(scene_key) {
            Some(scene_id) => ResolvedScene { engine, scene_id },
            None => {
                eprintln!(
                    "[warn] scenes.default = \"{token}\" is not a known scene or alias; using \"{FALLBACK_SCENE_ID}\"."
                );
                ResolvedScene {
                    engine,
                    scene_id: FALLBACK_SCENE_ID.to_string(),
                }
            }
        }
    })
}

/// Resolve a CLI scene token. `None` / `""` mean "omitted" — engine and scene
/// both come from [`default_scene`]. A bare `.` pins Codex while keeping the
/// default scene.
pub fn resolve_scene_token(token: Option<&str>) -> Option<ResolvedScene> {
    let Some(token) = token else {
        return Some(default_scene().clone());
    };
    let token = token.trim();
    if token.is_empty() {
        return Some(default_scene().clone());
    }

    let (engine, scene_key) = split_engine_prefix(token);
    if scene_key.is_empty() {
        return Some(ResolvedScene {
            engine,
            scene_id: default_scene().scene_id.clone(),
        });
    }

    resolve_scene_id(scene_key).map(|scene_id| ResolvedScene { engine, scene_id })
}

pub fn get_scene_text(scene_id: &str) -> Result<String, String> {
    // 1. config scene files (primary)
    if let Some(text) = load_user_scenes().get(scene_id) {
        return Ok(text.clone());
    }

    // 2. built-in fallback (only when not yet initialized)
    if !is_initialized() {
        if let Some(text) = builtin_scene_text(scene_id) {
            return Ok(text.to_string());
        }
    }

    Err(format!(
        "Scene \"{scene_id}\" not found. Check {}/scenes/ directory.",
        get_config_dir().display()
    ))
}

pub fn list_all_scene_names() -> Vec<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();

    if is_initialized() {
        for key in get_user_scene_aliases().keys() {
            names.insert(key.clone());
        }
        for key in load_user_scenes().keys() {
            names.insert(key.clone());
        }
    } else {
        for (key, _) in BUILTIN_ALIASES {
            names.insert((*key).to_string());
        }
    }

    names.into_iter().collect()
}
