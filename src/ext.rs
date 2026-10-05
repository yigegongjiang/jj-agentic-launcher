//! `ext`: switch Claude Code / Codex MCP servers, skills and plugins from one
//! place — everything at once in the user (global) config, or one by one in the
//! cwd's project config.
//!
//! Every subcommand is plan-then-apply: each engine module turns the current
//! file text into new text plus a human-readable change list, so `--dry-run`
//! and the tests never touch disk.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::meta::NAME;
use crate::{ext_claude, ext_codex};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Eng {
    Claude,
    Codex,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Mcp,
    Skill,
    Plugin,
}

impl Eng {
    fn label(self) -> &'static str {
        match self {
            Eng::Claude => "claude",
            Eng::Codex => "codex",
        }
    }
    fn parse(s: &str) -> Option<Eng> {
        match s {
            "claude" => Some(Eng::Claude),
            "codex" => Some(Eng::Codex),
            _ => None,
        }
    }
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Mcp => "mcp",
            Kind::Skill => "skill",
            Kind::Plugin => "plugin",
        }
    }
    fn parse(s: &str) -> Option<Kind> {
        match s {
            "mcp" => Some(Kind::Mcp),
            "skill" => Some(Kind::Skill),
            "plugin" => Some(Kind::Plugin),
            _ => None,
        }
    }
}

/// One switchable thing and where it currently stands.
pub struct Item {
    pub eng: Eng,
    pub kind: Kind,
    pub name: String,
    pub global: bool,
    /// Explicit project (cwd) setting; `None` = the project says nothing.
    pub project: Option<bool>,
}

/// A planned rewrite of one file. `before` is what the plan was computed from;
/// apply refuses to write if the file changed in between.
pub struct FileEdit {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: String,
    pub changes: Vec<String>,
}

pub struct Inventory {
    pub items: Vec<Item>,
    pub notes: Vec<String>,
}

const USAGE: &str = "Usage:
  {NAME} ext [ls] [claude|codex]                      List MCP / skill / plugin state (global + this project)
  {NAME} ext global on|off [claude|codex] [--dry-run] Switch everything in the user config at once
  {NAME} ext on|off [--dry-run]                       Pick items with fzf, switch them for this project (cwd)
  {NAME} ext on|off <claude|codex> <mcp|skill|plugin> <name>... [--dry-run]

Project switches are written to the cwd: .claude/settings.json + .mcp.json (Claude),
.codex/config.toml (Codex). Global writes keep <file>.jj-orig (first write) + <file>.jj-bak (last).";

pub fn run(args: &[String]) -> i32 {
    match dispatch(args) {
        Ok(()) => 0,
        Err(msg) => {
            eprintln!("error: {msg}");
            1
        }
    }
}

fn usage() -> String {
    USAGE.replace("{NAME}", NAME)
}

fn dispatch(args: &[String]) -> Result<(), String> {
    let dry = args.iter().any(|a| a == "--dry-run" || a == "-n");
    let rest: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| *a != "--dry-run" && *a != "-n")
        .collect();
    let cwd = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;

    match rest.as_slice() {
        [] | ["ls"] => list(&cwd, &[Eng::Claude, Eng::Codex]),
        ["ls", e] => list(&cwd, &[parse_eng(e)?]),
        ["help"] | ["-h"] | ["--help"] => {
            println!("{}", usage());
            Ok(())
        }
        ["global", sw, engs @ ..] => {
            let on = parse_switch(sw)?;
            let engs = parse_engs(engs)?;
            let mut edits = Vec::new();
            for e in engs {
                let (mut ed, notes) = match e {
                    Eng::Claude => ext_claude::plan_global(on)?,
                    Eng::Codex => ext_codex::plan_global(on)?,
                };
                print_notes(&notes);
                edits.append(&mut ed);
            }
            apply(edits, dry, true)
        }
        [sw] => {
            let on = parse_switch(sw)?;
            let picked = pick(&cwd, on)?;
            switch_project(&cwd, on, picked, dry)
        }
        [sw, e, k, names @ ..] if !names.is_empty() => {
            let on = parse_switch(sw)?;
            let eng = parse_eng(e)?;
            let kind = Kind::parse(k).ok_or_else(|| format!("unknown kind `{k}` (mcp|skill|plugin)"))?;
            let picked = names.iter().map(|n| (eng, kind, n.to_string())).collect();
            switch_project(&cwd, on, picked, dry)
        }
        _ => Err(format!("bad arguments\n\n{}", usage())),
    }
}

