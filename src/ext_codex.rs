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
/// `parent` kept implicit (no bare `[plugins]` header). Works on dotted keys and
/// inline tables alike; a non-table value there is the user's data and an error.
fn set_enabled(doc: &mut DocumentMut, parent: &str, name: &str, on: bool, changes: &mut Vec<String>) -> Result<(), String> {
    if !doc.contains_key(parent) {
        let mut t = Table::new();
        t.set_implicit(true);
        doc.insert(parent, TItem::Table(t));
    }
    let p = doc[parent]
        .as_table_like_mut()
        .ok_or_else(|| format!("`{parent}` is not a table; fix it by hand first"))?;
    if p.get(name).is_none() {
        p.insert(name, TItem::Table(Table::new()));
    }
    let t = p
        .get_mut(name)
        .and_then(TItem::as_table_like_mut)
        .ok_or_else(|| format!("`{parent}.{name}` is not a table; fix it by hand first"))?;
    if t.get("enabled").and_then(TItem::as_bool) != Some(on) {
        t.insert("enabled", value(on));
        changes.push(format!("{parent}.\"{name}\".enabled = {on}"));
    }
    Ok(())
}

/// `skills.config`, located (and created as `[[skills.config]]` when absent)
/// without changing how the user wrote it: a standard array of tables, or an
/// inline `config = [{ ... }]` (possibly inside `skills = { ... }`). Converting
/// the inline form would drop it — an array of tables cannot live inside an
/// inline table — so both forms are edited where they are.
fn skills_rules(doc: &mut DocumentMut) -> Result<&mut TItem, String> {
    if !doc.contains_key("skills") {
        let mut t = Table::new();
        t.set_implicit(true);
        doc.insert("skills", TItem::Table(t));
    }
    let inline = doc["skills"].is_inline_table();
    let skills = doc["skills"]
        .as_table_like_mut()
        .ok_or("`skills` is not a table; fix it by hand first")?;
    if skills.get("config").is_none() {
        if inline || skills.is_dotted() {
            skills.insert("config", TItem::Value(toml_edit::Value::Array(toml_edit::Array::new())));
        } else {
            skills.insert("config", TItem::ArrayOfTables(ArrayOfTables::new()));
        }
    }
    let item = skills.get_mut("config").ok_or("skills.config")?;
    let ok = item.is_array_of_tables()
        || item.as_array().is_some_and(|a| a.iter().all(|v| v.is_inline_table()));
    if !ok {
        return Err("`skills.config` is not a list of tables; fix it by hand first".into());
    }
    Ok(item)
}

/// Every rule in `skills.config`, whichever form it is written in.
fn rules_mut(item: &mut TItem) -> Vec<&mut dyn toml_edit::TableLike> {
    match item {
        TItem::ArrayOfTables(aot) => aot.iter_mut().map(|t| t as &mut dyn toml_edit::TableLike).collect(),
        TItem::Value(toml_edit::Value::Array(arr)) => arr
            .iter_mut()
            .filter_map(|v| v.as_inline_table_mut())
            .map(|t| t as &mut dyn toml_edit::TableLike)
            .collect(),
        _ => Vec::new(),
    }
}

fn push_rule(item: &mut TItem, key: &str, val: &str, on: bool) {
    match item {
        TItem::ArrayOfTables(aot) => {
            let mut t = Table::new();
            t.insert(key, value(val));
            t.insert("enabled", value(on));
            aot.push(t);
        }
        TItem::Value(toml_edit::Value::Array(arr)) => {
            let mut t = toml_edit::InlineTable::new();
            t.insert(key, val.into());
            t.insert("enabled", on.into());
            arr.push(t);
        }
        _ => {}
    }
}

fn canon(p: &str) -> PathBuf {
    let pb = PathBuf::from(p);
    pb.canonicalize().unwrap_or(pb)
}

/// Global skill switch: every rule matching the skill (by canonical path or by
/// name) gets `enabled = on`; a skill with no rule gets a new *name* rule. A
/// path rule would go stale when the skill moves — symlinked skills resolve to
/// versioned app paths (`.../ego lite.app/.../0.5.1.13/...`) — and the skill
/// would silently come back on after an update.
fn set_skill_global(item: &mut TItem, skill: &Skill, on: bool, changes: &mut Vec<String>) {
    let target = canon(&skill.path);
    let mut matched = false;
    for t in rules_mut(item) {
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
        push_rule(item, "name", &skill.name, on);
        changes.push(format!("skills.config + {} = {on}", skill.name));
    }
}

/// Project skill switch: one name rule per skill.
fn set_skill_project(item: &mut TItem, name: &str, on: bool, changes: &mut Vec<String>) {
    let mut matched = false;
    for t in rules_mut(item) {
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
        push_rule(item, "name", name, on);
        changes.push(format!("skills.config + {name} = {on}"));
    }
}

/// Safety net for every TOML plan: apart from `enabled` flags and appended
/// list entries, the new text must hold exactly the old data. A formatting
/// corner case in the editor can then never drop or alter user config.
fn preserves(before: &toml::Value, after: &toml::Value) -> bool {
    use toml::Value as V;
    match (before, after) {
        (V::Table(b), V::Table(a)) => b
            .iter()
            .all(|(k, bv)| k == "enabled" || a.get(k).is_some_and(|av| preserves(bv, av))),
        (V::Array(b), V::Array(a)) => a.len() >= b.len() && b.iter().zip(a).all(|(x, y)| preserves(x, y)),
        _ => before == after,
    }
}

