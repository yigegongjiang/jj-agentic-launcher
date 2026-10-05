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
  {NAME} ext on|off [--dry-run]                       Multi-pick with fzf (TAB), switch them for this project (cwd)
  {NAME} ext on|off <claude|codex> <mcp|skill|plugin> <name>... [--dry-run]

Project switches are written to the cwd: .claude/settings.local.json + .mcp.json (Claude),
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
        // Default fzf marks a pick with a thin bar next to the gutter, easy to miss;
        // a coloured check + Ctrl-A make multi-select obvious. Long-standing flags
        // only: an unknown one makes fzf exit 2 with empty output.
        .args([
            "-m",
            "--no-sort",
            "--marker=✓",
            "--color=marker:green:bold",
            "--bind=ctrl-a:toggle-all",
            "--prompt",
            &prompt,
            "--header",
            "TAB / Shift-TAB = toggle, Ctrl-A = toggle all shown, ENTER = apply",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("fzf not available ({e}); pass targets: ext on|off <engine> <kind> <name>..."))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| format!("fzf: {e}"))?;
    if out.status.code() == Some(2) {
        return Err("fzf failed (exit 2)".into());
    }
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

/// Write planned edits. Every file is checked against the text its plan was
/// computed from before anything is written, so a concurrent change (Claude /
/// Codex rewriting their own config) aborts the whole run instead of being
/// overwritten. Global (`backup`) writes keep the file as it was before `ext`
/// first touched it (`.jj-orig`, written once) and as it was before this write
/// (`.jj-bak`) — two files at most, so nothing piles up over time.
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
        if fs::read_to_string(&e.path).ok() != e.before {
            return Err(format!("{} changed while planning; nothing written, re-run", e.path.display()));
        }
    }
    for e in &edits {
        let target = real_path(&e.path);
        if backup && e.before.is_some() {
            let orig = suffixed(&target, ".jj-orig");
            if !orig.exists() {
                copy_file(&target, &orig)?;
            }
            copy_file(&target, &suffixed(&target, ".jj-bak"))?;
        }
        write_atomic(&target, &e.after)?;
    }
    Ok(())
}

/// Follow a symlinked config (dotfiles managers link `~/.codex/config.toml` et
/// al.) so the rename replaces the real file, not the link.
fn real_path(path: &Path) -> PathBuf {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

/// `fs::copy` keeps the permission bits, so a backup is never more readable
/// than the config it copies (these files can hold tokens).
fn copy_file(from: &Path, to: &Path) -> Result<(), String> {
    fs::copy(from, to).map(|_| ()).map_err(|e| format!("{}: {e}", to.display()))
}

fn suffixed(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Exclusive temp file in the target dir + fsync + rename, so a reader (the
/// engine itself) never sees half a file and a crash never leaves one. The
/// temp file takes the target's permission bits before it replaces it.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write as _;

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let tmp = suffixed(path, &format!(".jj-tmp-{}-{nanos}", std::process::id()));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        if let Ok(meta) = fs::metadata(path) {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        fs::rename(&tmp, path)
    })();
    result.map_err(|e| {
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
    fn write_goes_through_symlink_and_keeps_mode() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let d = std::env::temp_dir().join(format!("jj-ext-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let real = d.join("real.toml");
        let link = d.join("link.toml");
        fs::write(&real, "a").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&real, &link).unwrap();
        let e = FileEdit {
            path: link.clone(),
            before: Some("a".into()),
            after: "b".into(),
            changes: vec!["c".into()],
        };
        apply(vec![e], false, true).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "b");
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&real), 0o600);
        assert_eq!(mode(&suffixed(&real, ".jj-bak")), 0o600);
        assert!(fs::read_dir(&d).unwrap().all(|x| !x.unwrap().file_name().to_string_lossy().contains("jj-tmp")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn stale_file_aborts_before_any_write() {
        let d = std::env::temp_dir().join(format!("jj-ext-stale-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let (a, b) = (d.join("a"), d.join("b"));
        fs::write(&a, "a1").unwrap();
        fs::write(&b, "moved").unwrap();
        let ed = |p: &Path, before: &str| FileEdit {
            path: p.to_path_buf(),
            before: Some(before.into()),
            after: "new".into(),
            changes: vec!["c".into()],
        };
        assert!(apply(vec![ed(&a, "a1"), ed(&b, "b1")], false, false).is_err());
        assert_eq!(fs::read_to_string(&a).unwrap(), "a1", "first file untouched");
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
