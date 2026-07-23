use std::collections::BTreeSet;

use crate::config::{
    get_config_dir, get_user_scene_aliases, is_initialized, load_user_scenes,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Claude,
    Codex,
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

pub fn resolve_scene_token(token: Option<&str>) -> Option<ResolvedScene> {
    let token = token?;
    if token.is_empty() {
        return None;
    }

    let engine = if token.starts_with('.') {
        Engine::Codex
    } else {
        Engine::Claude
    };
    let raw_scene = if let Engine::Codex = engine {
        &token[1..]
    } else {
        token
    };
    let scene_key = if raw_scene.is_empty() { "d" } else { raw_scene };

    // 1. config aliases (primary after init)
    if let Some(target) = get_user_scene_aliases().get(scene_key) {
        return Some(ResolvedScene {
            engine,
            scene_id: target.clone(),
        });
    }

    // 2. config scene files
    if load_user_scenes().contains_key(scene_key) {
        return Some(ResolvedScene {
            engine,
            scene_id: scene_key.to_string(),
        });
    }

    // 3. built-in fallback (only when not yet initialized)
    if !is_initialized() {
        if let Some(builtin_id) = builtin_alias(scene_key) {
            return Some(ResolvedScene {
                engine,
                scene_id: builtin_id.to_string(),
            });
        }
    }

    None
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
