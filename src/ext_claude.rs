//! `ext` for Claude Code.
//!
//! - plugin: user `~/.claude/settings.json` `enabledPlugins` (global) / cwd
//!   `.claude/settings.local.json` `enabledPlugins` (project). `@builtin` plugins are
//!   Claude Code internals (telemetry et al.) and are never touched.
//! - skill: launcher `claude.user_skills_off` (global, see `skills.rs`) / cwd
//!   `.claude/settings.local.json` `skillOverrides` (project).
//! - mcp: the launcher runs Claude with `--strict-mcp-config`, so only the
//!   `--mcp-config` files count. The absolute one in `claude.args` is the global
//!   "on" set; `mcp-catalog.json` next to config.json keeps definitions that are
//!   switched off; the cwd `.mcp.json` is the project set.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::config::{get_config_dir, get_configured_args, DEFAULT_CONFIG_JSON};
use crate::ext::{home, Eng, FileEdit, Inventory, Item, Kind};
use crate::parse::Mode;
use crate::scenes::Engine;
use crate::skills::{claude_config_dir, skill_names};

struct Paths {
    user_settings: PathBuf,
    installed_plugins: PathBuf,
    claude_dir: PathBuf,
    claude_json: PathBuf,
    launcher_config: PathBuf,
    catalog: PathBuf,
    /// Absolute `--mcp-config` files from `claude.args`; the first is written.
    global_mcp: Vec<PathBuf>,
    strict: bool,
}

fn paths() -> Paths {
    let claude_dir = claude_config_dir();
    let claude_json = match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        Some(d) => PathBuf::from(d).join(".claude.json"),
        None => home().join(".claude.json"),
    };
    let args = get_configured_args(Engine::Claude, Mode::Interactive);
    Paths {
        user_settings: claude_dir.join("settings.json"),
        installed_plugins: claude_dir.join("plugins").join("installed_plugins.json"),
        claude_dir,
        claude_json,
        launcher_config: get_config_dir().join("config.json"),
        catalog: get_config_dir().join("mcp-catalog.json"),
        global_mcp: global_mcp_files(&args),
        strict: args.iter().any(|a| a == "--strict-mcp-config"),
    }
}

/// Absolute file sources of `--mcp-config` (variadic, or `--mcp-config=x`).
fn global_mcp_files(args: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let mut sources: Vec<&String> = Vec::new();
        if a == "--mcp-config" {
            let mut j = i + 1;
            while j < args.len() && !args[j].starts_with('-') {
                sources.push(&args[j]);
                j += 1;
            }
            i = j;
        } else {
            i += 1;
            if let Some(v) = a.strip_prefix("--mcp-config=") {
                out.extend(Some(PathBuf::from(v)).filter(|p| p.is_absolute()));
            }
            continue;
        }
        for s in sources {
            let p = PathBuf::from(s);
            if !s.trim_start().starts_with('{') && p.is_absolute() {
                out.push(p);
            }
        }
    }
    out
}

/// File text (None when absent) + its top-level object.
fn read_obj(path: &Path) -> Result<(Option<String>, Map<String, Value>), String> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok((None, Map::new()));
    };
    if text.trim().is_empty() {
        return Ok((Some(text), Map::new()));
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(m)) => Ok((Some(text), m)),
        Ok(_) => Err(format!("{}: top level is not a JSON object", path.display())),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn obj<'a>(m: &'a Map<String, Value>, key: &str) -> Option<&'a Map<String, Value>> {
    m.get(key).and_then(Value::as_object)
}

/// `m[key]` as an object, created when absent. A present non-object value is an
/// error, never silently replaced — it is the user's data.
fn obj_mut<'a>(m: &'a mut Map<String, Value>, key: &str) -> Result<&'a mut Map<String, Value>, String> {
    let v = m.entry(key.to_string()).or_insert_with(|| Value::Object(Map::new()));
    v.as_object_mut()
        .ok_or_else(|| format!("`{key}` is not a JSON object; fix it by hand first"))
}