fn checked(path: &Path, before: &Option<String>, after: &str) -> Result<(), String> {
    let parse = |t: &str| t.parse::<toml::Table>().map(toml::Value::Table);
    let old = parse(before.as_deref().unwrap_or("")).map_err(|e| format!("{}: {e}", path.display()))?;
    let new = parse(after).map_err(|e| format!("{}: edit produced invalid TOML ({e}); nothing written", path.display()))?;
    if !preserves(&old, &new) {
        return Err(format!("{}: edit would change data beyond `enabled`; nothing written", path.display()));
    }
    Ok(())
}

fn doc_edit(path: &Path, before: Option<String>, doc: &DocumentMut, changes: Vec<String>) -> Result<FileEdit, String> {
    let after = doc.to_string();
    checked(path, &before, &after)?;
    Ok(FileEdit {
        path: path.to_path_buf(),
        before,
        after,
        changes,
    })
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
    Ok((vec![plan_global_doc(&path, before, &mut doc, &skills, &plugins, on)?], Vec::new()))
}

fn plan_global_doc(
    path: &Path,
    before: Option<String>,
    doc: &mut DocumentMut,
    skills: &[Skill],
    plugins: &[Plugin],
    on: bool,
) -> Result<FileEdit, String> {
    let mut changes = Vec::new();
    for (name, _) in mcp_names(doc) {
        set_enabled(doc, "mcp_servers", &name, on, &mut changes)?;
    }
    for p in plugins {
        set_enabled(doc, "plugins", &p.id, on, &mut changes)?;
    }
    if !skills.is_empty() {
        let rules = skills_rules(doc)?;
        for s in skills {
            set_skill_global(rules, s, on, &mut changes);
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
                set_enabled(&mut doc, "mcp_servers", name, on, &mut changes)?;
            }
            Kind::Plugin => set_enabled(&mut doc, "plugins", name, on, &mut changes)?,
            Kind::Skill => set_skill_project(skills_rules(&mut doc)?, name, on, &mut changes),
        }
    }
    if targets.iter().any(|(k, _)| *k != Kind::Skill) {
        notes.extend(trust_note(cwd, &user));
    }
    Ok((vec![doc_edit(&path, before, &doc, changes)?], notes))
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
        let e = plan_global_doc(Path::new("/x"), Some(src.into()), &mut doc, &skills, &plugins, false).unwrap();
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
        assert_eq!(rules[1]["name"].as_str(), Some("two"), "new global rules select by name");
        assert!(!out.contains("\n[plugins]\n"), "parent table stays implicit");
        assert_eq!(e.changes.len(), 5);
    }

    #[test]
    fn inline_forms_are_edited_not_skipped() {
        let src = "mcp_servers = { docs = { url = \"u\" } }\nskills = { config = [{ name = \"a\", enabled = true }] }\nplugins.\"p@m\".enabled = true\n";
        let mut doc: DocumentMut = src.parse().unwrap();
        let skills = vec![skill("a", "/s/a/SKILL.md")];
        let plugins = vec![Plugin { id: "p@m".into(), enabled: true }];
        let e = plan_global_doc(Path::new("/x"), None, &mut doc, &skills, &plugins, false).unwrap();
        let v: toml::Table = e.after.parse().unwrap();
        assert_eq!(v["mcp_servers"]["docs"]["enabled"].as_bool(), Some(false));
        assert_eq!(v["mcp_servers"]["docs"]["url"].as_str(), Some("u"));
        assert_eq!(v["skills"]["config"][0]["enabled"].as_bool(), Some(false));
        assert_eq!(v["skills"]["config"][0]["name"].as_str(), Some("a"));
        assert_eq!(v["plugins"]["p@m"]["enabled"].as_bool(), Some(false));
        assert_eq!(e.changes.len(), 3);
        assert!(e.after.starts_with("mcp_servers = { docs = {"), "inline layout kept");
    }

    #[test]
    fn non_table_values_are_an_error_not_overwritten() {
        let mut doc: DocumentMut = "plugins = \"oops\"\n".parse().unwrap();
        assert!(set_enabled(&mut doc, "plugins", "p@m", false, &mut Vec::new()).is_err());
        assert_eq!(doc.to_string(), "plugins = \"oops\"\n");
    }

    #[test]
    fn guard_rejects_data_loss() {
        let before = Some("a = 1\n[t]\nx = \"k\"\nenabled = true\n".to_string());
        assert!(checked(Path::new("/x"), &before, "a = 1\n[t]\nx = \"k\"\nenabled = false\n").is_ok());
        assert!(checked(Path::new("/x"), &before, "a = 1\n[t]\nenabled = false\n").is_err());
        assert!(checked(Path::new("/x"), &before, "a = 2\n[t]\nx = \"k\"\n").is_err());
    }

    #[test]
    fn global_is_idempotent() {
        let mut doc: DocumentMut = "[mcp_servers.docs]\nurl = \"u\"\nenabled = false\n".parse().unwrap();
        let e = plan_global_doc(Path::new("/x"), None, &mut doc, &[], &[], false).unwrap();
        assert!(e.changes.is_empty());
    }

    #[test]
    fn project_skill_rule_is_by_name_and_updated_in_place() {
        let mut doc: DocumentMut = "[[skills.config]]\nname = \"h\"\nenabled = false\n".parse().unwrap();
        let mut changes = Vec::new();
        set_skill_project(skills_rules(&mut doc).unwrap(), "h", true, &mut changes);
        set_skill_project(skills_rules(&mut doc).unwrap(), "k", true, &mut changes);
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
