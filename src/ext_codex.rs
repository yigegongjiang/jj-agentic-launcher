//! `ext` for Codex.
//!
//! - mcp: `[mcp_servers.<name>] enabled` in `$CODEX_HOME/config.toml` (global) /
//!   cwd `.codex/config.toml` (project; merged onto the global definition, so a
//!   project can only switch servers the global config defines).
//! - plugin: `[plugins."<id>"] enabled`, local plugins only. Remote
//!   (`@openai-curated-remote`) plugins follow account state on the server; the
//!   `features.remote_plugin` kill switch is left to the user.
//! - skill: global `[[skills.config]] path = … enabled`; project
//!   `[[skills.config]] name = … enabled`, which Codex ignores at project level
//!   and the launcher forwards as `-c` (see `codex_project.rs`).
//!
//! The skill / plugin inventory comes from Codex itself (`codex app-server`
//! `skills/list` + `plugin/list`) instead of re-implementing its discovery.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use toml_edit::{value, ArrayOfTables, DocumentMut, Item as TItem, Table};

use crate::codex_project::{codex_home, project_dirs};
use crate::ext::{home, Eng, FileEdit, Inventory, Item, Kind};

const RPC_TIMEOUT: Duration = Duration::from_secs(60);

fn user_config() -> Result<PathBuf, String> {
    codex_home()
        .map(|h| h.join("config.toml"))
        .ok_or_else(|| "cannot locate CODEX_HOME".to_string())
}

fn project_config(cwd: &Path) -> PathBuf {
    cwd.join(".codex").join("config.toml")
}

fn read_doc(path: &Path) -> Result<(Option<String>, DocumentMut), String> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok((None, DocumentMut::new()));
    };
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((Some(text), doc))
}

// --- app-server ---

/// Run requests against a throwaway `codex app-server` and return their
/// results in order. cwd is `$HOME` and no `-c` is passed, so the answer is the
/// user-level state, untouched by whatever project the caller stands in.
fn rpc(requests: &[(&str, Value)]) -> Result<Vec<Value>, String> {
    let mut child = Command::new("codex")
        .arg("app-server")
        .current_dir(home())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to start `codex app-server`: {e}"))?;
    let result = rpc_session(&mut child, requests);
    let _ = child.kill();
    let _ = child.wait();
    result
}

fn rpc_session(child: &mut std::process::Child, requests: &[(&str, Value)]) -> Result<Vec<Value>, String> {
    let mut stdin = child.stdin.take().ok_or("app-server stdin")?;
    let stdout = child.stdout.take().ok_or("app-server stdout")?;
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut msgs = vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"jj-agentic-launcher","version":crate::meta::VERSION}}}),
        json!({"jsonrpc":"2.0","method":"initialized"}),
    ];
    for (i, (method, params)) in requests.iter().enumerate() {
        msgs.push(json!({"jsonrpc":"2.0","id":i + 2,"method":method,"params":params}));
    }
    for m in msgs {
        writeln!(stdin, "{m}").map_err(|e| format!("app-server write: {e}"))?;
    }
    stdin.flush().map_err(|e| format!("app-server write: {e}"))?;

    let mut results: BTreeMap<u64, Value> = BTreeMap::new();
    let deadline = Instant::now() + RPC_TIMEOUT;
    while results.len() < requests.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        let line = rx
            .recv_timeout(left)
            .map_err(|_| "`codex app-server` did not answer in time".to_string())?;
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = v.get("id").and_then(Value::as_u64) else {
            continue; // notification
        };
        if id < 2 {
            continue;
        }
        if let Some(err) = v.get("error") {
            return Err(format!("app-server {}: {err}", requests[(id - 2) as usize].0));
        }
        results.insert(id, v.get("result").cloned().unwrap_or(Value::Null));
    }
    Ok(results.into_values().collect())
}

struct Skill {
    name: String,
    path: String,
    enabled: bool,
}

struct Plugin {
    id: String,
    enabled: bool,
}

/// User-level skills (no repo / plugin skills) and installed local plugins.
fn query_codex() -> Result<(Vec<Skill>, Vec<Plugin>), String> {
    let home = home().to_string_lossy().into_owned();
    let res = rpc(&[
        ("skills/list", json!({"cwds":[home],"forceReload":true})),
        ("plugin/list", json!({})),
    ])?;
    Ok((parse_skills(&res[0]), parse_plugins(&res[1])))
}

fn parse_skills(v: &Value) -> Vec<Skill> {
    let mut out: Vec<Skill> = Vec::new();
    for entry in v["data"].as_array().into_iter().flatten() {
        for s in entry["skills"].as_array().into_iter().flatten() {
            if s["scope"] == "repo" || !s["pluginId"].is_null() {
                continue;
            }
            let (Some(name), Some(path)) = (s["name"].as_str(), s["path"].as_str()) else {
                continue;
            };
            if out.iter().any(|x| x.path == path) {
                continue;
            }
            out.push(Skill {
                name: name.into(),
                path: path.into(),
                enabled: s["enabled"].as_bool().unwrap_or(true),
            });
        }
    }
    out
}

