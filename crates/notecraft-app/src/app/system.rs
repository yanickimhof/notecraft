//! Opening links and files, trash helpers.

use super::*;

/// Open a URL or file with the system's default app (only when the person clicks).
/// True when `target` is safe to hand to the system: a web, mail or phone link, the macOS settings
/// panes this app points to, or an absolute file path. Other schemes (`file:`, `smb:`, custom
/// application links) and anything starting with `-` are refused, so a link inside a page can never
/// launch another program or be read as an option.
pub fn is_openable(target: &str) -> bool {
    let t = target.trim();
    if t.is_empty() || t.starts_with('-') || t.contains('\n') || t.contains('\0') {
        return false;
    }
    match t.split_once(':') {
        Some((scheme, _))
            if scheme.len() > 1 && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)) =>
        {
            matches!(
                scheme.to_ascii_lowercase().as_str(),
                "http" | "https" | "mailto" | "tel" | "x-apple.systempreferences"
            )
        }
        _ => Path::new(t).is_absolute() && !runs_a_program(Path::new(t)),
    }
}

/// Whether opening this file would start a program (an application, a script, an installer, an
/// executable) instead of showing a document. A link in a page must never do that.
pub(super) fn runs_a_program(path: &Path) -> bool {
    const PROGRAMS: [&str; 32] = [
        "app",
        "command",
        "sh",
        "bash",
        "zsh",
        "csh",
        "ksh",
        "tool",
        "terminal",
        "scpt",
        "scptd",
        "applescript",
        "workflow",
        "action",
        "wflow",
        "pkg",
        "mpkg",
        "dmg",
        "jar",
        "exe",
        "bat",
        "cmd",
        "com",
        "msi",
        "plugin",
        "prefpane",
        "saver",
        "webloc",
        "inetloc",
        "fileloc",
        "url",
        "osax",
    ];
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if PROGRAMS.contains(&ext.as_str()) {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = std::fs::metadata(path) {
            return m.is_file() && m.permissions().mode() & 0o111 != 0;
        }
    }
    false
}

/// Open a link or file with the system's default program (see [`is_openable`]).
pub fn open_external(target: &str) {
    if !is_openable(target) {
        return;
    }
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(target).spawn();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(target).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(target).spawn();
    let _ = r;
}

/// Show a file in Finder / Explorer.
/// The home folder, Desktop, Documents and the like are not workspaces: NoteCraft would fill them with its folders.
pub fn is_too_broad(path: &Path) -> bool {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else { return false };
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path == home || ["Desktop", "Documents", "Downloads"].iter().any(|d| path == home.join(d))
}

/// Move a folder into `~/.Trash`, so it can still be put back from there.
pub(super) fn move_to_trash(path: &Path) -> std::io::Result<()> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| std::io::Error::other("no home folder"))?;
    move_into(path, &home.join(".Trash"))
}

/// Move `path` into the folder `dir`; a clash in names gets a number (the time in seconds).
pub(super) fn move_into(path: &Path, dir: &Path) -> std::io::Result<()> {
    let name = path.file_name().ok_or_else(|| std::io::Error::other("no folder name"))?.to_os_string();
    let mut dest = dir.join(&name);
    if dest.exists() {
        let secs =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
        dest = dir.join(format!("{} {secs}", name.to_string_lossy()));
    }
    std::fs::rename(path, dest)
}

pub fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(not(target_os = "macos"))]
    open_external(&path.parent().unwrap_or(path).display().to_string());
}

pub fn is_image(path: &Path) -> bool {
    matches!(
        path.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
    )
}