fn servers(m: &Map<String, Value>) -> Map<String, Value> {
    obj(m, "mcpServers").cloned().unwrap_or_default()
}

fn edit(path: &Path, before: Option<String>, root: &Map<String, Value>, changes: Vec<String>) -> FileEdit {
    let mut after = serde_json::to_string_pretty(root).unwrap_or_default();
    after.push('\n');
    FileEdit {
        path: path.to_path_buf(),
        before,
        after,
        changes,
    }
}

fn label(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

/// Set `map[key] = val`, recording a change line when it differs.
fn set(map: &mut Map<String, Value>, key: &str, val: Value, what: &str, changes: &mut Vec<String>) {
    if map.get(key) != Some(&val) {
        changes.push(format!("{what}.{key} = {val}"));
        map.insert(key.to_string(), val);
    }
}

fn plugin_ids(installed: &Map<String, Value>, user_settings: &Map<String, Value>) -> Vec<String> {
    let mut ids: BTreeSet<String> = obj(installed, "plugins")
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    if let Some(m) = obj(user_settings, "enabledPlugins") {
        ids.extend(m.keys().cloned());
    }
    ids.into_iter().filter(|id| !id.ends_with("@builtin")).collect()
}

/// Project setting from `.claude/settings{,.local}.json`; local wins, as in Claude.
fn project_value(cwd: &Path, section: &str, key: &str) -> Option<Value> {
    let dir = cwd.join(".claude");
    ["settings.local.json", "settings.json"].iter().find_map(|f| {
        let (_, m) = read_obj(&dir.join(f)).ok()?;
        obj(&m, section)?.get(key).cloned()
    })
}

pub fn inventory(cwd: &Path) -> Result<Inventory, String> {
    let p = paths();
    let (_, user_settings) = read_obj(&p.user_settings)?;
    let (_, installed) = read_obj(&p.installed_plugins)?;
    let mut items = Vec::new();
    let mut notes = Vec::new();

    for id in plugin_ids(&installed, &user_settings) {
        let global = obj(&user_settings, "enabledPlugins")
            .and_then(|m| m.get(&id))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let project = project_value(cwd, "enabledPlugins", &id).and_then(|v| v.as_bool());
        items.push(Item { eng: Eng::Claude, kind: Kind::Plugin, name: id, global, project });
    }

    let skills_off = crate::config::claude_user_skills_off();
    for name in skill_names(&p.claude_dir) {
        let project = project_value(cwd, "skillOverrides", &name).map(|v| v.as_str() != Some("off"));
        items.push(Item { eng: Eng::Claude, kind: Kind::Skill, name, global: !skills_off, project });
    }

    let (_, catalog) = read_obj(&p.catalog)?;
    let (_, claude_json) = read_obj(&p.claude_json)?;
    let mut global_on: BTreeSet<String> = BTreeSet::new();
    for f in &p.global_mcp {
        global_on.extend(servers(&read_obj(f)?.1).keys().cloned());
    }
    let (_, project_mcp) = read_obj(&cwd.join(".mcp.json"))?;
    let project_on: BTreeSet<String> = servers(&project_mcp).keys().cloned().collect();
    let mut names: BTreeSet<String> = servers(&catalog).keys().cloned().collect();
    names.extend(servers(&claude_json).keys().cloned());
    names.extend(global_on.iter().cloned());
    names.extend(project_on.iter().cloned());
    if names.is_empty() {
        notes.push(format!(
            "claude mcp: no definitions yet — add them with `claude mcp add -s user ...`, into {}, or a project .mcp.json",
            p.catalog.display()
        ));
    }
    for name in names {
        let global = global_on.contains(&name);
        let project = project_on.contains(&name).then_some(true);
        items.push(Item { eng: Eng::Claude, kind: Kind::Mcp, name, global, project });
    }
    if !p.strict {
        notes.push("claude: config.json claude.args lacks --strict-mcp-config — MCP servers outside --mcp-config still load".into());
    }
    Ok(Inventory { items, notes })
}

/// Seed the `claude` section from the built-in defaults when config.json has
/// none: a section holding only `user_skills_off` would otherwise mean "no
/// args" and drop the mode flags (see `config::get_configured_args`).
fn claude_section(root: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, String> {
    if !root.get("claude").is_some_and(Value::is_object) {
        let seed = serde_json::from_str::<Value>(DEFAULT_CONFIG_JSON)
            .ok()
            .and_then(|v| v.get("claude").cloned())
            .unwrap_or_else(|| Value::Object(Map::new()));
        root.insert("claude".into(), seed);
    }
    obj_mut(root, "claude")
}

pub fn plan_global(on: bool) -> Result<(Vec<FileEdit>, Vec<String>), String> {
    let p = paths();
    let mut edits = Vec::new();
    let mut notes = Vec::new();

    // plugins
    let (before, mut settings) = read_obj(&p.user_settings)?;
    let (_, installed) = read_obj(&p.installed_plugins)?;
    let mut changes = Vec::new();
    let ids = plugin_ids(&installed, &settings);
    let ep = obj_mut(&mut settings, "enabledPlugins")?;
    for id in ids {
        set(ep, &id, Value::Bool(on), "enabledPlugins", &mut changes);
    }
    edits.push(edit(&p.user_settings, before, &settings, changes));

    // skills
    let (before, mut cfg) = read_obj(&p.launcher_config)?;
    let mut changes = Vec::new();
    set(claude_section(&mut cfg)?, "user_skills_off", Value::Bool(!on), "claude", &mut changes);
    edits.push(edit(&p.launcher_config, before, &cfg, changes));

    // mcp
    match p.global_mcp.first() {
        None => notes.push("claude mcp: no absolute --mcp-config file in config.json claude.args, skipped".into()),
        Some(target) => {
            let (gbefore, mut groot) = read_obj(target)?;
            let (cbefore, mut catalog) = read_obj(&p.catalog)?;
            let (_, claude_json) = read_obj(&p.claude_json)?;
            let mut gchanges = Vec::new();
            let mut cchanges = Vec::new();
            let mut global = servers(&groot);
            let cat = obj_mut(&mut catalog, "mcpServers")?;
            if on {
                let mut all = servers(&claude_json);
                all.extend(cat.clone());
                for (name, def) in all {
                    if !global.contains_key(&name) {
                        gchanges.push(format!("+ {name}"));
                        global.insert(name, def);
                    }
                }
            } else {
                for (name, def) in std::mem::take(&mut global) {
                    if cat.get(&name) != Some(&def) {
                        cchanges.push(format!("+ {name} (kept while off)"));
                        cat.insert(name.clone(), def);
                    }
                    gchanges.push(format!("- {name}"));
                }
            }
            groot.insert("mcpServers".into(), Value::Object(global));
            edits.push(edit(&p.catalog, cbefore, &catalog, cchanges));
            edits.push(edit(target, gbefore, &groot, gchanges));
            if p.global_mcp.len() > 1 {
                notes.push(format!("claude mcp: only {} is switched; other --mcp-config files stay as they are", target.display()));
            }
        }
    }
    if !p.strict {
        notes.push("claude: config.json claude.args lacks --strict-mcp-config — MCP servers outside --mcp-config still load".into());
    }
    Ok((edits, notes))
}

pub fn plan_project(cwd: &Path, on: bool, targets: &[(Kind, String)]) -> Result<(Vec<FileEdit>, Vec<String>), String> {
    let p = paths();
    let mut notes = Vec::new();
    // Project switches always go to settings.local.json: it outranks the shared
    // .claude/settings.json (Claude settings precedence: managed > --settings >
    // project local > shared project > user), so the switch holds whatever the
    // shared file says.
    let local_path = cwd.join(".claude").join("settings.local.json");
    let (lbefore, mut local) = read_obj(&local_path)?;
    let mut lchanges = Vec::new();
    let mcp_path = cwd.join(".mcp.json");
    let (mbefore, mut mroot) = read_obj(&mcp_path)?;
    let mut mchanges = Vec::new();
    let (cbefore, mut catalog) = read_obj(&p.catalog)?;
    let mut cchanges = Vec::new();

    let user_skills = skill_names(&p.claude_dir);
    for (kind, name) in targets {
        match kind {
            Kind::Plugin => {
                set(obj_mut(&mut local, "enabledPlugins")?, name, Value::Bool(on), "enabledPlugins", &mut lchanges);
            }
            Kind::Skill => {
                if !user_skills.contains(name) {
                    notes.push(format!("claude skill `{name}` is not under {}/skills", p.claude_dir.display()));
                }
                let v = Value::String(label(on).into());
                set(obj_mut(&mut local, "skillOverrides")?, name, v, "skillOverrides", &mut lchanges);
            }
            Kind::Mcp => {
                let project = obj_mut(&mut mroot, "mcpServers")?;
                if on {
                    if project.contains_key(name) {
                        continue;
                    }
                    let def = mcp_definition(&p, name)?
                        .ok_or_else(|| format!("claude mcp `{name}`: no definition (see `ext ls claude`)"))?;
                    project.insert(name.clone(), def);
                    mchanges.push(format!("mcpServers + {name}"));
                } else if let Some(def) = project.remove(name) {
                    mchanges.push(format!("mcpServers - {name}"));
                    let cat = obj_mut(&mut catalog, "mcpServers")?;
                    if !cat.contains_key(name) {
                        cat.insert(name.clone(), def);
                        cchanges.push(format!("+ {name} (kept while off)"));
                    }
                }
            }
        }
    }
    Ok((
        vec![
            edit(&p.catalog, cbefore, &catalog, cchanges),
            edit(&local_path, lbefore, &local, lchanges),
            edit(&mcp_path, mbefore, &mroot, mchanges),
        ],
        notes,
    ))
}

/// Definition by name: catalog, then `~/.claude.json` user scope, then the
/// global `--mcp-config` files.
fn mcp_definition(p: &Paths, name: &str) -> Result<Option<Value>, String> {
    let mut sources = vec![p.catalog.clone(), p.claude_json.clone()];
    sources.extend(p.global_mcp.iter().cloned());
    for f in sources {
        if let Some(def) = servers(&read_obj(&f)?.1).get(name) {
            return Ok(Some(def.clone()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn global_mcp_files_keeps_absolute_file_sources_only() {
        let args = s(&[
            "--x",
            "--mcp-config",
            "/abs/g.json",
            ".mcp.json",
            "{\"a\":1}",
            "--strict-mcp-config",
            "--mcp-config=/abs/h.json",
        ]);
        assert_eq!(
            global_mcp_files(&args),
            vec![PathBuf::from("/abs/g.json"), PathBuf::from("/abs/h.json")]
        );
    }

    #[test]
    fn builtin_plugins_are_not_managed() {
        let installed: Map<String, Value> =
            serde_json::from_str(r#"{"plugins":{"a@m":[]}}"#).unwrap();
        let settings: Map<String, Value> =
            serde_json::from_str(r#"{"enabledPlugins":{"b@m":false,"t@builtin":false}}"#).unwrap();
        assert_eq!(plugin_ids(&installed, &settings), s(&["a@m", "b@m"]));
    }

    #[test]
    fn claude_section_is_seeded_from_defaults() {
        let mut root = Map::new();
        claude_section(&mut root).unwrap().insert("user_skills_off".into(), Value::Bool(true));
        let c = root.get("claude").unwrap();
        assert!(c.get("args").is_some(), "seeded args keep mode flags working");
        assert_eq!(c.get("user_skills_off"), Some(&Value::Bool(true)));
    }
}