fn parse_plugins(v: &Value) -> Vec<Plugin> {
    let mut out: Vec<Plugin> = Vec::new();
    for m in v["marketplaces"].as_array().into_iter().flatten() {
        for p in m["plugins"].as_array().into_iter().flatten() {
            // Remote plugins follow account state on the server, not config.
            if p["installed"] != true || !p["remotePluginId"].is_null() {
                continue;
            }
            let Some(id) = p["id"].as_str() else { continue };
            if out.iter().any(|x| x.id == id) {
                continue;
            }
            out.push(Plugin {
                id: id.into(),
                enabled: p["enabled"].as_bool().unwrap_or(false),
            });
        }
    }
    out
}

// --- TOML helpers ---

fn mcp_names(doc: &DocumentMut) -> Vec<(String, bool)> {
    doc.get("mcp_servers")
        .and_then(TItem::as_table_like)
        .map(|t| {
            t.iter()
                .map(|(k, v)| {
                    let on = v.get("enabled").and_then(TItem::as_bool).unwrap_or(true);
                    (k.to_string(), on)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `doc[parent][name].enabled = on`, creating `[parent.name]` as needed with
/// `parent` kept implicit (no bare `[plugins]` header).
fn set_enabled(doc: &mut DocumentMut, parent: &str, name: &str, on: bool, changes: &mut Vec<String>) {
    let cur = doc
        .get(parent)
        .and_then(|p| p.get(name))
        .and_then(|t| t.get("enabled"))
        .and_then(TItem::as_bool);
    if cur == Some(on) {
        return;
    }
    if !doc.contains_key(parent) {
        let mut t = Table::new();
        t.set_implicit(true);
        doc.insert(parent, TItem::Table(t));
    }
    let Some(p) = doc[parent].as_table_like_mut() else {
        return;
    };
    if p.get(name).is_none() {
        p.insert(name, TItem::Table(Table::new()));
    }
    if let Some(t) = p.get_mut(name).and_then(TItem::as_table_like_mut) {
        t.insert("enabled", value(on));
        changes.push(format!("{parent}.\"{name}\".enabled = {on}"));
    }
}

fn skills_aot(doc: &mut DocumentMut) -> Option<&mut ArrayOfTables> {
    if !doc.contains_key("skills") {
        let mut t = Table::new();
        t.set_implicit(true);
        doc.insert("skills", TItem::Table(t));
    }
    let skills = doc["skills"].as_table_like_mut()?;
    if skills.get("config").is_none() {
        skills.insert("config", TItem::ArrayOfTables(ArrayOfTables::new()));
    }
    skills.get_mut("config")?.as_array_of_tables_mut()
}

fn canon(p: &str) -> PathBuf {
    let pb = PathBuf::from(p);
    pb.canonicalize().unwrap_or(pb)
}

/// Global skill switch: every rule matching the skill (by canonical path or by
/// name) gets `enabled = on`; a skill with no rule gets a new path rule.
fn set_skill_global(aot: &mut ArrayOfTables, skill: &Skill, on: bool, changes: &mut Vec<String>) {
    let target = canon(&skill.path);
    let mut matched = false;
    for t in aot.iter_mut() {
        let by_path = t.get("path").and_then(TItem::as_str).is_some_and(|p| canon(p) == target);
        let by_name = t.get("name").and_then(TItem::as_str) == Some(skill.name.as_str());
        if !(by_path || by_name) {
            continue;
        }
        matched = true;
        if t.get("enabled").and_then(TItem::as_bool) != Some(on) {
            t.insert("enabled", value(on));
            changes.push(format!("skills.config {} = {on}", skill.name));
        }
    }
    if !matched {
        let mut t = Table::new();
        t.insert("path", value(skill.path.clone()));
        t.insert("enabled", value(on));
        aot.push(t);
        changes.push(format!("skills.config + {} ({}) = {on}", skill.name, skill.path));
    }
}

/// Project skill switch: one name rule per skill.
fn set_skill_project(aot: &mut ArrayOfTables, name: &str, on: bool, changes: &mut Vec<String>) {
    let mut matched = false;
    for t in aot.iter_mut() {
        if t.get("name").and_then(TItem::as_str) != Some(name) {
            continue;
        }
        matched = true;
        if t.get("enabled").and_then(TItem::as_bool) != Some(on) {
            t.insert("enabled", value(on));
            changes.push(format!("skills.config {name} = {on}"));
        }
    }
    if !matched {
        let mut t = Table::new();
        t.insert("name", value(name));
        t.insert("enabled", value(on));
        aot.push(t);
        changes.push(format!("skills.config + {name} = {on}"));
    }
}

fn doc_edit(path: &Path, before: Option<String>, doc: &DocumentMut, changes: Vec<String>) -> FileEdit {
    FileEdit {
        path: path.to_path_buf(),
        before,
        after: doc.to_string(),
        changes,
    }
}

/// Project state from the cwd's `.codex/config.toml`.
struct ProjectState {
    mcp: BTreeMap<String, bool>,
    plugins: BTreeMap<String, bool>,
    skills: BTreeMap<String, bool>,
}

fn project_state(cwd: &Path) -> Result<ProjectState, String> {
    let (_, doc) = read_doc(&project_config(cwd))?;
    let mcp = mcp_names(&doc).into_iter().collect();
    let plugins = doc
        .get("plugins")
        .and_then(TItem::as_table_like)
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| Some((k.to_string(), v.get("enabled")?.as_bool()?)))
                .collect()
        })
        .unwrap_or_default();
    let mut skills = BTreeMap::new();
    if let Some(aot) = doc
        .get("skills")
        .and_then(|s| s.get("config"))
        .and_then(TItem::as_array_of_tables)
    {
        for t in aot.iter() {
            if let (Some(n), Some(e)) = (
                t.get("name").and_then(TItem::as_str),
                t.get("enabled").and_then(TItem::as_bool),
            ) {
                skills.insert(n.to_string(), e);
            }
        }
    }
    Ok(ProjectState { mcp, plugins, skills })
}

pub fn inventory(cwd: &Path) -> Result<Inventory, String> {
    let (_, user) = read_doc(&user_config()?)?;
    let (skills, plugins) = query_codex()?;
    let ps = project_state(cwd)?;
    let mut items = Vec::new();
    for (name, global) in mcp_names(&user) {
        let project = ps.mcp.get(&name).copied();
        items.push(Item { eng: Eng::Codex, kind: Kind::Mcp, name, global, project });
    }
    for s in skills {
        let project = ps.skills.get(&s.name).copied();
        items.push(Item { eng: Eng::Codex, kind: Kind::Skill, name: s.name, global: s.enabled, project });
    }
    for p in plugins {
        let project = ps.plugins.get(&p.id).copied();
        items.push(Item { eng: Eng::Codex, kind: Kind::Plugin, name: p.id, global: p.enabled, project });
    }
    let mut notes = Vec::new();
    notes.extend(trust_note(cwd, &user));
    Ok(Inventory { items, notes })
}

pub fn plan_global(on: bool) -> Result<(Vec<FileEdit>, Vec<String>), String> {
    let path = user_config()?;
    let (skills, plugins) = query_codex()?;
    let (before, mut doc) = read_doc(&path)?;
    Ok((vec![plan_global_doc(&path, before, &mut doc, &skills, &plugins, on)], Vec::new()))
}

fn plan_global_doc(
    path: &Path,
    before: Option<String>,
    doc: &mut DocumentMut,
    skills: &[Skill],
    plugins: &[Plugin],
    on: bool,
) -> FileEdit {
    let mut changes = Vec::new();
    for (name, _) in mcp_names(doc) {
        set_enabled(doc, "mcp_servers", &name, on, &mut changes);
    }
    for p in plugins {
        set_enabled(doc, "plugins", &p.id, on, &mut changes);
    }
    if !skills.is_empty() {
        if let Some(aot) = skills_aot(doc) {
            for s in skills {
                set_skill_global(aot, s, on, &mut changes);
            }
        }
    }
    doc_edit(path, before, doc, changes)
}

pub fn plan_project(cwd: &Path, on: bool, targets: &[(Kind, String)]) -> Result<(Vec<FileEdit>, Vec<String>), String> {
    let (_, user) = read_doc(&user_config()?)?;
    let path = project_config(cwd);
    let (before, mut doc) = read_doc(&path)?;
    let defined: Vec<String> = mcp_names(&user).into_iter().map(|(n, _)| n).collect();
    let mut changes = Vec::new();
    let mut notes = Vec::new();
    for (kind, name) in targets {
        match kind {
            Kind::Mcp => {
                // An enabled-only project table has no transport of its own; without
                // a global definition to merge onto it is a config error for Codex.
                if !defined.contains(name) {
                    return Err(format!(
                        "codex mcp `{name}` is not defined in {}",
                        user_config()?.display()
                    ));
                }
                set_enabled(&mut doc, "mcp_servers", name, on, &mut changes);
            }
            Kind::Plugin => set_enabled(&mut doc, "plugins", name, on, &mut changes),
            Kind::Skill => {
                if let Some(aot) = skills_aot(&mut doc) {
                    set_skill_project(aot, name, on, &mut changes);
                }
            }
        }
    }
    if targets.iter().any(|(k, _)| *k != Kind::Skill) {
        notes.extend(trust_note(cwd, &user));
    }
    Ok((vec![doc_edit(&path, before, &doc, changes)], notes))
}

/// Codex ignores a project's `.codex/config.toml` (mcp / plugin) unless the
/// project is trusted; skills still work because the launcher forwards them.
fn trust_note(cwd: &Path, user: &DocumentMut) -> Option<String> {
    let root = project_dirs(cwd).into_iter().next().unwrap_or_else(|| cwd.to_path_buf());
    let trusted = |p: &Path| {
        user.get("projects")
            .and_then(|t| t.get(p.to_string_lossy().as_ref()))
            .and_then(|t| t.get("trust_level"))
            .and_then(TItem::as_str)
            == Some("trusted")
    };
    if trusted(cwd) || trusted(&root) {
        return None;
    }
    Some(format!(
        "codex: {} is not trusted — Codex skips its .codex/config.toml mcp/plugin switches (trust it once in Codex)",
        root.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(name: &str, path: &str) -> Skill {
        Skill { name: name.into(), path: path.into(), enabled: true }
    }

    #[test]
    fn global_off_touches_mcp_plugins_and_skills_only() {
        let src = r#"# keep me
model = "x"

[features]
remote_plugin = false

[mcp_servers.docs]
url = "https://d"

[plugins."a@m"]
enabled = true

[[skills.config]]
path = "/s/one/SKILL.md"
enabled = true
"#;
        let mut doc: DocumentMut = src.parse().unwrap();
        let skills = vec![skill("one", "/s/one/SKILL.md"), skill("two", "/s/two/SKILL.md")];
        let plugins = vec![Plugin { id: "a@m".into(), enabled: true }, Plugin { id: "b@m".into(), enabled: true }];
        let e = plan_global_doc(Path::new("/x"), Some(src.into()), &mut doc, &skills, &plugins, false);
        let out = e.after;
        assert!(out.starts_with("# keep me\nmodel = \"x\""));
        assert!(out.contains("remote_plugin = false"));
        let v: toml::Table = out.parse().unwrap();
        assert_eq!(v["mcp_servers"]["docs"]["enabled"].as_bool(), Some(false));
        assert_eq!(v["mcp_servers"]["docs"]["url"].as_str(), Some("https://d"));
        assert_eq!(v["plugins"]["a@m"]["enabled"].as_bool(), Some(false));
        assert_eq!(v["plugins"]["b@m"]["enabled"].as_bool(), Some(false));
        let rules = v["skills"]["config"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert!(rules.iter().all(|r| r["enabled"].as_bool() == Some(false)));
        assert!(!out.contains("\n[plugins]\n"), "parent table stays implicit");
        assert_eq!(e.changes.len(), 5);
    }

    #[test]
    fn global_is_idempotent() {
        let mut doc: DocumentMut = "[mcp_servers.docs]\nurl = \"u\"\nenabled = false\n".parse().unwrap();
        let e = plan_global_doc(Path::new("/x"), None, &mut doc, &[], &[], false);
        assert!(e.changes.is_empty());
    }

    #[test]
    fn project_skill_rule_is_by_name_and_updated_in_place() {
        let mut doc: DocumentMut = "[[skills.config]]\nname = \"h\"\nenabled = false\n".parse().unwrap();
        let mut changes = Vec::new();
        set_skill_project(skills_aot(&mut doc).unwrap(), "h", true, &mut changes);
        set_skill_project(skills_aot(&mut doc).unwrap(), "k", true, &mut changes);
        let v: toml::Table = doc.to_string().parse().unwrap();
        let rules = v["skills"]["config"].as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0]["enabled"].as_bool(), Some(true));
        assert_eq!(rules[1]["name"].as_str(), Some("k"));
    }

    #[test]
    fn inventory_parsers_drop_repo_plugin_and_remote_entries() {
        let s = json!({"data":[{"skills":[
            {"name":"a","path":"/a","scope":"user","enabled":false,"pluginId":null},
            {"name":"r","path":"/r","scope":"repo","enabled":true,"pluginId":null},
            {"name":"p","path":"/p","scope":"user","enabled":true,"pluginId":"x@y"}
        ]}]});
        let got = parse_skills(&s);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "a");
        let p = json!({"marketplaces":[{"plugins":[
            {"id":"l@m","installed":true,"enabled":false,"source":{"type":"local","path":"/l"}},
            {"id":"n@m","installed":false,"enabled":false,"source":{"type":"local","path":"/n"}},
            {"id":"r@remote","installed":true,"enabled":true,"remotePluginId":"x","source":{"type":"remote","id":"x"}}
        ]}]});
        let got = parse_plugins(&p);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "l@m");
    }
}
