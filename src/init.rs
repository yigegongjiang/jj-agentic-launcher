use std::fs;

use crate::config::{get_config_dir, DEFAULT_CONFIG_JSON};
use crate::scenes::BUILTIN_SCENE_TEXTS;

/// First-run setup: create the config dir, write the seed config.json, and drop
/// the built-in scenes to disk. No-op once config.json exists.
pub fn ensure_initialized() {
    let config_dir = get_config_dir();
    let config_file = config_dir.join("config.json");

    if config_file.exists() {
        return;
    }

    let scenes_dir = config_dir.join("scenes");
    if let Err(e) = fs::create_dir_all(&scenes_dir) {
        eprintln!("[warn] Failed to create config dir {}: {e}", scenes_dir.display());
        return;
    }

    if let Err(e) = fs::write(&config_file, DEFAULT_CONFIG_JSON) {
        eprintln!("[warn] Failed to write config {}: {e}", config_file.display());
        return;
    }

    for (name, text) in BUILTIN_SCENE_TEXTS {
        let scene_path = scenes_dir.join(format!("{name}.md"));
        if !scene_path.exists() {
            let _ = fs::write(&scene_path, text);
        }
    }

    eprintln!("Initialized config: {}", config_dir.display());
}
