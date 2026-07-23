use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::meta::{NAME, REPO, VERSION};

fn detect_asset() -> Result<String, String> {
    if std::env::consts::OS != "macos" {
        return Err(format!(
            "unsupported OS: {} (only darwin is supported)",
            std::env::consts::OS
        ));
    }
    // Map Rust arch names to the release asset suffixes (contract with the
    // release workflow, install.sh, and prior versions — must not change).
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => return Err(format!("unsupported arch: {other}")),
    };
    Ok(format!("{NAME}-darwin-{arch}"))
}

fn current_exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

fn exe_basename(dest: &Path) -> String {
    dest.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Download `url` into `dest` via curl, streaming a native progress bar to
/// stderr (mirrors install.sh).
fn curl_download(url: &str, dest: &Path) -> Result<(), String> {
    let status = Command::new("curl")
        .args(["-fL", "--progress-bar", "--retry", "3", "-o"])
        .arg(dest)
        .arg(url)
        .status()
        .map_err(|e| format!("failed to run curl: {e}"))?;
    if !status.success() {
        return Err(format!("download failed: {url}"));
    }
    Ok(())
}

/// Fetch a small text resource via curl (best-effort; None on any failure).
fn curl_text(url: &str) -> Option<String> {
    let output = Command::new("curl")
        .args(["-fsSL", "--retry", "3", url])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn update() -> Result<i32, String> {
    let asset = detect_asset()?;
    let base = format!("https://github.com/{REPO}/releases/latest/download");
    let asset_url = format!("{base}/{asset}");
    let checksums_url = format!("{base}/checksums.txt");

    let dest = current_exe()?;
    let dest_name = exe_basename(&dest);
    if dest_name != NAME {
        return Err(format!(
            "refusing to self-update: current executable is \"{dest_name}\", expected \"{NAME}\". \
             self-update only works on the installed binary, not when running from source."
        ));
    }

    println!("==> Updating {NAME}");
    println!("    repo:   {REPO}");
    println!("    target: {}", dest.display());
    println!("    before: {NAME} {VERSION}");

    println!("==> Downloading {asset_url}");

    let dir = dest.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    fs::create_dir_all(&dir).ok();
    let tmp = dir.join(format!(".{NAME}.update.{}", std::process::id()));

    if let Err(e) = curl_download(&asset_url, &tmp) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    let bytes = match fs::read(&tmp) {
        Ok(b) => b,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(format!("failed to read downloaded file: {e}"));
        }
    };

    // Verify checksum if checksums.txt exists for this release (best-effort).
    if let Some(text) = curl_text(&checksums_url) {
        let needle = format!(" {asset}");
        if let Some(line) = text.lines().find(|l| l.trim_end().ends_with(&needle)) {
            let expected = line
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_lowercase();
            let actual = sha256_hex(&bytes);
            if expected != actual {
                let _ = fs::remove_file(&tmp);
                eprintln!("error: checksum mismatch (expected {expected}, got {actual})");
                return Ok(1);
            }
            println!("==> Checksum OK");
        }
    }

    if let Err(e) = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755)) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("failed to chmod downloaded file: {e}"));
    }

    // Atomic replace via rename on the same filesystem.
    if let Err(e) = fs::rename(&tmp, &dest) {
        let _ = fs::remove_file(&tmp);
        return Err(e.to_string());
    }

    println!("==> Updated: {}", dest.display());

    // Best-effort: print the new version. If the new binary cannot exec, the
    // replace already succeeded.
    if let Ok(out) = Command::new(&dest).arg("version").output() {
        if out.status.success() {
            let after = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !after.is_empty() {
                println!("    after:  {after}");
            }
        }
    }

    Ok(0)
}

pub fn uninstall() -> Result<i32, String> {
    let dest = current_exe()?;
    let dest_name = exe_basename(&dest);
    if dest_name != NAME {
        return Err(format!(
            "refusing to uninstall: current executable is \"{dest_name}\", expected \"{NAME}\". \
             uninstall only works on the installed binary, not when running from source."
        ));
    }

    println!("==> Uninstalling {NAME}");
    println!("    target: {}", dest.display());

    fs::remove_file(&dest).map_err(|e| e.to_string())?;

    println!("==> Removed: {}", dest.display());
    Ok(0)
}
