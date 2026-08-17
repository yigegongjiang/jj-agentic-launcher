use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::parse::Mode;
use crate::scenes::Engine;

pub fn get_config_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    home.join(".config").join("jj-agentic-launcher")
}

// --- Types ---

#[derive(Deserialize, Default)]
pub struct EngineConfig {
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub interactive: Option<Vec<String>>,
    #[serde(default)]
    pub print: Option<Vec<String>>,
    #[serde(default)]
    pub stream: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
pub struct ScenesConfig {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub aliases: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Default)]
pub struct UserConfig {
    #[serde(default)]
    pub claude: Option<EngineConfig>,
    #[serde(default)]
    pub codex: Option<EngineConfig>,
    #[serde(default)]
    pub agy: Option<EngineConfig>,
    #[serde(default)]
    pub scenes: Option<ScenesConfig>,
}

/// Seed config written verbatim on first run, and the fallback for any engine
/// section a pre-existing config.json does not have yet (see
/// [`get_configured_args`]).
pub const DEFAULT_CONFIG_JSON: &str = r#"{
  "claude": {
    "args": [
      "--dangerously-skip-permissions",
      "--allow-dangerously-skip-permissions"
    ],
    "interactive": [
      "--ide"
    ],
    "print": [
      "-p"
    ],
    "stream": [
      "--output-format",
      "stream-json",
      "--verbose",
      "--include-partial-messages"
    ]
  },
  "codex": {
    "args": [
      "--dangerously-bypass-approvals-and-sandbox",
      "-c",
      "web_search=\"live\""
    ],
    "interactive": [],
    "print": [],
    "stream": [
      "--json"
    ]
  },
  "agy": {
    "args": [
      "--dangerously-skip-permissions"
    ],
    "interactive": [],
    "print": [
      "--print-timeout",
      "24h"
    ],
    "stream": [
      "--output-format",
      "stream-json"
    ]
  },
  "scenes": {
    "default": "default",
    "aliases": {
      "d": "default",
      "ai": "ai-expert",
      "code": "code-expert",
      "it": "it-expert"
    }
  }
}
"#;

// --- Cached state ---

fn config() -> &'static UserConfig {
    static CONFIG: OnceLock<UserConfig> = OnceLock::new();
    CONFIG.get_or_init(load_config_uncached)
}

fn load_config_uncached() -> UserConfig {
    let config_file = get_config_dir().join("config.json");
    if !config_file.exists() {
        return UserConfig::default();
    }
    match fs::read_to_string(&config_file) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
            eprintln!(
                "[warn] Failed to parse config {}, using defaults.",
                config_file.display()
            );
            UserConfig::default()
        }),
        Err(_) => UserConfig::default(),
    }
}

/// Seed defaults, parsed from [`DEFAULT_CONFIG_JSON`] on first use.
fn seed_config() -> &'static UserConfig {
    static SEED: OnceLock<UserConfig> = OnceLock::new();
    SEED.get_or_init(|| serde_json::from_str(DEFAULT_CONFIG_JSON).unwrap_or_default())
}

fn engine_section(cfg: &UserConfig, engine: Engine) -> Option<&EngineConfig> {
    match engine {
        Engine::Claude => cfg.claude.as_ref(),
        Engine::Codex => cfg.codex.as_ref(),
        Engine::Agy => cfg.agy.as_ref(),
    }
}

/// Whether the config dir has been initialized (config.json present). Uncached
/// so it reflects on-disk state before and after `ensure_initialized`.
pub fn is_initialized() -> bool {
    get_config_dir().join("config.json").exists()
}

/// Merged args with progressive inheritance:
///   interactive -> args + interactive
///   print       -> args + print
///   stream      -> args + print + stream
///
/// A config.json with no section at all for this engine — every config written
/// before the engine was supported — falls back to the seed defaults: launching
/// with zero arguments would silently drop the mode flags and give a REPL where
/// a single-shot was asked for. A present-but-empty section (`"agy": {}`) still
/// means "no arguments".
pub fn get_configured_args(engine: Engine, mode: Mode) -> Vec<String> {
    let Some(ec) = engine_section(config(), engine).or_else(|| engine_section(seed_config(), engine))
    else {
        return Vec::new();
    };

    let global = ec.args.clone().unwrap_or_default();
    let mode_args = match mode {
        Mode::Interactive => ec.interactive.clone().unwrap_or_default(),
        Mode::Print => ec.print.clone().unwrap_or_default(),
        Mode::Stream => ec.stream.clone().unwrap_or_default(),
    };

    let mut out = global;
    if let Mode::Stream = mode {
        out.extend(ec.print.clone().unwrap_or_default());
    }
    out.extend(mode_args);
    out
}

/// Raw `scenes.default` value — a scene *token* (alias / scene file name /
/// optional `.` prefix for Codex), resolved by `scenes::default_scene`.
pub fn get_default_scene_token() -> String {
    config()
        .scenes
        .as_ref()
        .and_then(|s| s.default.clone())
        .unwrap_or_else(|| "default".to_string())
}

pub fn get_user_scene_aliases() -> HashMap<String, String> {
    config()
        .scenes
        .as_ref()
        .and_then(|s| s.aliases.clone())
        .unwrap_or_default()
}

pub fn load_user_scenes() -> &'static HashMap<String, String> {
    static SCENES: OnceLock<HashMap<String, String>> = OnceLock::new();
    SCENES.get_or_init(|| {
        let mut map = HashMap::new();
        let scenes_dir = get_config_dir().join("scenes");
        if let Ok(entries) = fs::read_dir(&scenes_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                if let (Some(stem), Ok(text)) =
                    (path.file_stem().and_then(|s| s.to_str()), fs::read_to_string(&path))
                {
                    map.insert(stem.to_string(), text);
                }
            }
        }
        map
    })
}
