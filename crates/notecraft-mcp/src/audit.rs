//! A journal of every tool call, kept outside the notes folder.
//!
//! One JSON object per line in `<app data>/NoteCraft/audit/<workspace hash>.jsonl`:
//! time, tool, level (read, write, delete), outcome (ok, error, denied) and the arguments, with
//! long texts replaced by their length so the journal stays small and never duplicates a page.
//! The journal is append-only from the server's point of view and lives where a client that
//! only reaches the notes folder cannot rewrite it.

use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;

pub struct Audit {
    path: PathBuf,
}

/// Long strings and lists are shortened: the journal records what was asked, not the content.
pub fn summarize(v: &Value) -> Value {
    match v {
        Value::String(s) if s.chars().count() > 80 => json!(format!("<{} chars>", s.chars().count())),
        Value::Array(a) if a.len() > 5 => json!(format!("<{} items>", a.len())),
        Value::Array(a) => Value::Array(a.iter().map(summarize).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), summarize(x))).collect()),
        other => other.clone(),
    }
}

impl Audit {
    pub fn new(path: PathBuf) -> Audit {
        Audit { path }
    }

    /// The journal of a workspace in the app data folder.
    pub fn for_workspace(folder: &std::path::Path) -> Audit {
        let base = notecraft_core::sync::index_base_dir();
        let dir = base.parent().map(|p| p.join("audit")).unwrap_or_else(|| base.join("audit"));
        Audit::new(dir.join(format!("{}.jsonl", notecraft_core::sync::path_hash(folder))))
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Append one entry; a failure to write never blocks the tool call.
    pub fn record(&self, tool: &str, level: &str, outcome: &str, args: &Value, detail: &str) {
        let mut entry = json!({
            "time": notecraft_core::now_string(),
            "tool": tool,
            "level": level,
            "outcome": outcome,
            "args": summarize(args),
        });
        if !detail.is_empty() {
            entry["detail"] = json!(detail.chars().take(200).collect::<String>());
        }
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = writeln!(f, "{entry}");
        }
    }
}
