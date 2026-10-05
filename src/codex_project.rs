//! Forward project-level `[[skills.config]]` rules to Codex as a `-c` override.
//!
//! Codex already honors a trusted project's `.codex/config.toml` for
//! `[plugins.*]` / `[mcp_servers.*]` / `[skills.bundled]`, but skill toggles are
//! read from the user and session-flag layers only — project entries are
//! silently dropped (`skill_config_rules_from_stack` in
//! codex-rs/config/src/skills_config.rs, verified against codex-cli 0.160.0).
//! Re-sending them as `-c skills.config=[...]` lands them in the session-flag
//! layer, where they stack on top of the user's global rules.
//!
//! Only `skills.config` is forwarded. Re-sending the rest of the file would put
//! repository content above Codex's trust gate and project-config denylist
//! (`notify`, `model_provider`, base URLs, …); skill toggles only switch skills
//! already present on this machine.

use std::path::{Path, PathBuf};

use toml::{Table, Value};

/// Hidden subcommand used by `--pre`: the cwd is only known after the pre
/// command ran, so the generated script calls back into this binary.
pub const SUBCOMMAND: &str = "__codex-project-skills";

/// `skills.config=[...]` for the project around `cwd`, or `None` when no project
/// config carries skill rules.
pub fn skills_override(cwd: &Path) -> Option<String> {
    let codex_home = codex_home();
    let mut rules: Vec<Value> = Vec::new();
    for dir in project_dirs(cwd) {
        let dot_codex = dir.join(".codex");
        // `~/.codex` is the user layer, not a project layer.
        if codex_home.as_deref() == Some(dot_codex.as_path()) {
            continue;
        }
        let file = dot_codex.join("config.toml");
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let table: Table = match text.parse() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[warn] skip {}: {e}", file.display());
                continue;
            }
        };
        rules.extend(skill_rules(&table, &dot_codex));
    }
    if rules.is_empty() {
        return None;
    }
    Some(format!("skills.config={}", Value::Array(rules)))
}

/// Directories from the project root down to `cwd`, root first — the order
/// Codex stacks project layers in, so a deeper `.codex/` wins on conflicts.
/// The root is the nearest ancestor holding a `.git` marker (Codex's default
/// `project_root_markers`); without one only `cwd` itself is a project layer.
pub(crate) fn project_dirs(cwd: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for dir in cwd.ancestors() {
        dirs.push(dir.to_path_buf());
        if is_git_root(dir) {
            dirs.reverse();
            return dirs;
        }
    }
    vec![cwd.to_path_buf()]
}

/// `.git` file (worktree / submodule) or a `.git` dir that has `HEAD`.
fn is_git_root(dir: &Path) -> bool {
    let git = dir.join(".git");
    git.is_file() || (git.is_dir() && git.join("HEAD").exists())
}

pub(crate) fn codex_home() -> Option<PathBuf> {
    match std::env::var_os("CODEX_HOME").filter(|v| !v.is_empty()) {
        Some(v) => Some(PathBuf::from(v)),
        None => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex")),
    }
}

/// `[[skills.config]]` entries with `path` made absolute: Codex resolves a
/// project-layer relative path against that `.codex/` folder, but a `-c` value
/// would be resolved against the cwd instead.
fn skill_rules(table: &Table, dot_codex: &Path) -> Vec<Value> {
    let Some(entries) = table
        .get("skills")
        .and_then(|s| s.get("config"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(Value::as_table)
        .map(|entry| {
            let mut entry = entry.clone();
            if let Some(path) = entry.get("path").and_then(Value::as_str) {
                let abs = absolutize(path, dot_codex);
                entry.insert("path".into(), Value::String(abs));
            }
            Value::Table(entry)
        })
        .collect()
}

fn absolutize(path: &str, base: &Path) -> String {
    let p = match path.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(rest),
            None => PathBuf::from(path),
        },
        None => PathBuf::from(path),
    };
    let p = if p.is_absolute() { p } else { base.join(p) };
    p.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("jj-codex-project-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Parse an override back the way `codex -c` does (key = TOML value), so
    /// assertions do not depend on inline-table key order.
    fn parse(over: &str) -> Value {
        let (_, v) = over.split_once('=').unwrap();
        format!("v = {v}").parse::<Table>().unwrap().remove("v").unwrap()
    }

    fn write(dir: &Path, body: &str) {
        fs::create_dir_all(dir.join(".codex")).unwrap();
        fs::write(dir.join(".codex/config.toml"), body).unwrap();
    }

    #[test]
    fn forwards_skill_rules_only() {
        let root = tmp("only");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        write(
            &root,
            r#"
[[skills.config]]
name = "handoff"
enabled = true

[plugins."x@y"]
enabled = true
"#,
        );
        let got = parse(&skills_override(&root).unwrap());
        assert_eq!(got, parse(r#"skills.config=[{ name = "handoff", enabled = true }]"#));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn root_layer_comes_before_nested_and_paths_are_absolute() {
        let root = tmp("nested");
        fs::write(root.join(".git"), "gitdir: /elsewhere\n").unwrap();
        let sub = root.join("pkg");
        write(&root, "[[skills.config]]\npath = \"skills/a/SKILL.md\"\nenabled = false\n");
        write(&sub, "[[skills.config]]\npath = \"/abs/a b/SKILL.md\"\nenabled = true\n");
        let got = parse(&skills_override(&sub).unwrap());
        let rel = root.join(".codex/skills/a/SKILL.md");
        assert_eq!(
            got,
            parse(&format!(
                r#"skills.config=[{{ path = "{}", enabled = false }}, {{ path = "/abs/a b/SKILL.md", enabled = true }}]"#,
                rel.display()
            ))
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn without_git_only_cwd_is_a_layer() {
        let root = tmp("nogit");
        let sub = root.join("sub");
        write(&root, "[[skills.config]]\nname = \"outer\"\nenabled = true\n");
        fs::create_dir_all(&sub).unwrap();
        // The outer `.codex/` sits above a cwd with no project root marker.
        assert_eq!(project_dirs(&sub), vec![sub.clone()]);
        assert!(skills_override(&sub).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn no_rules_means_no_override() {
        let root = tmp("none");
        write(&root, "[mcp_servers.a]\nurl = \"https://x\"\n");
        assert!(skills_override(&root).is_none());
        let _ = fs::remove_dir_all(&root);
    }
}