fn parse_switch(s: &str) -> Result<bool, String> {
    match s {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(format!("expected on|off, got `{s}`\n\n{}", usage())),
    }
}

fn parse_eng(s: &str) -> Result<Eng, String> {
    Eng::parse(s).ok_or_else(|| format!("unknown engine `{s}` (claude|codex)"))
}

fn parse_engs(v: &[&str]) -> Result<Vec<Eng>, String> {
    if v.is_empty() {
        return Ok(vec![Eng::Claude, Eng::Codex]);
    }
    v.iter().map(|e| parse_eng(e)).collect()
}

fn inventory(cwd: &Path, eng: Eng) -> Result<Inventory, String> {
    match eng {
        Eng::Claude => ext_claude::inventory(cwd),
        Eng::Codex => ext_codex::inventory(cwd),
    }
}

fn state(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

fn row(it: &Item, name_w: usize) -> String {
    let project = it.project.map(state).unwrap_or("-");
    let effective = state(it.project.unwrap_or(it.global));
    format!(
        "{:<6}  {:<6}  {:<name_w$}  global={:<3}  project={:<3}  => {}",
        it.eng.label(),
        it.kind.label(),
        it.name,
        state(it.global),
        project,
        effective
    )
}

fn list(cwd: &Path, engs: &[Eng]) -> Result<(), String> {
    let mut notes = Vec::new();
    let mut items = Vec::new();
    for e in engs {
        let inv = inventory(cwd, *e)?;
        items.extend(inv.items);
        notes.extend(inv.notes);
    }
    let w = items.iter().map(|i| i.name.len()).max().unwrap_or(4);
    println!("project: {}", cwd.display());
    for it in &items {
        println!("{}", row(it, w));
    }
    print_notes(&notes);
    Ok(())
}

fn print_notes(notes: &[String]) {
    for n in notes {
        eprintln!("[note] {n}");
    }
}

/// fzf multi-select over both engines' inventories.
fn pick(cwd: &Path, on: bool) -> Result<Vec<(Eng, Kind, String)>, String> {
    let mut items = Vec::new();
    let mut notes = Vec::new();
    for e in [Eng::Claude, Eng::Codex] {
        let inv = inventory(cwd, e)?;
        items.extend(inv.items);
        notes.extend(inv.notes);
    }
    print_notes(&notes);
    if items.is_empty() {
        return Err("nothing to pick".into());
    }
    let w = items.iter().map(|i| i.name.len()).max().unwrap_or(4);
    let input: String = items.iter().map(|i| row(i, w) + "\n").collect();

    let prompt = format!("project {} > ", if on { "ON" } else { "OFF" });
    let mut child = Command::new("fzf")
        .args(["-m", "--no-sort", "--prompt", &prompt, "--header", "TAB = select, ENTER = apply"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("fzf not available ({e}); pass targets: ext on|off <engine> <kind> <name>..."))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| format!("fzf: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut picked = Vec::new();
    for line in text.lines() {
        let mut f = line.split_whitespace();
        let (Some(e), Some(k), Some(n)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        if let (Some(e), Some(k)) = (Eng::parse(e), Kind::parse(k)) {
            picked.push((e, k, n.to_string()));
        }
    }
    if picked.is_empty() {
        return Err("nothing selected".into());
    }
    Ok(picked)
}

fn switch_project(
    cwd: &Path,
    on: bool,
    picked: Vec<(Eng, Kind, String)>,
    dry: bool,
) -> Result<(), String> {
    let mut edits: Vec<FileEdit> = Vec::new();
    for eng in [Eng::Claude, Eng::Codex] {
        let targets: Vec<(Kind, String)> = picked
            .iter()
            .filter(|(e, _, _)| *e == eng)
            .map(|(_, k, n)| (*k, n.clone()))
            .collect();
        if targets.is_empty() {
            continue;
        }
        let (mut ed, notes) = match eng {
            Eng::Claude => ext_claude::plan_project(cwd, on, &targets)?,
            Eng::Codex => ext_codex::plan_project(cwd, on, &targets)?,
        };
        print_notes(&notes);
        edits.append(&mut ed);
    }
    apply(edits, dry, false)
}

/// Write planned edits. Global (`backup`) writes first keep the file as it was
/// before `ext` ever touched it (`.jj-orig`, written once) and as it was before
/// this write (`.jj-bak`) — two files at most, so nothing piles up over time.
fn apply(edits: Vec<FileEdit>, dry: bool, backup: bool) -> Result<(), String> {
    let edits: Vec<FileEdit> = edits.into_iter().filter(|e| !e.changes.is_empty()).collect();
    if edits.is_empty() {
        println!("nothing to change");
        return Ok(());
    }
    for e in &edits {
        println!("{}{}", if dry { "[dry-run] " } else { "" }, e.path.display());
        for c in &e.changes {
            println!("  {c}");
        }
    }
    if dry {
        return Ok(());
    }
    for e in &edits {
        let now = fs::read_to_string(&e.path).ok();
        if now != e.before {
            return Err(format!(
                "{} changed while planning; nothing after it was written, re-run",
                e.path.display()
            ));
        }
        if backup {
            if let Some(cur) = &now {
                let orig = suffixed(&e.path, ".jj-orig");
                if !orig.exists() {
                    fs::write(&orig, cur).map_err(|x| format!("{}: {x}", orig.display()))?;
                }
                let bak = suffixed(&e.path, ".jj-bak");
                fs::write(&bak, cur).map_err(|x| format!("{}: {x}", bak.display()))?;
            }
        }
        write_atomic(&e.path, &e.after)?;
    }
    Ok(())
}

fn suffixed(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Temp file + rename, so a reader (the engine itself) never sees half a file.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = suffixed(path, &format!(".jj-tmp-{}", std::process::id()));
    fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_refuses_when_file_moved_on() {
        let d = std::env::temp_dir().join(format!("jj-ext-apply-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let p = d.join("f.json");
        fs::write(&p, "new").unwrap();
        let edit = FileEdit {
            path: p.clone(),
            before: Some("old".into()),
            after: "x".into(),
            changes: vec!["c".into()],
        };
        assert!(apply(vec![edit], false, true).is_err());
        assert_eq!(fs::read_to_string(&p).unwrap(), "new");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn global_apply_keeps_orig_once_and_bak_each_time() {
        let d = std::env::temp_dir().join(format!("jj-ext-bak-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let p = d.join("c.toml");
        fs::write(&p, "v1").unwrap();
        for (before, after) in [("v1", "v2"), ("v2", "v3")] {
            let e = FileEdit {
                path: p.clone(),
                before: Some(before.into()),
                after: after.into(),
                changes: vec!["c".into()],
            };
            apply(vec![e], false, true).unwrap();
        }
        assert_eq!(fs::read_to_string(&p).unwrap(), "v3");
        assert_eq!(fs::read_to_string(suffixed(&p, ".jj-orig")).unwrap(), "v1");
        assert_eq!(fs::read_to_string(suffixed(&p, ".jj-bak")).unwrap(), "v2");
        let _ = fs::remove_dir_all(&d);
    }
}
