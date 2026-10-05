//! `claude.user_skills_off`: hide every user-level skill (`~/.claude/skills/*`)
//! unless the project opts it back in.
//!
//! Claude Code has no "user skills off, project decides" switch. The pieces it
//! does have (verified against claude 2.1.289, 2026-10-05):
//! - `skillOverrides: {"<name>": "off"}` hides a skill; plugin skills ignore it.
//! - `--settings` outranks project and local settings, so a blanket `off` passed
//!   there could never be re-enabled by the project.
//! - Project settings are read from the cwd only, not from a parent / git root.
//!
//! Hence the list is computed per launch: every user skill, minus any name the
//! cwd's `.claude/settings{,.local}.json` already lists in `skillOverrides` (the
//! project's own state wins) and minus names of the cwd's own `.claude/skills`
//! (an `off` would hide the project skill too). Enumerated live, so newly
//! installed user skills are covered without touching config.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// Hidden subcommand used by the `--pre` script: the cwd is only known after
/// the pre command ran, so the script calls back into this binary there.
pub const SETTINGS_SUBCOMMAND: &str = "__claude-user-skills-settings";

pub(crate) fn claude_config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".claude")
}

/// `name:` from the SKILL.md frontmatter (what `skillOverrides` matches), else
/// the directory name.
fn skill_name(skill_md: &str, dir_name: &str) -> String {
    let mut lines = skill_md.lines();
    if lines.next().map(str::trim) == Some("---") {
        for line in lines {
            let line = line.trim();
            if line == "---" {
                break;
            }
            if let Some(v) = line.strip_prefix("name:") {
                let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
                if !v.is_empty() {
                    return v.to_string();
                }
            }
        }
    }
    dir_name.to_string()
}

/// Skill names under `<root>/skills/*/SKILL.md`. `fs::read_to_string` follows
/// symlinks, which is how `npx skills` installs them.
pub(crate) fn skill_names(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root.join("skills")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let text = fs::read_to_string(e.path().join("SKILL.md")).ok()?;
            Some(skill_name(&text, &e.file_name().to_string_lossy()))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn override_keys(settings_file: &Path) -> Vec<String> {
    fs::read_to_string(settings_file)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("skillOverrides")?.as_object().map(|m| m.keys().cloned().collect()))
        .unwrap_or_default()
}

fn off_list(user_root: &Path, project_dir: &Path) -> Vec<String> {
    let project_claude = project_dir.join(".claude");
    let mut keep = skill_names(&project_claude);
    keep.extend(override_keys(&project_claude.join("settings.json")));
    keep.extend(override_keys(&project_claude.join("settings.local.json")));
    skill_names(user_root)
        .into_iter()
        .filter(|n| !keep.contains(n))
        .collect()
}

fn settings_for(user_root: &Path, project_dir: &Path) -> Option<String> {
    let off = off_list(user_root, project_dir);
    if off.is_empty() {
        return None;
    }
    let map: Map<String, Value> = off
        .into_iter()
        .map(|n| (n, Value::String("off".to_string())))
        .collect();
    let mut root = Map::new();
    root.insert("skillOverrides".to_string(), Value::Object(map));
    Some(Value::Object(root).to_string())
}

/// `--settings` JSON for the current cwd, or `None` when nothing needs hiding.
pub fn settings_json() -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    settings_for(&claude_config_dir(), &cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("jj-skills-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn add_skill(root: &Path, dir: &str, md: &str) {
        let d = root.join("skills").join(dir);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("SKILL.md"), md).unwrap();
    }

    #[test]
    fn frontmatter_name_wins_over_dir_name() {
        assert_eq!(skill_name("---\nname: \"real\"\n---\nbody", "dir"), "real");
        assert_eq!(skill_name("no frontmatter\nname: x", "dir"), "dir");
        assert_eq!(skill_name("---\ndescription: d\n---\nname: x", "dir"), "dir");
    }

    #[test]
    fn project_overrides_and_project_skills_are_left_alone() {
        let base = tmp("off");
        let user = base.join("user");
        let proj = base.join("proj");
        add_skill(&user, "a", "---\nname: a\n---");
        add_skill(&user, "b", "---\nname: b\n---");
        add_skill(&user, "c", "---\nname: c\n---");
        add_skill(&user, "d", "---\nname: d\n---");
        fs::create_dir_all(user.join("skills").join("plugin-dir-no-skill-md")).unwrap();
        add_skill(&proj.join(".claude"), "c", "---\nname: c\n---");
        fs::write(
            proj.join(".claude").join("settings.json"),
            r#"{"skillOverrides":{"a":"on"}}"#,
        )
        .unwrap();
        fs::write(
            proj.join(".claude").join("settings.local.json"),
            r#"{"skillOverrides":{"b":"name-only"}}"#,
        )
        .unwrap();

        assert_eq!(off_list(&user, &proj), vec!["d".to_string()]);
        assert_eq!(
            settings_for(&user, &proj).as_deref(),
            Some(r#"{"skillOverrides":{"d":"off"}}"#)
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn nothing_to_hide_yields_none() {
        let base = tmp("none");
        assert_eq!(settings_for(&base.join("user"), &base), None);
        let _ = fs::remove_dir_all(&base);
    }
}
