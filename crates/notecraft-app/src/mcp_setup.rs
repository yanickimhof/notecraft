//! Connect NoteCraft's MCP server to Claude Desktop by editing its configuration file.
//!
//! The file is only ever changed on a click, a copy is kept next to it, and a file that cannot be
//! parsed is left alone.

use serde_json::{json, Map, Value};
use std::path::PathBuf;

const KEY: &str = "notecraft";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopState {
    /// Claude Desktop does not seem to be installed.
    Missing,
    /// Installed, NoteCraft not added yet.
    NotAdded,
    /// Added, but with other settings than the ones chosen here.
    Different,
    /// Added with exactly these settings.
    Connected,
}

fn support_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/Claude"))
}

pub fn config_path() -> Option<PathBuf> {
    support_dir().map(|d| d.join("claude_desktop_config.json"))
}

fn entry(bin: &str, args: &[String]) -> Value {
    json!({ "command": bin, "args": args })
}

/// Where this configuration stands for the given server entry.
pub fn state_in(text: Option<&str>, bin: &str, args: &[String]) -> DesktopState {
    let Some(text) = text else { return DesktopState::NotAdded };
    let Ok(v) = serde_json::from_str::<Value>(text) else { return DesktopState::NotAdded };
    match v.get("mcpServers").and_then(|m| m.get(KEY)) {
        None => DesktopState::NotAdded,
        Some(e) if *e == entry(bin, args) => DesktopState::Connected,
        Some(_) => DesktopState::Different,
    }
}

pub fn state(bin: &str, args: &[String]) -> DesktopState {
    let Some(dir) = support_dir() else { return DesktopState::Missing };
    if !dir.is_dir() {
        return DesktopState::Missing;
    }
    let text = config_path().and_then(|p| std::fs::read_to_string(p).ok());
    state_in(text.as_deref(), bin, args)
}

/// The configuration text with our server added (or replaced); other servers are kept.
pub fn with_entry(existing: Option<&str>, bin: &str, args: &[String]) -> Result<String, String> {
    let mut root = match existing.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => serde_json::from_str::<Value>(t).map_err(|e| e.to_string())?,
        None => Value::Object(Map::new()),
    };
    let obj = root.as_object_mut().ok_or("not a JSON object")?;
    let servers = obj.entry("mcpServers").or_insert_with(|| Value::Object(Map::new()));
    servers.as_object_mut().ok_or("mcpServers is not an object")?.insert(KEY.into(), entry(bin, args));
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

/// The configuration text without our server; everything else is kept.
pub fn without_entry(existing: &str) -> Result<String, String> {
    let mut root: Value = serde_json::from_str(existing).map_err(|e| e.to_string())?;
    if let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
        servers.remove(KEY);
    }
    serde_json::to_string_pretty(&root).map_err(|e| e.to_string())
}

fn write_config(new_text: &str) -> Result<(), String> {
    let path = config_path().ok_or("no home folder")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if path.exists() {
        let backup = path.with_extension("json.notecraft-backup");
        std::fs::copy(&path, backup).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, new_text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub fn add(bin: &str, args: &[String]) -> Result<(), String> {
    let existing = config_path().and_then(|p| std::fs::read_to_string(p).ok());
    write_config(&with_entry(existing.as_deref(), bin, args)?)
}

pub fn remove() -> Result<(), String> {
    let Some(text) = config_path().and_then(|p| std::fs::read_to_string(p).ok()) else { return Ok(()) };
    write_config(&without_entry(&text)?)
}

/// The `claude` command line tool (Claude Code), if installed. A Finder launch has a short PATH,
/// so the usual install places are tried too.
pub fn claude_cli() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut candidates = vec![
        home.join(".local/bin/claude"),
        home.join(".claude/local/claude"),
        PathBuf::from("/opt/homebrew/bin/claude"),
        PathBuf::from("/usr/local/bin/claude"),
    ];
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|d| d.join("claude")));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Register the server with Claude Code for this user (`claude mcp add --scope user`), replacing
/// an earlier registration so the options stay current.
pub fn add_to_claude_code(bin: &str, args: &[String]) -> Result<(), String> {
    let claude = claude_cli().ok_or("claude not found")?;
    let _ = std::process::Command::new(&claude).args(["mcp", "remove", "--scope", "user", KEY]).output();
    let out = std::process::Command::new(&claude)
        .args(["mcp", "add", "--scope", "user", KEY, "--", bin])
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args() -> Vec<String> {
        vec!["--workspace".into(), "/w".into()]
    }

    #[test]
    fn adds_to_an_empty_or_missing_file() {
        for existing in [None, Some(""), Some("{}")] {
            let t = with_entry(existing, "/bin/nc", &args()).unwrap();
            assert_eq!(state_in(Some(&t), "/bin/nc", &args()), DesktopState::Connected);
        }
    }

    #[test]
    fn keeps_other_servers_and_settings() {
        let old = r#"{"theme":"x","mcpServers":{"other":{"command":"o"}}}"#;
        let t = with_entry(Some(old), "/bin/nc", &args()).unwrap();
        let v: Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["theme"], "x");
        assert_eq!(v["mcpServers"]["other"]["command"], "o");
        assert_eq!(v["mcpServers"]["notecraft"]["command"], "/bin/nc");
        let back: Value = serde_json::from_str(&without_entry(&t).unwrap()).unwrap();
        assert!(back["mcpServers"].get("notecraft").is_none());
        assert_eq!(back["mcpServers"]["other"]["command"], "o");
    }

    #[test]
    fn different_settings_are_noticed_and_broken_files_refused() {
        let t = with_entry(None, "/bin/nc", &args()).unwrap();
        assert_eq!(state_in(Some(&t), "/bin/nc", &["--read-only".to_string()]), DesktopState::Different);
        assert_eq!(state_in(None, "/bin/nc", &args()), DesktopState::NotAdded);
        assert!(with_entry(Some("{ not json"), "/bin/nc", &args()).is_err());
        assert!(with_entry(Some("[1]"), "/bin/nc", &args()).is_err());
    }
}
